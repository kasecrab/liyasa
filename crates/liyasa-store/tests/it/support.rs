//! A database per test, in a directory that goes away with the process.

use std::path::PathBuf;
use std::sync::atomic::{AtomicU64, Ordering};

use liyasa_store::db::{self, OpenOptions};
use sqlx::sqlite::SqlitePool;

static COUNTER: AtomicU64 = AtomicU64::new(0);

pub struct TempDir(pub PathBuf);

impl TempDir {
    pub fn new(name: &str) -> Self {
        let n = COUNTER.fetch_add(1, Ordering::SeqCst);
        let path =
            std::env::temp_dir().join(format!("liyasa-store-{name}-{}-{n}", std::process::id()));
        let _ = std::fs::remove_dir_all(&path);
        std::fs::create_dir_all(&path).expect("a test directory");
        Self(path)
    }

    pub fn join(&self, name: &str) -> PathBuf {
        self.0.join(name)
    }
}

impl Drop for TempDir {
    fn drop(&mut self) {
        let _ = std::fs::remove_dir_all(&self.0);
    }
}

/// An application database, migrated.
pub async fn app_db(name: &str) -> (TempDir, SqlitePool) {
    let dir = TempDir::new(name);
    let pool = db::open(&dir.join("liyasa.db"), &OpenOptions::default(), db::APP)
        .await
        .expect("an application database");
    (dir, pool)
}
