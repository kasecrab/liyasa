//! Where records live between runs.
//!
//! `liyasa_core::store::Drift` is a field-less `entity!` and is frozen, so the
//! frozen `DriftRepo` can carry a record's identity and nothing about it — not
//! the values that moved, not the pages, not the evidence. [`RecordStore`] is
//! the seam that carries the record itself, and RFC 2061 is why it exists here
//! rather than as a change to the contract.
//!
//! [`MemoryDrift`] implements both, which is the point: a caller holding only
//! `&dyn DriftRepo` can still ask how much drift is open, and the answer comes
//! from the same rows the engine wrote.

use std::collections::BTreeMap;
use std::sync::RwLock;

use liyasa_core::ids::{JobId, ProjectId};
use liyasa_core::net::BoxFut;
use liyasa_core::store::{Drift, DriftQuery, DriftRepo, Page, Repo};
use liyasa_core::verify::StoreError;

use super::record::{DriftKey, DriftRecord};

/// The records the engine reads and writes.
///
/// Sync, because [`liyasa_core::verify::DriftEngine::apply`] is sync and this
/// crate has no runtime to block on.
/// The method names avoid `get`, `put`, `delete` and `list` on purpose: an
/// implementation is expected to be the same type as the frozen `Repo<Drift>`,
/// and a caller should never have to disambiguate which `put` it meant.
pub trait RecordStore: Send + Sync {
    /// Every record, open or resolved, in key order.
    fn all(&self) -> Result<Vec<DriftRecord>, StoreError>;
    fn find(&self, key: &DriftKey) -> Result<Option<DriftRecord>, StoreError>;
    /// Insert or replace by [`DriftRecord::key`].
    fn save(&self, record: &DriftRecord) -> Result<(), StoreError>;

    /// Every write one reconciliation makes, in one call.
    ///
    /// `Engine::record` calls this once rather than `save` per record, because
    /// a run is one logical operation: a store backed by SQL should be able to
    /// make it one transaction, and under autocommit the alternative is a
    /// separate commit per record — fine at ten and not at a thousand.
    ///
    /// The default is a loop, so an implementation that only has `save` needs
    /// no change and gains nothing. Override it to get the transaction.
    fn save_all(&self, records: &[DriftRecord]) -> Result<(), StoreError> {
        records.iter().try_for_each(|record| self.save(record))
    }

    /// The open ones, which is what a dashboard and the maintenance agent read.
    fn open_records(&self) -> Result<Vec<DriftRecord>, StoreError> {
        Ok(self
            .all()?
            .into_iter()
            .filter(DriftRecord::is_open)
            .collect())
    }
}

#[derive(Default)]
pub struct MemoryDrift {
    rows: RwLock<BTreeMap<DriftKey, DriftRecord>>,
}

impl MemoryDrift {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn len(&self) -> usize {
        self.read(|rows| rows.len())
    }

    pub fn is_empty(&self) -> bool {
        self.len() == 0
    }

    fn read<T>(&self, f: impl FnOnce(&BTreeMap<DriftKey, DriftRecord>) -> T) -> T {
        match self.rows.read() {
            Ok(rows) => f(&rows),
            // A poisoned lock means a panic while a record was being written.
            // The map is still a map; reading it is better than panicking
            // again on the way to reporting the first panic.
            Err(poisoned) => f(&poisoned.into_inner()),
        }
    }
}

impl RecordStore for MemoryDrift {
    fn all(&self) -> Result<Vec<DriftRecord>, StoreError> {
        Ok(self.read(|rows| rows.values().cloned().collect()))
    }

    fn find(&self, key: &DriftKey) -> Result<Option<DriftRecord>, StoreError> {
        Ok(self.read(|rows| rows.get(key).cloned()))
    }

    fn save(&self, record: &DriftRecord) -> Result<(), StoreError> {
        let mut rows = self.rows.write().map_err(poisoned)?;
        rows.insert(record.key(), record.clone());
        Ok(())
    }
}

fn poisoned<T>(_: T) -> StoreError {
    StoreError::Io("the drift store lock is poisoned".to_owned())
}

/// The frozen half. `Drift` carries no fields, so every method that would have
/// to say *which* drift through the value alone answers that it cannot
/// (RFC 2061); the ones that only need a count answer from the real rows.
impl Repo<Drift> for MemoryDrift {
    fn get<'a>(&'a self, id: &'a JobId) -> BoxFut<'a, Result<Option<Drift>, StoreError>> {
        let found = self.read(|rows| {
            rows.keys()
                .any(|key| key.job_id() == *id)
                .then(Drift::default)
        });
        Box::pin(std::future::ready(Ok(found)))
    }

    /// A field-less value names no record, so a write through this method would
    /// be a silent no-op. Use [`RecordStore::put`].
    fn put<'a>(&'a self, _: &'a Drift) -> BoxFut<'a, Result<(), StoreError>> {
        Box::pin(std::future::ready(Err(StoreError::Conflict)))
    }

    fn delete<'a>(&'a self, id: &'a JobId) -> BoxFut<'a, Result<(), StoreError>> {
        let key = self.read(|rows| rows.keys().find(|key| key.job_id() == *id).cloned());
        let result = match key {
            Some(key) => {
                let _ = self.rows.write().map(|mut rows| rows.remove(&key));
                Ok(())
            }
            None => Err(StoreError::NotFound),
        };
        Box::pin(std::future::ready(result))
    }

    fn list<'a>(
        &'a self,
        q: &'a DriftQuery,
        page: Page,
    ) -> BoxFut<'a, Result<Vec<Drift>, StoreError>> {
        let matched = self.read(|rows| {
            rows.values()
                .filter(|record| q.open.is_none_or(|open| open == record.is_open()))
                .count()
        });
        let capped = match page.limit {
            0 => matched,
            limit => matched.min(usize::try_from(limit).unwrap_or(usize::MAX)),
        };
        Box::pin(std::future::ready(Ok(vec![Drift::default(); capped])))
    }
}

impl DriftRepo for MemoryDrift {
    /// One row per open record. The project is not a field of anything this
    /// store holds, so an in-memory store is one project's.
    fn open<'a>(&'a self, _: &'a ProjectId) -> BoxFut<'a, Result<Vec<Drift>, StoreError>> {
        let open = self.read(|rows| rows.values().filter(|r| r.is_open()).count());
        Box::pin(std::future::ready(Ok(vec![Drift::default(); open])))
    }
}

#[cfg(test)]
mod tests;
