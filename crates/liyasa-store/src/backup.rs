//! `liyasa backup` and `liyasa restore` (HOST-06).
//!
//! One archive holds the application database, the analytics database, the
//! object-storage references, and the secrets. The secrets travel as the
//! ciphertext already in the table, so an archive is readable only with the
//! master key that wrote it and nothing is ever re-encrypted into a weaker
//! form on the way out.
//!
//! The container is uncompressed ustar, so an operator can list an archive
//! with `tar tf` and take one file out of it without Liyasa.

use std::path::Path;

use liyasa_core::store::StoreError;
use serde::{Deserialize, Serialize};
use sqlx::Row;
use sqlx::sqlite::SqlitePool;

use crate::db::sql_error;

pub const FORMAT_VERSION: u32 = 1;
pub const MANIFEST: &str = "liyasa-backup.json";
pub const APP_DB: &str = "liyasa.db";
pub const ANALYTICS_DB: &str = "analytics.db";
pub const OBJECTS: &str = "objects.json";

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct BackupManifest {
    pub format_version: u32,
    pub liyasa_version: String,
    pub created_at: i64,
    /// The master key the secrets in this archive are encrypted under. A
    /// restore refuses an archive whose key the instance does not hold,
    /// rather than silently restoring secrets it cannot read.
    pub secret_key_id: Option<String>,
    pub secrets: usize,
    pub projects: usize,
    pub builds: usize,
    pub includes_analytics: bool,
}

/// A reference to something in object storage; the bytes stay where they are,
/// which is what makes a backup small enough to take often (HOST-06).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ObjectRef {
    pub kind: String,
    pub key: String,
}

#[derive(Debug, thiserror::Error)]
pub enum BackupError {
    #[error("{0}: {1}")]
    Io(String, String),
    #[error(transparent)]
    Store(#[from] StoreError),
    #[error("not a Liyasa backup: {0}")]
    Malformed(String),
    #[error("this archive is format version {found}; this build reads {expected}")]
    Version { found: u32, expected: u32 },
    #[error(
        "the archive's secrets are encrypted under key {archive}, and this instance holds {local}"
    )]
    WrongKey { archive: String, local: String },
}

fn io(path: &Path) -> impl Fn(std::io::Error) -> BackupError + '_ {
    move |e| BackupError::Io(path.display().to_string(), e.to_string())
}

// ---- a minimal ustar writer and reader ----

const BLOCK: usize = 512;

fn header(name: &str, size: usize) -> Result<[u8; BLOCK], BackupError> {
    if name.len() > 99 {
        return Err(BackupError::Malformed(format!("`{name}` is too long")));
    }
    let mut block = [0u8; BLOCK];
    block[..name.len()].copy_from_slice(name.as_bytes());
    let write = |block: &mut [u8; BLOCK], at: usize, len: usize, value: &str| {
        block[at..at + value.len().min(len)]
            .copy_from_slice(&value.as_bytes()[..value.len().min(len)]);
    };
    write(&mut block, 100, 8, "0000644\0");
    write(&mut block, 108, 8, "0000000\0");
    write(&mut block, 116, 8, "0000000\0");
    write(&mut block, 124, 12, &format!("{size:011o}\0"));
    write(
        &mut block,
        136,
        12,
        &format!("{:011o}\0", crate::now_ms() / 1000),
    );
    block[156] = b'0';
    // `ustar\0` then the version `00`, which is the POSIX magic.
    write(&mut block, 257, 8, "ustar\x0000");
    // The checksum is computed with its own field read as spaces.
    block[148..156].fill(b' ');
    let sum: u32 = block.iter().map(|b| *b as u32).sum();
    write(&mut block, 148, 8, &format!("{sum:06o}\0 "));
    Ok(block)
}

fn append(out: &mut Vec<u8>, name: &str, bytes: &[u8]) -> Result<(), BackupError> {
    out.extend_from_slice(&header(name, bytes.len())?);
    out.extend_from_slice(bytes);
    let padding = (BLOCK - bytes.len() % BLOCK) % BLOCK;
    out.extend(std::iter::repeat_n(0u8, padding));
    Ok(())
}

/// Reads every entry of an archive into memory. A backup is small by
/// construction: it holds databases and references, never the objects.
pub fn entries(archive: &[u8]) -> Result<Vec<(String, Vec<u8>)>, BackupError> {
    let mut out = Vec::new();
    let mut at = 0;
    while at + BLOCK <= archive.len() {
        let block = &archive[at..at + BLOCK];
        if block.iter().all(|b| *b == 0) {
            break;
        }
        let end = block[..100].iter().position(|b| *b == 0).unwrap_or(100);
        let name = String::from_utf8_lossy(&block[..end]).into_owned();
        let size_field = String::from_utf8_lossy(&block[124..136]);
        let size = usize::from_str_radix(size_field.trim_matches(['\0', ' ']), 8)
            .map_err(|_| BackupError::Malformed(format!("`{name}` has no size")))?;
        at += BLOCK;
        if at + size > archive.len() {
            return Err(BackupError::Malformed(format!("`{name}` is truncated")));
        }
        out.push((name, archive[at..at + size].to_vec()));
        at += size + (BLOCK - size % BLOCK) % BLOCK;
    }
    Ok(out)
}

// ---- export ----

/// Copies a live SQLite database consistently. `VACUUM INTO` takes its own
/// read transaction, so the server keeps serving while it runs.
async fn snapshot(pool: &SqlitePool, into: &Path) -> Result<Vec<u8>, BackupError> {
    let _ = std::fs::remove_file(into);
    let target = into.display().to_string().replace('\'', "''");
    sqlx::query(sqlx::AssertSqlSafe(format!("VACUUM INTO '{target}'")))
        .execute(pool)
        .await
        .map_err(|e| BackupError::Store(sql_error(e)))?;
    let bytes = std::fs::read(into).map_err(io(into))?;
    let _ = std::fs::remove_file(into);
    Ok(bytes)
}

async fn count(pool: &SqlitePool, table: &str) -> Result<usize, BackupError> {
    let row = sqlx::query(sqlx::AssertSqlSafe(format!(
        "SELECT COUNT(*) AS n FROM {table}"
    )))
    .fetch_one(pool)
    .await
    .map_err(|e| BackupError::Store(sql_error(e)))?;
    Ok(row
        .try_get::<i64, _>("n")
        .map_err(|e| BackupError::Store(sql_error(e)))? as usize)
}

/// Writes an archive of `store` to `out`. `analytics` is optional: a
/// deployment that streams raw events elsewhere does not need them here.
pub async fn export(
    app: &SqlitePool,
    analytics: Option<&SqlitePool>,
    objects: &[ObjectRef],
    out: &Path,
) -> Result<BackupManifest, BackupError> {
    let scratch = out
        .parent()
        .map(Path::to_owned)
        .unwrap_or_else(|| std::path::PathBuf::from("."));

    let key_id: Option<String> = sqlx::query("SELECT key_id FROM secret LIMIT 1")
        .fetch_optional(app)
        .await
        .map_err(|e| BackupError::Store(sql_error(e)))?
        .map(|row| row.try_get("key_id"))
        .transpose()
        .map_err(|e| BackupError::Store(sql_error(e)))?;

    let manifest = BackupManifest {
        format_version: FORMAT_VERSION,
        liyasa_version: env!("CARGO_PKG_VERSION").to_owned(),
        created_at: crate::now_ms(),
        secret_key_id: key_id,
        secrets: count(app, "secret").await?,
        projects: count(app, "project").await?,
        builds: count(app, "build").await?,
        includes_analytics: analytics.is_some(),
    };

    let mut archive = Vec::new();
    append(
        &mut archive,
        MANIFEST,
        serde_json::to_vec_pretty(&manifest)
            .map_err(|e| BackupError::Malformed(e.to_string()))?
            .as_slice(),
    )?;
    append(
        &mut archive,
        OBJECTS,
        serde_json::to_vec_pretty(objects)
            .map_err(|e| BackupError::Malformed(e.to_string()))?
            .as_slice(),
    )?;
    append(
        &mut archive,
        APP_DB,
        &snapshot(app, &scratch.join("liyasa-backup-app.tmp")).await?,
    )?;
    if let Some(analytics) = analytics {
        append(
            &mut archive,
            ANALYTICS_DB,
            &snapshot(analytics, &scratch.join("liyasa-backup-analytics.tmp")).await?,
        )?;
    }
    // Two zero blocks end a tar.
    archive.extend(std::iter::repeat_n(0u8, BLOCK * 2));
    std::fs::write(out, &archive).map_err(io(out))?;
    Ok(manifest)
}

// ---- restore ----

/// What a restore produced.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Restored {
    pub manifest: BackupManifest,
    pub objects: Vec<ObjectRef>,
}

/// Rebuilds an instance from an archive: the databases are written beside
/// each other in `into`, and the object references are handed back so the
/// caller can check they still resolve.
///
/// `local_key_id` is this instance's master key. A restore into an instance
/// with a different key is refused rather than leaving secrets that will not
/// decrypt.
pub fn restore(
    archive: &Path,
    into: &Path,
    local_key_id: Option<&str>,
) -> Result<Restored, BackupError> {
    let bytes = std::fs::read(archive).map_err(io(archive))?;
    let entries = entries(&bytes)?;
    let find = |name: &str| entries.iter().find(|(n, _)| n == name).map(|(_, b)| b);

    let manifest: BackupManifest = serde_json::from_slice(
        find(MANIFEST).ok_or_else(|| BackupError::Malformed(format!("no {MANIFEST}")))?,
    )
    .map_err(|e| BackupError::Malformed(e.to_string()))?;
    if manifest.format_version != FORMAT_VERSION {
        return Err(BackupError::Version {
            found: manifest.format_version,
            expected: FORMAT_VERSION,
        });
    }
    if let (Some(archive_key), Some(local)) = (&manifest.secret_key_id, local_key_id)
        && archive_key != local
    {
        return Err(BackupError::WrongKey {
            archive: archive_key.clone(),
            local: local.to_owned(),
        });
    }

    let objects: Vec<ObjectRef> = match find(OBJECTS) {
        Some(bytes) => {
            serde_json::from_slice(bytes).map_err(|e| BackupError::Malformed(e.to_string()))?
        }
        None => Vec::new(),
    };

    std::fs::create_dir_all(into).map_err(io(into))?;
    for name in [APP_DB, ANALYTICS_DB] {
        if let Some(bytes) = find(name) {
            let path = into.join(name);
            std::fs::write(&path, bytes).map_err(io(&path))?;
        }
    }
    Ok(Restored { manifest, objects })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn an_archive_round_trips_through_the_tar_container() {
        let mut archive = Vec::new();
        append(&mut archive, "a.json", b"{\"x\":1}").expect("an entry");
        append(&mut archive, "b.bin", &vec![7u8; 1500]).expect("an entry");
        archive.extend(std::iter::repeat_n(0u8, BLOCK * 2));

        let entries = entries(&archive).expect("a read");
        assert_eq!(entries.len(), 2);
        assert_eq!(entries[0].0, "a.json");
        assert_eq!(entries[0].1, b"{\"x\":1}");
        assert_eq!(entries[1].0, "b.bin");
        assert_eq!(entries[1].1.len(), 1500);
        // Every entry starts on a block boundary, which is what lets `tar`
        // read what we wrote.
        assert_eq!(archive.len() % BLOCK, 0);
    }

    #[test]
    fn a_header_carries_a_checksum_tar_will_accept() {
        let block = header("liyasa.db", 4096).expect("a header");
        let stored = String::from_utf8_lossy(&block[148..156]);
        let stored = u32::from_str_radix(stored.trim_matches(['\0', ' ']), 8).expect("octal");
        let mut recomputed = block;
        recomputed[148..156].fill(b' ');
        let sum: u32 = recomputed.iter().map(|b| *b as u32).sum();
        assert_eq!(stored, sum);
        assert_eq!(&block[257..262], b"ustar");
        assert_eq!(block[156], b'0', "a regular file");
    }

    #[test]
    fn a_truncated_archive_is_refused_rather_than_half_read() {
        let mut archive = Vec::new();
        append(&mut archive, "a.bin", &vec![1u8; 1024]).expect("an entry");
        archive.truncate(BLOCK + 100);
        assert!(matches!(entries(&archive), Err(BackupError::Malformed(_))));
    }

    #[test]
    fn a_name_too_long_for_ustar_is_refused() {
        assert!(header(&"x".repeat(120), 1).is_err());
    }
}
