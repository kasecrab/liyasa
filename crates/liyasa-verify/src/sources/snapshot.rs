//! Snapshots: one refresh of one source, and the difference between two
//! (VER-22, VER-23).
//!
//! A snapshot's digest is over the source ID and its values, never over the
//! clock, so refreshing an unchanged source twice produces the same digest and
//! a changed value is the only thing that makes a new one. That is what makes a
//! change diffable — comparing digests answers "did anything move?" without
//! comparing every fact.
//!
//! Every string in a snapshot goes through the [`Scrubber`] on the way in.
//! §30.2.4 names the snapshot as one of the four places a credential must never
//! reach, and a `url` source that echoes its own bearer token back in a JSON
//! body is the ordinary way one would get there.

use std::collections::{BTreeMap, BTreeSet};
use std::sync::RwLock;
use std::time::SystemTime;

use liyasa_core::ids::{FactId, Fingerprint};
use liyasa_core::verify::{
    ChangeKind, FactChange, FactValue, Snapshot, SnapshotDiffer, StoreError,
};

use crate::core::scrub::Scrubber;

/// The digest of a source's values: the source ID, then each fact ID and its
/// value, length-prefixed so no two different snapshots collide.
pub fn digest_of(source: &str, values: &BTreeMap<FactId, FactValue>) -> Fingerprint {
    let mut parts: Vec<Vec<u8>> = vec![source.as_bytes().to_vec()];
    for (fact, value) in values {
        parts.push(fact.as_str().as_bytes().to_vec());
        parts.push(serde_json::to_vec(value).unwrap_or_else(|_| b"<unserializable>".to_vec()));
    }
    Fingerprint::of_parts(parts.iter().map(Vec::as_slice))
}

/// One refresh, scrubbed and digested.
pub fn build(
    source: &str,
    taken_at: SystemTime,
    values: BTreeMap<FactId, FactValue>,
    scrubber: &Scrubber,
) -> Snapshot {
    let values: BTreeMap<FactId, FactValue> = values
        .into_iter()
        .map(|(fact, value)| (fact, scrub_value(&value, scrubber)))
        .collect();
    Snapshot {
        source: source.to_owned(),
        taken_at,
        digest: digest_of(source, &values),
        values,
    }
}

fn scrub_value(value: &FactValue, scrubber: &Scrubber) -> FactValue {
    match value {
        FactValue::Str(text) => FactValue::Str(scrubber.scrub(text)),
        FactValue::Enum(text) => FactValue::Enum(scrubber.scrub(text)),
        FactValue::Date(text) => FactValue::Date(scrubber.scrub(text)),
        FactValue::List(items) => {
            FactValue::List(items.iter().map(|it| scrub_value(it, scrubber)).collect())
        }
        FactValue::Object(fields) => FactValue::Object(
            fields
                .iter()
                .map(|(key, field)| (key.clone(), scrub_value(field, scrubber)))
                .collect(),
        ),
        // Numbers and booleans carry no text to redact.
        other => other.clone(),
    }
}

/// [`SnapshotDiffer`] over the values themselves.
#[derive(Debug, Clone, Copy, Default)]
pub struct ValueDiffer;

impl SnapshotDiffer for ValueDiffer {
    fn diff(&self, old: &Snapshot, new: &Snapshot) -> Vec<FactChange> {
        let facts: BTreeSet<&FactId> = old.values.keys().chain(new.values.keys()).collect();
        facts
            .into_iter()
            .filter_map(|fact| {
                let before = old.values.get(fact);
                let after = new.values.get(fact);
                let kind = match (before, after) {
                    (None, Some(_)) => ChangeKind::Added,
                    (Some(_), None) => ChangeKind::Removed,
                    (Some(a), Some(b)) if a != b => ChangeKind::Changed,
                    _ => return None,
                };
                Some(FactChange {
                    fact: fact.clone(),
                    old: before.cloned(),
                    new: after.cloned(),
                    kind,
                })
            })
            .collect()
    }
}

/// One stored refresh: the snapshot, what refreshed it, and whether that
/// refresh was trusted enough for an untrusted build to fall back to.
#[derive(Debug, Clone, PartialEq)]
pub struct StoredSnapshot {
    pub snapshot: Snapshot,
    /// The branch, build, or schedule that took it — VER-22's "attributable".
    pub by: String,
    /// Taken by a build on a trusted branch. VER-25 sends an untrusted build to
    /// the latest of these rather than letting it refresh.
    pub production: bool,
}

/// Every refresh of every source, newest last.
///
/// `liyasa-verify` has no database (PRD §6.2.1 puts `sqlx` in `liyasa-store`),
/// so this is the CLI's whole snapshot history and the shape the server's table
/// persists.
#[derive(Debug, Default)]
pub struct SnapshotLog {
    per_source: RwLock<BTreeMap<String, Vec<StoredSnapshot>>>,
}

impl SnapshotLog {
    pub fn new() -> Self {
        Self::default()
    }

    /// Stores a refresh and returns what changed since the last one of this
    /// source, or an empty list for the first.
    pub fn record(
        &self,
        snapshot: Snapshot,
        by: &str,
        production: bool,
    ) -> Result<Vec<FactChange>, StoreError> {
        let mut per_source = self.write()?;
        let history = per_source.entry(snapshot.source.clone()).or_default();
        let changes = history
            .last()
            .map(|previous| ValueDiffer.diff(&previous.snapshot, &snapshot))
            .unwrap_or_default();
        history.push(StoredSnapshot {
            snapshot,
            by: by.to_owned(),
            production,
        });
        Ok(changes)
    }

    pub fn latest(&self, source: &str) -> Result<Option<StoredSnapshot>, StoreError> {
        Ok(self
            .read()?
            .get(source)
            .and_then(|history| history.last())
            .cloned())
    }

    /// The newest snapshot a trusted build took. VER-25's fallback for a fork
    /// pull request or a branch outside the trusted list.
    pub fn latest_production(&self, source: &str) -> Result<Option<StoredSnapshot>, StoreError> {
        Ok(self
            .read()?
            .get(source)
            .and_then(|history| history.iter().rev().find(|stored| stored.production))
            .cloned())
    }

    pub fn history(&self, source: &str) -> Result<Vec<StoredSnapshot>, StoreError> {
        Ok(self.read()?.get(source).cloned().unwrap_or_default())
    }

    pub fn sources(&self) -> Result<Vec<String>, StoreError> {
        Ok(self.read()?.keys().cloned().collect())
    }

    fn read(&self) -> Result<std::sync::RwLockReadGuard<'_, Log>, StoreError> {
        self.per_source.read().map_err(|_| poisoned())
    }

    fn write(&self) -> Result<std::sync::RwLockWriteGuard<'_, Log>, StoreError> {
        self.per_source.write().map_err(|_| poisoned())
    }
}

type Log = BTreeMap<String, Vec<StoredSnapshot>>;

fn poisoned() -> StoreError {
    StoreError::Io("the snapshot log's lock is poisoned".to_owned())
}

#[cfg(test)]
mod tests;
