//! A real analytics database per test.
//!
//! Nothing here models the schema. The migration, the queue, the batch writer
//! and the hourly rollup are `liyasa-store`'s, run unmodified, so a query test
//! that passes has passed against the rows the server really writes — not
//! against this package's idea of them.

use std::path::PathBuf;
use std::sync::atomic::{AtomicU64, Ordering};

use liyasa_store::db::{self, OpenOptions};
use liyasa_store::ingest::{IngestOptions, IngestQueue, Writer};
use liyasa_store::records::EventRecord;
use sqlx::sqlite::SqlitePool;

static COUNTER: AtomicU64 = AtomicU64::new(0);

/// `2026-09-14T00:00:00Z`, so every timestamp in a test is a readable offset
/// from a known midnight.
pub const T0: i64 = 1_789_344_000_000;
pub const HOUR: i64 = 3_600_000;
pub const DAY: i64 = 86_400_000;

pub struct TempDir(pub PathBuf);

impl TempDir {
    pub fn new(name: &str) -> Self {
        let n = COUNTER.fetch_add(1, Ordering::SeqCst);
        let path = std::env::temp_dir().join(format!(
            "liyasa-analytics-{name}-{}-{n}",
            std::process::id()
        ));
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

/// An analytics database with `events` written through the real writer.
pub async fn analytics(name: &str, events: Vec<EventRecord>) -> (TempDir, Writer) {
    let dir = TempDir::new(name);
    let queue = IngestQueue::new(10_000, 1_000);
    let mut writer = Writer::open(
        &dir.join("analytics.db"),
        queue.clone(),
        IngestOptions::default(),
    )
    .await
    .expect("an analytics database");
    for event in events {
        queue.push(event).expect("room in the queue");
    }
    while writer.flush().await.expect("a flush") > 0 {}
    (dir, writer)
}

/// An application database, which is where feedback lives.
pub async fn app(name: &str) -> (TempDir, SqlitePool) {
    let dir = TempDir::new(name);
    let pool = db::open(&dir.join("liyasa.db"), &OpenOptions::default(), db::APP)
        .await
        .expect("an application database");
    (dir, pool)
}

/// One event, with the fields a query cares about and the server's defaults
/// for the rest.
pub struct Event {
    inner: EventRecord,
}

impl Event {
    pub fn new(kind: &str, route: &str, at: i64) -> Self {
        Self {
            inner: EventRecord {
                ts: at,
                site: "acme-docs".to_owned(),
                env: "production".to_owned(),
                route: route.to_owned(),
                kind: kind.to_owned(),
                caller: serde_json::json!({ "kind": "human", "agent_name": null }),
                format: "html".to_owned(),
                session_key: "k1:00000000000000000000000000000001".to_owned(),
                device: serde_json::json!({ "class": "desktop" }),
                ..EventRecord::default()
            },
        }
    }

    pub fn caller(mut self, kind: &str) -> Self {
        self.inner.caller = serde_json::json!({ "kind": kind, "agent_name": null });
        self
    }

    pub fn agent(mut self, name: &str) -> Self {
        self.inner.caller = serde_json::json!({ "kind": "agent", "agent_name": name });
        self
    }

    pub fn session(mut self, n: u32) -> Self {
        self.inner.session_key = format!("k1:{n:032}");
        self
    }

    pub fn variant(mut self, variant: serde_json::Value) -> Self {
        self.inner.variant = variant;
        self
    }

    pub fn referrer(mut self, host: &str) -> Self {
        self.inner.referrer_host = Some(host.to_owned());
        self
    }

    pub fn props(mut self, props: serde_json::Value) -> Self {
        self.inner.props = props;
        self
    }

    pub fn format(mut self, format: &str) -> Self {
        self.inner.format = format.to_owned();
        self
    }

    pub fn site(mut self, site: &str) -> Self {
        self.inner.site = site.to_owned();
        self
    }

    pub fn build(self) -> EventRecord {
        self.inner
    }
}
