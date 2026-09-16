//! The `Store` contract over SQLite (PRD §6.8, §34.9).
//!
//! `liyasa-store` re-exports the contracts and is the only crate that opens a
//! database. The entity structs in `liyasa-core` carry no fields yet, so the
//! records the server reads and writes are declared here (RFC 1400).

pub mod backup;
pub mod db;
pub mod facade;
pub mod ingest;
pub mod jobs;
pub mod records;
pub mod repos;
pub mod secrets;

pub use backup::{BackupError, BackupManifest, ObjectRef};
pub use facade::SqliteStore;
pub use ingest::{IngestOptions, IngestQueue, RawSink, Writer};
pub use jobs::{Enqueue, Enqueued, Jobs};
pub use liyasa_core::store::*;
pub use secrets::{MasterKey, Secrets};

/// Milliseconds since the Unix epoch, the timestamp every table stores.
pub fn now_ms() -> i64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_millis() as i64)
        .unwrap_or(0)
}

/// A fresh ULID; the server mints IDs, core never does.
pub fn new_ulid() -> ulid::Ulid {
    ulid::Ulid::generate()
}
