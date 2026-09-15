//! Re-indexing only what changed (SRC-07), and reusing what did not (SRC-11).
//!
//! Two halves. Section fingerprints decide which documents the tantivy writer
//! is handed; shard content hashes decide which browser-index files the
//! artifact cache can hand back instead of rewriting them. Both are content
//! addresses, so CI on an unchanged branch writes nothing.

use std::collections::BTreeMap;

use liyasa_core::build::ArtifactCache;
use liyasa_core::ids::Fingerprint;
use liyasa_core::vfs::Bytes;

use crate::doc::SectionDocument;
use crate::idx::writer::BuiltIndex;

/// What each section hashed to last time.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct SectionFingerprints(BTreeMap<String, Fingerprint>);

impl SectionFingerprints {
    pub fn of(documents: &[SectionDocument]) -> Self {
        Self(
            documents
                .iter()
                .map(|document| (document.key(), fingerprint(document)))
                .collect(),
        )
    }

    pub fn get(&self, key: &str) -> Option<Fingerprint> {
        self.0.get(key).copied()
    }

    pub fn len(&self) -> usize {
        self.0.len()
    }

    pub fn is_empty(&self) -> bool {
        self.0.is_empty()
    }
}

/// A section's content address: everything the index stores about it, so a
/// change to a boost or a group is a change, and a change elsewhere on the
/// page is not.
pub fn fingerprint(document: &SectionDocument) -> Fingerprint {
    match serde_json::to_vec(document) {
        Ok(bytes) => Fingerprint::of(bytes),
        // A section that will not serialize cannot be matched to a previous
        // build, so it is treated as new every time rather than as unchanged.
        Err(_) => Fingerprint::of(document.key().as_bytes()),
    }
}

#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct Plan {
    pub added: Vec<String>,
    pub changed: Vec<String>,
    pub removed: Vec<String>,
    pub unchanged: Vec<String>,
}

impl Plan {
    /// Everything the writer must touch.
    pub fn to_write(&self) -> Vec<String> {
        let mut out = self.added.clone();
        out.extend(self.changed.iter().cloned());
        out.sort();
        out
    }

    pub fn is_noop(&self) -> bool {
        self.added.is_empty() && self.changed.is_empty() && self.removed.is_empty()
    }
}

pub fn plan(previous: &SectionFingerprints, documents: &[SectionDocument]) -> Plan {
    let current = SectionFingerprints::of(documents);
    let mut plan = Plan::default();

    for (key, hash) in &current.0 {
        match previous.get(key) {
            None => plan.added.push(key.clone()),
            Some(before) if before != *hash => plan.changed.push(key.clone()),
            Some(_) => plan.unchanged.push(key.clone()),
        }
    }
    for key in previous.0.keys() {
        if !current.0.contains_key(key) {
            plan.removed.push(key.clone());
        }
    }
    plan
}

/// What a rebuild got back from the cache rather than writing again.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct CacheReport {
    pub hits: usize,
    pub misses: usize,
}

impl CacheReport {
    pub fn total(&self) -> usize {
        self.hits + self.misses
    }
}

/// Stores every shard file under its content address, so the next build can
/// ask for it by hash (SRC-07: "index segments are content-addressed for
/// cache reuse in CI").
pub fn store(cache: &dyn ArtifactCache, built: &BuiltIndex) -> CacheReport {
    let mut report = CacheReport::default();
    for name in shard_files(built) {
        let Some(bytes) = built.files.get(&name) else {
            continue;
        };
        let key = Fingerprint::of(bytes);
        if cache.get(&key).is_some() {
            report.hits += 1;
            continue;
        }
        report.misses += 1;
        let _ = cache.put(&key, Bytes::copy_from_slice(bytes), &[]);
    }
    report
}

/// How much of `built` the cache already holds. A shard whose documents did
/// not change hashes to the same bytes and is a hit.
pub fn report(cache: &dyn ArtifactCache, built: &BuiltIndex) -> CacheReport {
    let mut report = CacheReport::default();
    for name in shard_files(built) {
        let Some(bytes) = built.files.get(&name) else {
            continue;
        };
        if cache.get(&Fingerprint::of(bytes)).is_some() {
            report.hits += 1;
        } else {
            report.misses += 1;
        }
    }
    report
}

/// Every shard file, in manifest order. `manifest.json` is excluded: it
/// carries corpus statistics and changes whenever anything does.
fn shard_files(built: &BuiltIndex) -> Vec<String> {
    built
        .manifest
        .shards
        .iter()
        .flat_map(|shard| shard.files.all().map(str::to_owned))
        .collect()
}

#[cfg(test)]
mod tests {
    use liyasa_core::ids::{Locale, Route};

    use super::*;
    use crate::doc::{DocKind, SectionDocument};

    fn section(route: &str, body: &str) -> SectionDocument {
        SectionDocument {
            route: Route::new(route),
            anchor: String::new(),
            title: "Title".to_owned(),
            section: "Title".to_owned(),
            breadcrumb: Vec::new(),
            body: body.to_owned(),
            code: String::new(),
            keywords: Vec::new(),
            tab: None,
            version: None,
            locale: Locale::new("en"),
            kind: DocKind::Page,
            boost: 1.0,
            groups: Vec::new(),
            regions: Vec::new(),
            updated: None,
        }
    }

    #[test]
    fn an_unchanged_corpus_is_a_noop() {
        let documents = vec![section("/a", "one"), section("/b", "two")];
        let before = SectionFingerprints::of(&documents);
        let plan = plan(&before, &documents);
        assert!(plan.is_noop());
        assert_eq!(plan.unchanged.len(), 2);
    }

    #[test]
    fn only_the_edited_section_is_changed() {
        let documents = vec![section("/a", "one"), section("/b", "two")];
        let before = SectionFingerprints::of(&documents);
        let after = vec![section("/a", "one"), section("/b", "two, edited")];
        let plan = plan(&before, &after);
        assert_eq!(plan.changed, ["/b"]);
        assert_eq!(plan.unchanged, ["/a"]);
        assert!(plan.added.is_empty() && plan.removed.is_empty());
    }

    #[test]
    fn a_new_page_is_added_and_a_deleted_one_removed() {
        let before = SectionFingerprints::of(&[section("/a", "one"), section("/gone", "x")]);
        let plan = plan(&before, &[section("/a", "one"), section("/new", "y")]);
        assert_eq!(plan.added, ["/new"]);
        assert_eq!(plan.removed, ["/gone"]);
        assert_eq!(plan.to_write(), ["/new"]);
    }

    #[test]
    fn a_changed_boost_is_a_change() {
        let documents = vec![section("/a", "one")];
        let before = SectionFingerprints::of(&documents);
        let mut boosted = documents;
        boosted[0].boost = 2.0;
        assert_eq!(plan(&before, &boosted).changed, ["/a"]);
    }

    #[test]
    fn the_same_section_hashes_the_same_way_twice() {
        assert_eq!(
            fingerprint(&section("/a", "one")),
            fingerprint(&section("/a", "one"))
        );
        assert_ne!(
            fingerprint(&section("/a", "one")),
            fingerprint(&section("/a", "two"))
        );
    }
}
