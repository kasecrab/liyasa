//! The claim table: which block states which fact, and who said so (§14.12).
//!
//! §14.12's rule is that the scanner never writes claims — the reviewer's
//! confirmation does. The table holds both halves so that a candidate the
//! reviewer confirmed is never rescanned and never re-proposed: a later scan
//! refreshes what the candidate says and leaves the decision alone. That is the
//! "claims registry that learns from confirmations" §14.5 relies on.
//!
//! `liyasa_core::store::Claim` is an empty marker (RFC 1400), so
//! [`ClaimRecord`] is the row, proposed verbatim for the marker's fields in
//! `plan/rfcs/2002-where-the-graph-tables-live.md`.

use std::collections::BTreeMap;
use std::sync::RwLock;
use std::time::SystemTime;

use liyasa_core::ids::{BlockId, FactId, Route};
use liyasa_core::verify::{ClaimCandidate, StoreError};

use crate::core::Scrubber;

/// What a claim is worth. A candidate is the scanner's proposal; the other two
/// are a reviewer's decision and outrank any later scan.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub enum ClaimStatus {
    Candidate,
    Confirmed,
    Rejected,
}

/// One block stating one fact. The block ID is stable across edits that do not
/// change the sentence (§7.16), so a claim survives a rewrite of the page
/// around it.
#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord)]
pub struct ClaimKey {
    pub page: Route,
    pub block: BlockId,
    pub fact: FactId,
}

impl ClaimKey {
    pub fn of(candidate: &ClaimCandidate) -> Self {
        Self {
            page: candidate.page.clone(),
            block: candidate.block,
            fact: candidate.fact.clone(),
        }
    }
}

#[derive(Debug, Clone, PartialEq)]
pub struct ClaimRecord {
    pub key: ClaimKey,
    pub confidence: f32,
    /// Scrubbed and capped at [`crate::core::EXCERPT_LIMIT`].
    pub excerpt: String,
    pub status: ClaimStatus,
    pub first_seen: SystemTime,
    pub last_seen: SystemTime,
    pub decided_at: Option<SystemTime>,
    pub decided_by: Option<String>,
}

#[derive(Default)]
pub struct MemoryClaims {
    rows: RwLock<BTreeMap<ClaimKey, ClaimRecord>>,
}

impl MemoryClaims {
    pub fn new() -> Self {
        Self::default()
    }

    /// Records what a scan found. A key the table already holds keeps its
    /// status, its reviewer, and the time it was first seen; everything the
    /// scan can observe afresh is refreshed.
    pub fn observe(&self, candidate: &ClaimCandidate, at: SystemTime) -> Result<(), StoreError> {
        let key = ClaimKey::of(candidate);
        let excerpt = Scrubber::new().excerpt(&candidate.excerpt);
        let mut rows = self.write()?;
        match rows.get_mut(&key) {
            Some(row) => {
                row.confidence = candidate.confidence;
                row.excerpt = excerpt;
                row.last_seen = at;
            }
            None => {
                rows.insert(
                    key.clone(),
                    ClaimRecord {
                        key,
                        confidence: candidate.confidence,
                        excerpt,
                        status: ClaimStatus::Candidate,
                        first_seen: at,
                        last_seen: at,
                        decided_at: None,
                        decided_by: None,
                    },
                );
            }
        }
        Ok(())
    }

    pub fn confirm(&self, key: &ClaimKey, by: &str, at: SystemTime) -> Result<(), StoreError> {
        self.decide(key, ClaimStatus::Confirmed, by, at)
    }

    pub fn reject(&self, key: &ClaimKey, by: &str, at: SystemTime) -> Result<(), StoreError> {
        self.decide(key, ClaimStatus::Rejected, by, at)
    }

    fn decide(
        &self,
        key: &ClaimKey,
        status: ClaimStatus,
        by: &str,
        at: SystemTime,
    ) -> Result<(), StoreError> {
        let mut rows = self.write()?;
        let row = rows.get_mut(key).ok_or(StoreError::NotFound)?;
        row.status = status;
        row.decided_at = Some(at);
        row.decided_by = Some(by.to_owned());
        Ok(())
    }

    pub fn get(&self, key: &ClaimKey) -> Result<Option<ClaimRecord>, StoreError> {
        Ok(self.read()?.get(key).cloned())
    }

    /// Every claim on a fact, in page then block order.
    pub fn for_fact(&self, fact: &FactId) -> Result<Vec<ClaimRecord>, StoreError> {
        self.filter(|row| &row.key.fact == fact)
    }

    pub fn for_page(&self, page: &Route) -> Result<Vec<ClaimRecord>, StoreError> {
        self.filter(|row| &row.key.page == page)
    }

    /// The pages a reviewer has confirmed state this fact. A scan that skips
    /// them is skipping the claims it already knows about, not missing them.
    pub fn confirmed_pages(&self, fact: &FactId) -> Result<Vec<Route>, StoreError> {
        let mut pages: Vec<Route> = self
            .for_fact(fact)?
            .into_iter()
            .filter(|row| row.status == ClaimStatus::Confirmed)
            .map(|row| row.key.page)
            .collect();
        pages.dedup();
        Ok(pages)
    }

    /// The claim table, in key order.
    pub fn rows(&self) -> Result<Vec<ClaimRecord>, StoreError> {
        self.filter(|_| true)
    }

    fn filter(&self, keep: impl Fn(&ClaimRecord) -> bool) -> Result<Vec<ClaimRecord>, StoreError> {
        Ok(self
            .read()?
            .values()
            .filter(|row| keep(row))
            .cloned()
            .collect())
    }

    fn read(
        &self,
    ) -> Result<std::sync::RwLockReadGuard<'_, BTreeMap<ClaimKey, ClaimRecord>>, StoreError> {
        self.rows.read().map_err(|_| poisoned())
    }

    fn write(
        &self,
    ) -> Result<std::sync::RwLockWriteGuard<'_, BTreeMap<ClaimKey, ClaimRecord>>, StoreError> {
        self.rows.write().map_err(|_| poisoned())
    }
}

fn poisoned() -> StoreError {
    StoreError::Io("the claim table's lock is poisoned".to_owned())
}

#[cfg(test)]
mod tests;
