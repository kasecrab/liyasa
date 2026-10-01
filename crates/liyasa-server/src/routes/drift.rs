//! A persistent `RecordStore` (VER-23, VER-77).
//!
//! The server is the first place the records half can be honest: `Engine::record`
//! reports `created`/`updated`/`resolved`, and those counts mean nothing against
//! a store that does not survive the run — WP-09 left this half here for exactly
//! that reason, on the grounds that a ledger which never closes anything is
//! worse than no ledger, because it looks like one.
//!
//! **The trait is synchronous and the pool is not.** `RecordStore`'s methods
//! return values rather than futures, so this bridges with `block_in_place` plus
//! the current runtime's `block_on`, which requires a multi-threaded runtime —
//! the binary's is, and tests need `#[tokio::test(flavor = "multi_thread")]`.
//! The alternative considered and rejected was a write queue drained by a task:
//! it makes `save` non-blocking at the cost of `Ok(())` no longer meaning the
//! record is durable, and durability is the whole reason this store exists.

use std::sync::Arc;

use liyasa_core::store::StoreError;
use liyasa_store::drift::{DriftRecords, StoredRecord};
use liyasa_verify::drift::record::{DriftKey, DriftRecord};
use liyasa_verify::drift::store::RecordStore;

/// Drift records on the application database.
#[derive(Debug, Clone)]
pub struct SqliteDrift {
    rows: Arc<DriftRecords>,
}

impl SqliteDrift {
    pub fn new(pool: liyasa_store::SqlitePool) -> Self {
        Self {
            rows: Arc::new(DriftRecords::new(pool)),
        }
    }

    /// Runs one async call on the current runtime.
    ///
    /// `block_in_place` moves this thread out of the async pool first, so a
    /// blocking wait here cannot starve the worker's other tasks.
    fn blocking<T>(
        &self,
        f: impl std::future::Future<Output = Result<T, StoreError>>,
    ) -> Result<T, StoreError> {
        tokio::task::block_in_place(|| tokio::runtime::Handle::current().block_on(f))
    }

    /// The document is the record's JSON. A key that does not deserialise is
    /// an error rather than a skipped row: a record written by a newer build
    /// and silently dropped here would read as "this drift is resolved".
    fn decode(stored: &StoredRecord) -> Result<DriftRecord, StoreError> {
        serde_json::from_str(&stored.document).map_err(|e| {
            StoreError::Io(format!(
                "the stored drift record `{}` does not read on this build: {e}",
                stored.key
            ))
        })
    }

    fn encode(record: &DriftRecord) -> Result<StoredRecord, StoreError> {
        Ok(StoredRecord {
            key: serde_json::to_string(&record.key())
                .map_err(|e| StoreError::Io(format!("a drift key did not serialise: {e}")))?,
            open: record.is_open(),
            document: serde_json::to_string(record)
                .map_err(|e| StoreError::Io(format!("a drift record did not serialise: {e}")))?,
        })
    }
}

impl RecordStore for SqliteDrift {
    fn all(&self) -> Result<Vec<DriftRecord>, StoreError> {
        let stored = self.blocking(self.rows.all())?;
        stored.iter().map(Self::decode).collect()
    }

    fn find(&self, key: &DriftKey) -> Result<Option<DriftRecord>, StoreError> {
        let encoded = serde_json::to_string(key)
            .map_err(|e| StoreError::Io(format!("a drift key did not serialise: {e}")))?;
        match self.blocking(self.rows.find(&encoded))? {
            Some(stored) => Ok(Some(Self::decode(&stored)?)),
            None => Ok(None),
        }
    }

    fn save(&self, record: &DriftRecord) -> Result<(), StoreError> {
        self.save_all(std::slice::from_ref(record))
    }

    /// One reconciliation, one transaction — which is what the batch seam on
    /// the trait exists for. The default body would make a commit per record.
    fn save_all(&self, records: &[DriftRecord]) -> Result<(), StoreError> {
        let encoded = records
            .iter()
            .map(Self::encode)
            .collect::<Result<Vec<_>, _>>()?;
        self.blocking(self.rows.save_all(&encoded))
    }

    /// Filtered in SQL rather than by reading every record this site has ever
    /// had and discarding the closed ones.
    fn open_records(&self) -> Result<Vec<DriftRecord>, StoreError> {
        let stored = self.blocking(self.rows.open())?;
        stored.iter().map(Self::decode).collect()
    }
}
