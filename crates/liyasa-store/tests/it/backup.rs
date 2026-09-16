//! HOST-06: `liyasa backup` exports, `liyasa restore` rebuilds.

use liyasa_core::store::SecretStore;
use liyasa_store::backup::{self, BackupError, ObjectRef};
use liyasa_store::secrets::{MasterKey, Secrets};
use liyasa_store::{IngestQueue, MasterKey as Key, SqliteStore};
use sqlx::Row;
use zeroize::Zeroizing;

use crate::support::TempDir;

async fn instance(name: &str, key: MasterKey) -> (TempDir, SqliteStore) {
    let dir = TempDir::new(name);
    let store = SqliteStore::open(&dir.join("liyasa.db"), key, IngestQueue::new(64, 16))
        .await
        .expect("a store");
    (dir, store)
}

#[tokio::test]
async fn an_archive_round_trips_a_whole_instance() {
    let key = Key::generate().expect("a key");
    let (dir, store) = instance("backup-source", key.clone()).await;
    let project = store
        .projects_typed()
        .create("acme-docs", "Acme")
        .await
        .expect("a project");
    store
        .secrets_typed()
        .set("STRIPE_KEY", Zeroizing::new("sk_live_123".to_owned()))
        .await
        .expect("a secret");

    let objects = vec![ObjectRef {
        kind: "bundle".to_owned(),
        key: "builds/blake3-abc/dist.tar".to_owned(),
    }];
    let archive = dir.join("backup.tar");
    let manifest = backup::export(store.pool(), None, &objects, &archive)
        .await
        .expect("an export");

    assert_eq!(manifest.format_version, backup::FORMAT_VERSION);
    assert_eq!(manifest.projects, 1);
    assert_eq!(manifest.secrets, 1);
    assert_eq!(manifest.secret_key_id.as_deref(), Some(key.id()));
    assert!(!manifest.includes_analytics);

    // Restore into a fresh directory and open it as an instance.
    let into = TempDir::new("backup-restored");
    let restored = backup::restore(&archive, &into.0, Some(key.id())).expect("a restore");
    assert_eq!(restored.objects, objects);
    assert_eq!(restored.manifest.created_at, manifest.created_at);

    let rebuilt = SqliteStore::open(&into.join("liyasa.db"), key, IngestQueue::new(64, 16))
        .await
        .expect("the restored instance opens");
    assert_eq!(
        rebuilt
            .projects_typed()
            .by_slug("acme-docs")
            .await
            .expect("a read")
            .expect("the project")
            .id,
        project.id
    );
    use liyasa_core::verify::SecretSource as _;
    assert_eq!(
        rebuilt
            .secrets_typed()
            .get("STRIPE_KEY")
            .expect("the secret")
            .as_str(),
        "sk_live_123",
        "the secrets travel encrypted and open again under the same key"
    );
}

#[tokio::test]
async fn a_restore_into_an_instance_with_another_master_key_is_refused() {
    let key = Key::generate().expect("a key");
    let (dir, store) = instance("backup-wrongkey", key.clone()).await;
    store
        .secrets_typed()
        .set("A", Zeroizing::new("one".to_owned()))
        .await
        .expect("a secret");
    let archive = dir.join("backup.tar");
    backup::export(store.pool(), None, &[], &archive)
        .await
        .expect("an export");

    let other = Key::generate().expect("another key");
    let into = TempDir::new("backup-wrongkey-into");
    let error = backup::restore(&archive, &into.0, Some(other.id()))
        .expect_err("a restore under the wrong key must be refused");
    assert!(matches!(error, BackupError::WrongKey { .. }), "{error}");
    assert!(
        !into.join("liyasa.db").exists(),
        "a refused restore leaves nothing half-written"
    );
}

#[tokio::test]
async fn the_analytics_database_travels_when_it_is_asked_for() {
    let key = Key::generate().expect("a key");
    let (dir, store) = instance("backup-analytics", key.clone()).await;
    let analytics = liyasa_store::db::open(
        &dir.join("analytics.db"),
        &liyasa_store::db::OpenOptions::default(),
        liyasa_store::db::ANALYTICS,
    )
    .await
    .expect("an analytics database");

    let archive = dir.join("backup.tar");
    let manifest = backup::export(store.pool(), Some(&analytics), &[], &archive)
        .await
        .expect("an export");
    assert!(manifest.includes_analytics);

    let into = TempDir::new("backup-analytics-into");
    backup::restore(&archive, &into.0, Some(key.id())).expect("a restore");
    assert!(into.join("analytics.db").exists());
    assert!(into.join("liyasa.db").exists());
}

#[tokio::test]
async fn the_snapshot_is_consistent_while_the_instance_keeps_writing() {
    let key = Key::generate().expect("a key");
    let (dir, store) = instance("backup-live", key.clone()).await;
    for n in 0..20 {
        store
            .projects_typed()
            .create(&format!("p{n}"), "P")
            .await
            .expect("a project");
    }

    // A writer runs across the export, which is the case HOST-06 is about:
    // the backup must not need the server stopped.
    let writer = {
        let store = store.clone_pool();
        tokio::spawn(async move {
            for n in 20..40 {
                let _ = sqlx::query(
                    "INSERT INTO project (id, slug, name, created_at, updated_at, version) \
                     VALUES (?, ?, 'P', 0, 0, 1)",
                )
                .bind(liyasa_store::new_ulid().to_string())
                .bind(format!("p{n}"))
                .execute(&store)
                .await;
            }
        })
    };
    let archive = dir.join("backup.tar");
    let manifest = backup::export(store.pool(), None, &[], &archive)
        .await
        .expect("an export during writes");
    writer.await.expect("the writer finished");

    let into = TempDir::new("backup-live-into");
    backup::restore(&archive, &into.0, Some(key.id())).expect("a restore");
    let rebuilt = liyasa_store::db::open(
        &into.join("liyasa.db"),
        &liyasa_store::db::OpenOptions::default(),
        liyasa_store::db::APP,
    )
    .await
    .expect("the snapshot opens");
    let counted: i64 = sqlx::query("SELECT COUNT(*) AS n FROM project")
        .fetch_one(&rebuilt)
        .await
        .expect("a row")
        .try_get("n")
        .expect("a count");
    assert!(
        counted >= manifest.projects as i64,
        "the snapshot held at least what the manifest counted"
    );
    assert!((20..=40).contains(&counted), "counted {counted}");
}

#[tokio::test]
async fn the_archive_is_a_tar_the_system_tool_can_read() {
    let key = Key::generate().expect("a key");
    let (dir, store) = instance("backup-tar", key).await;
    let archive = dir.join("backup.tar");
    backup::export(store.pool(), None, &[], &archive)
        .await
        .expect("an export");

    let Ok(output) = std::process::Command::new("tar")
        .arg("tf")
        .arg(&archive)
        .output()
    else {
        // A machine without tar still has the reader above; being readable by
        // the standard tool is the claim this test makes when it can.
        return;
    };
    assert!(
        output.status.success(),
        "tar refused the archive: {}",
        String::from_utf8_lossy(&output.stderr)
    );
    let listed = String::from_utf8_lossy(&output.stdout);
    for name in [backup::MANIFEST, backup::OBJECTS, backup::APP_DB] {
        assert!(listed.contains(name), "tar did not list `{name}`: {listed}");
    }
}

#[tokio::test]
async fn a_secretless_instance_archives_and_restores_anywhere() {
    let key = Key::generate().expect("a key");
    let (dir, store) = instance("backup-nosecrets", key).await;
    let archive = dir.join("backup.tar");
    let manifest = backup::export(store.pool(), None, &[], &archive)
        .await
        .expect("an export");
    assert_eq!(manifest.secret_key_id, None);

    let into = TempDir::new("backup-nosecrets-into");
    // No secrets means no key to match, so any instance may restore it.
    let other = Secrets::open(
        liyasa_store::db::open(
            &into.join("scratch.db"),
            &liyasa_store::db::OpenOptions::default(),
            liyasa_store::db::APP,
        )
        .await
        .expect("a database"),
        MasterKey::generate().expect("a key"),
    )
    .await
    .expect("a store");
    let _ = other;
    backup::restore(&archive, &into.0, Some("a-different-key")).expect("a restore");
    assert!(into.join("liyasa.db").exists());
}
