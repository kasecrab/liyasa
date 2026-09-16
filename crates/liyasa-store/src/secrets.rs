//! The secret store (NFR-11, §30.2.4).
//!
//! Values are AES-256-GCM under a per-instance master key with the secret's
//! name as associated data, so a ciphertext moved to another row does not
//! decrypt. `SecretSource::get` is synchronous, so the decrypted values live
//! in memory behind a lock and the table is the durable copy.

use std::collections::HashMap;
use std::sync::RwLock;

use aes_gcm::aead::{Aead, KeyInit, Payload};
use aes_gcm::{Aes256Gcm, Nonce};
use liyasa_core::net::BoxFut;
use liyasa_core::store::{SecretStore, StoreError};
use liyasa_core::verify::SecretSource;
use sqlx::Row;
use sqlx::sqlite::SqlitePool;
use zeroize::Zeroizing;

use crate::db::sql_error;
use crate::now_ms;

/// 32 bytes, from `LIYASA_MASTER_KEY` (64 hex characters) or a key file.
#[derive(Clone)]
pub struct MasterKey {
    id: String,
    bytes: Zeroizing<[u8; 32]>,
}

impl std::fmt::Debug for MasterKey {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "MasterKey({})", self.id)
    }
}

impl MasterKey {
    pub fn from_bytes(bytes: [u8; 32]) -> Self {
        let digest = liyasa_core::ids::Fingerprint::of(bytes).to_hex();
        Self {
            id: digest[..16].to_owned(),
            bytes: Zeroizing::new(bytes),
        }
    }

    pub fn from_hex(text: &str) -> Result<Self, StoreError> {
        let text = text.trim();
        if text.len() != 64 {
            return Err(StoreError::Io(
                "master key must be 64 hexadecimal characters".to_owned(),
            ));
        }
        let mut bytes = [0u8; 32];
        for (i, byte) in bytes.iter_mut().enumerate() {
            *byte = u8::from_str_radix(&text[i * 2..i * 2 + 2], 16)
                .map_err(|_| StoreError::Io("master key is not hexadecimal".to_owned()))?;
        }
        Ok(Self::from_bytes(bytes))
    }

    pub fn generate() -> Result<Self, StoreError> {
        let mut bytes = [0u8; 32];
        getrandom::fill(&mut bytes).map_err(|e| StoreError::Io(e.to_string()))?;
        Ok(Self::from_bytes(bytes))
    }

    /// A short digest of the key, stored beside each ciphertext so a rotation
    /// knows which rows still need re-encrypting.
    pub fn id(&self) -> &str {
        &self.id
    }

    fn cipher(&self) -> Aes256Gcm {
        Aes256Gcm::new(&(*self.bytes).into())
    }

    fn seal(&self, name: &str, value: &[u8]) -> Result<(Vec<u8>, Vec<u8>), StoreError> {
        let mut nonce = [0u8; 12];
        getrandom::fill(&mut nonce).map_err(|e| StoreError::Io(e.to_string()))?;
        let ciphertext = self
            .cipher()
            .encrypt(
                &Nonce::from(nonce),
                Payload {
                    msg: value,
                    aad: name.as_bytes(),
                },
            )
            .map_err(|_| StoreError::Io("encryption failed".to_owned()))?;
        Ok((nonce.to_vec(), ciphertext))
    }

    fn open(
        &self,
        name: &str,
        nonce: &[u8],
        ciphertext: &[u8],
    ) -> Result<Zeroizing<String>, StoreError> {
        let nonce: [u8; 12] = nonce
            .try_into()
            .map_err(|_| StoreError::Io(format!("secret `{name}` has a malformed nonce")))?;
        let plain = self
            .cipher()
            .decrypt(
                &Nonce::from(nonce),
                Payload {
                    msg: ciphertext,
                    aad: name.as_bytes(),
                },
            )
            .map_err(|_| {
                StoreError::Io(format!("secret `{name}` does not decrypt under this key"))
            })?;
        String::from_utf8(plain)
            .map(Zeroizing::new)
            .map_err(|_| StoreError::Io(format!("secret `{name}` is not UTF-8")))
    }
}

pub struct Secrets {
    pool: SqlitePool,
    key: RwLock<MasterKey>,
    cache: RwLock<HashMap<String, Zeroizing<String>>>,
}

impl std::fmt::Debug for Secrets {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("Secrets").finish_non_exhaustive()
    }
}

impl Secrets {
    /// Loads and decrypts every row; a row under a key this instance does
    /// not hold is an error, never a silently missing secret.
    pub async fn open(pool: SqlitePool, key: MasterKey) -> Result<Self, StoreError> {
        let rows = sqlx::query("SELECT name, key_id, nonce, ciphertext FROM secret")
            .fetch_all(&pool)
            .await
            .map_err(sql_error)?;
        let mut cache = HashMap::new();
        for row in rows {
            let name: String = row.try_get("name").map_err(sql_error)?;
            let key_id: String = row.try_get("key_id").map_err(sql_error)?;
            if key_id != key.id() {
                return Err(StoreError::Io(format!(
                    "secret `{name}` is encrypted under key {key_id}, not the configured {}",
                    key.id()
                )));
            }
            let nonce: Vec<u8> = row.try_get("nonce").map_err(sql_error)?;
            let ciphertext: Vec<u8> = row.try_get("ciphertext").map_err(sql_error)?;
            cache.insert(name.clone(), key.open(&name, &nonce, &ciphertext)?);
        }
        Ok(Self {
            pool,
            key: RwLock::new(key),
            cache: RwLock::new(cache),
        })
    }

    pub fn names(&self) -> Vec<String> {
        let mut names: Vec<String> = self
            .cache
            .read()
            .unwrap_or_else(|e| e.into_inner())
            .keys()
            .cloned()
            .collect();
        names.sort();
        names
    }

    pub async fn remove(&self, name: &str) -> Result<(), StoreError> {
        sqlx::query("DELETE FROM secret WHERE name = ?")
            .bind(name)
            .execute(&self.pool)
            .await
            .map_err(sql_error)?;
        self.cache
            .write()
            .unwrap_or_else(|e| e.into_inner())
            .remove(name);
        Ok(())
    }

    /// NFR-11: re-encrypts every row under `new` in one transaction and only
    /// then retires the old key.
    pub async fn rotate_master(&self, new: MasterKey) -> Result<usize, StoreError> {
        let values: Vec<(String, Zeroizing<String>)> = self
            .cache
            .read()
            .unwrap_or_else(|e| e.into_inner())
            .iter()
            .map(|(k, v)| (k.clone(), v.clone()))
            .collect();
        let mut tx = self.pool.begin().await.map_err(sql_error)?;
        let now = now_ms();
        for (name, value) in &values {
            let (nonce, ciphertext) = new.seal(name, value.as_bytes())?;
            sqlx::query("UPDATE secret SET key_id = ?, nonce = ?, ciphertext = ?, updated_at = ? WHERE name = ?")
                .bind(new.id())
                .bind(nonce)
                .bind(ciphertext)
                .bind(now)
                .bind(name)
                .execute(&mut *tx)
                .await
                .map_err(sql_error)?;
        }
        sqlx::query("INSERT INTO audit_log (at, actor, action, subject, detail) VALUES (?, 'system', 'secrets.rotate-master', ?, ?)")
            .bind(now)
            .bind(new.id())
            .bind(format!("{} secrets re-encrypted", values.len()))
            .execute(&mut *tx)
            .await
            .map_err(sql_error)?;
        tx.commit().await.map_err(sql_error)?;
        *self.key.write().unwrap_or_else(|e| e.into_inner()) = new;
        Ok(values.len())
    }
}

impl SecretSource for Secrets {
    fn get(&self, name: &str) -> Option<Zeroizing<String>> {
        self.cache
            .read()
            .unwrap_or_else(|e| e.into_inner())
            .get(name)
            .cloned()
    }
}

impl SecretStore for Secrets {
    fn set<'a>(
        &'a self,
        name: &'a str,
        value: Zeroizing<String>,
    ) -> BoxFut<'a, Result<(), StoreError>> {
        Box::pin(async move {
            let (key_id, sealed) = {
                let key = self.key.read().unwrap_or_else(|e| e.into_inner());
                (key.id().to_owned(), key.seal(name, value.as_bytes())?)
            };
            let now = now_ms();
            sqlx::query(
                "INSERT INTO secret (name, key_id, nonce, ciphertext, created_at, updated_at) \
                 VALUES (?, ?, ?, ?, ?, ?) ON CONFLICT (name) DO UPDATE SET key_id = excluded.key_id, \
                 nonce = excluded.nonce, ciphertext = excluded.ciphertext, updated_at = excluded.updated_at",
            )
            .bind(name)
            .bind(key_id)
            .bind(sealed.0)
            .bind(sealed.1)
            .bind(now)
            .bind(now)
            .execute(&self.pool)
            .await
            .map_err(sql_error)?;
            self.cache
                .write()
                .unwrap_or_else(|e| e.into_inner())
                .insert(name.to_owned(), value);
            Ok(())
        })
    }
}
