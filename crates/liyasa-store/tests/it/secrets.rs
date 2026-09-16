//! The secret store (NFR-11, §30.2.4).

use liyasa_core::store::SecretStore;
use liyasa_core::verify::SecretSource;
use liyasa_store::secrets::{MasterKey, Secrets};
use sqlx::Row;
use zeroize::Zeroizing;

use crate::support::app_db;

fn value(text: &str) -> Zeroizing<String> {
    Zeroizing::new(text.to_owned())
}

#[tokio::test]
async fn a_secret_round_trips_and_survives_a_restart() {
    let (dir, pool) = app_db("secrets-round-trip").await;
    let key = MasterKey::generate().expect("a key");
    let secrets = Secrets::open(pool.clone(), key.clone())
        .await
        .expect("an empty store");
    secrets
        .set("STRIPE_KEY", value("sk_live_123"))
        .await
        .expect("a write");

    assert_eq!(
        secrets.get("STRIPE_KEY").expect("the secret").as_str(),
        "sk_live_123"
    );
    assert!(secrets.get("UNKNOWN").is_none());
    assert_eq!(secrets.names(), ["STRIPE_KEY"]);

    // Reopening decrypts from the table, not from memory.
    let reopened = Secrets::open(pool, key).await.expect("a reopen");
    assert_eq!(
        reopened.get("STRIPE_KEY").expect("the secret").as_str(),
        "sk_live_123"
    );
    drop(dir);
}

#[tokio::test]
async fn the_ciphertext_is_bound_to_the_name_it_was_stored_under() {
    let (_dir, pool) = app_db("secrets-aad").await;
    let key = MasterKey::generate().expect("a key");
    let secrets = Secrets::open(pool.clone(), key.clone())
        .await
        .expect("a store");
    secrets
        .set("LOW", value("readonly"))
        .await
        .expect("a write");
    secrets
        .set("HIGH", value("admin-token"))
        .await
        .expect("a write");

    // Moving HIGH's ciphertext onto LOW's row must not decrypt: the name is
    // the associated data.
    let row = sqlx::query("SELECT nonce, ciphertext FROM secret WHERE name = 'HIGH'")
        .fetch_one(&pool)
        .await
        .expect("a row");
    let nonce: Vec<u8> = row.try_get("nonce").expect("a nonce");
    let ciphertext: Vec<u8> = row.try_get("ciphertext").expect("a ciphertext");
    sqlx::query("UPDATE secret SET nonce = ?, ciphertext = ? WHERE name = 'LOW'")
        .bind(nonce)
        .bind(ciphertext)
        .execute(&pool)
        .await
        .expect("the tamper");

    let error = Secrets::open(pool, key)
        .await
        .expect_err("a moved ciphertext must not open");
    assert!(error.to_string().contains("does not decrypt"), "{error}");
}

#[tokio::test]
async fn rotating_the_master_key_re_encrypts_every_row_and_is_audited() {
    let (_dir, pool) = app_db("secrets-rotate").await;
    let old = MasterKey::generate().expect("a key");
    let secrets = Secrets::open(pool.clone(), old.clone())
        .await
        .expect("a store");
    secrets.set("A", value("one")).await.expect("a write");
    secrets.set("B", value("two")).await.expect("a write");

    let new = MasterKey::generate().expect("a key");
    assert_ne!(old.id(), new.id());
    assert_eq!(
        secrets
            .rotate_master(new.clone())
            .await
            .expect("a rotation"),
        2
    );

    // Every row is under the new key, and the old one no longer opens them.
    assert!(Secrets::open(pool.clone(), old).await.is_err());
    let reopened = Secrets::open(pool.clone(), new).await.expect("the new key");
    assert_eq!(reopened.get("A").expect("a secret").as_str(), "one");
    assert_eq!(reopened.get("B").expect("a secret").as_str(), "two");

    let audited: i64 =
        sqlx::query("SELECT COUNT(*) AS n FROM audit_log WHERE action = 'secrets.rotate-master'")
            .fetch_one(&pool)
            .await
            .expect("a row")
            .try_get("n")
            .expect("a count");
    assert_eq!(audited, 1);
}

#[tokio::test]
async fn a_master_key_is_thirty_two_bytes_of_hexadecimal() {
    assert!(MasterKey::from_hex("nope").is_err());
    assert!(MasterKey::from_hex(&"zz".repeat(32)).is_err());
    let key = MasterKey::from_hex(&"ab".repeat(32)).expect("a key");
    assert_eq!(key.id(), MasterKey::from_bytes([0xab; 32]).id());
}

#[tokio::test]
async fn the_conformance_kit_passes() {
    let (_dir, pool) = app_db("secrets-conformance").await;
    let secrets = Secrets::open(pool, MasterKey::generate().expect("a key"))
        .await
        .expect("a store");
    secrets
        .set("deploy_token", value("t0ken"))
        .await
        .expect("a write");
    liyasa_core::conformance::secret_source::check(&secrets, "deploy_token", "t0ken", "absent");
}
