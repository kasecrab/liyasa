//! The verification index (PRD §6.6, "to avoid the key-discovery problem").
//!
//! A query's inputs are only known once it has run, so a cold cache cannot form
//! the artifact key without doing the work. The index remembers, per
//! `(query, primary key)`, the inputs as last stamped and the output key. A
//! rebuild re-stats those inputs — size and mtime first, a re-hash only when
//! they disagree — and on a match fetches the output without running anything.
//!
//! The file is disposable: a corrupt one is reported as `W0702` and rebuilt, so
//! it stays safe to restore in CI and to share between branches, where rows
//! from another branch are simply misses.

use std::collections::BTreeMap;
use std::fs;
use std::path::Path;
use std::time::{SystemTime, UNIX_EPOCH};

use liyasa_core::build::CacheError;
use liyasa_core::diagnostics::{Diagnostic, Diagnostics, code};
use liyasa_core::ids::Fingerprint;
use liyasa_core::vfs::{Vfs, VfsPath};
use serde::{Deserialize, Serialize};

/// One input file as last seen. `mtime` and `size` are the short-circuit;
/// `fingerprint` is the truth they stand in for.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct InputStamp {
    pub path: VfsPath,
    pub size: u64,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub mtime_unix: Option<i64>,
    pub fingerprint: Fingerprint,
}

impl InputStamp {
    pub fn of(vfs: &dyn Vfs, path: &VfsPath) -> Option<Self> {
        let meta = vfs.metadata(path).ok()?;
        let fingerprint = vfs.fingerprint(path).ok()?;
        Some(Self {
            path: path.clone(),
            size: meta.size,
            mtime_unix: meta.mtime.and_then(to_unix),
            fingerprint,
        })
    }

    /// Whether the file still matches this stamp. Equal size and mtime are
    /// taken at their word; anything else is re-hashed.
    pub fn is_current(&self, vfs: &dyn Vfs) -> bool {
        let Ok(meta) = vfs.metadata(&self.path) else {
            return false;
        };
        if meta.size == self.size
            && self.mtime_unix.is_some()
            && meta.mtime.and_then(to_unix) == self.mtime_unix
        {
            return true;
        }
        vfs.fingerprint(&self.path)
            .is_ok_and(|found| found == self.fingerprint)
    }
}

/// What one query execution read and what it produced.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Row {
    #[serde(default)]
    pub inputs: Vec<InputStamp>,
    /// Fingerprints that are not files: config keys, other queries' outputs,
    /// environment values (§6.6.2 rule 6).
    #[serde(default)]
    pub derived: Vec<Fingerprint>,
    pub output: Fingerprint,
}

impl Row {
    pub fn is_current(&self, vfs: &dyn Vfs, derived: &[Fingerprint]) -> bool {
        self.derived == derived && self.inputs.iter().all(|input| input.is_current(vfs))
    }

    /// The input fingerprints in the order the artifact key was formed from.
    pub fn fingerprints(&self) -> Vec<Fingerprint> {
        self.inputs
            .iter()
            .map(|input| input.fingerprint)
            .chain(self.derived.iter().copied())
            .collect()
    }
}

#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct Index {
    /// Query name, then primary key — a page path, a config key, a route.
    #[serde(default)]
    rows: BTreeMap<String, BTreeMap<String, Row>>,
}

impl Index {
    pub const FILE: &'static str = "index.json";

    pub fn new() -> Self {
        Self::default()
    }

    /// Reads the index beside the artifacts. A file that will not parse is not
    /// an error: it is `W0702` and an empty index, which costs a rebuild.
    pub fn load(path: &Path) -> (Self, Diagnostics) {
        let mut diagnostics = Diagnostics::new();
        let Ok(bytes) = fs::read(path) else {
            return (Self::new(), diagnostics);
        };
        match serde_json::from_slice(&bytes) {
            Ok(index) => (index, diagnostics),
            Err(error) => {
                diagnostics.push(
                    Diagnostic::new(
                        code::W0702,
                        format!(
                            "the build cache index at {} is unreadable ({error})",
                            path.display()
                        ),
                    )
                    .help("the index was discarded and will be rebuilt; artifacts were kept"),
                );
                (Self::new(), diagnostics)
            }
        }
    }

    pub fn save(&self, path: &Path) -> Result<(), CacheError> {
        if let Some(parent) = path.parent() {
            fs::create_dir_all(parent).map_err(|error| CacheError(error.to_string()))?;
        }
        let bytes = serde_json::to_vec(self).map_err(|error| CacheError(error.to_string()))?;
        fs::write(path, bytes).map_err(|error| CacheError(error.to_string()))
    }

    pub fn row(&self, query: &str, primary: &str) -> Option<&Row> {
        self.rows.get(query)?.get(primary)
    }

    pub fn record(&mut self, query: &str, primary: &str, row: Row) {
        self.rows
            .entry(query.to_owned())
            .or_default()
            .insert(primary.to_owned(), row);
    }

    pub fn forget(&mut self, query: &str, primary: &str) {
        if let Some(rows) = self.rows.get_mut(query) {
            rows.remove(primary);
        }
    }

    /// Every output an index row still points at, which is what a garbage
    /// collection pass must keep.
    pub fn reachable(&self) -> Vec<Fingerprint> {
        self.rows
            .values()
            .flat_map(|rows| rows.values().map(|row| row.output))
            .collect()
    }

    pub fn len(&self) -> usize {
        self.rows.values().map(BTreeMap::len).sum()
    }

    pub fn is_empty(&self) -> bool {
        self.len() == 0
    }
}

fn to_unix(time: SystemTime) -> Option<i64> {
    match time.duration_since(UNIX_EPOCH) {
        Ok(since) => i64::try_from(since.as_secs()).ok(),
        Err(before) => i64::try_from(before.duration().as_secs())
            .ok()
            .map(|seconds| -seconds),
    }
}

#[cfg(test)]
mod tests {
    use liyasa_config::vfs::MemVfs;

    use super::*;

    fn vfs(pairs: &[(&str, &str)]) -> MemVfs {
        pairs
            .iter()
            .map(|(path, text)| (*path, text.as_bytes().to_vec()))
            .collect()
    }

    fn stamp(vfs: &dyn Vfs, path: &str) -> InputStamp {
        InputStamp::of(vfs, &VfsPath::new(path)).expect("the file exists")
    }

    #[test]
    fn a_stamp_of_an_unchanged_file_is_current() {
        let files = vfs(&[("index.md", "# hello")]);
        let stamp = stamp(&files, "index.md");
        assert!(stamp.is_current(&files));
    }

    #[test]
    fn a_changed_file_invalidates_its_stamp() {
        let before = vfs(&[("index.md", "# hello")]);
        let after = vfs(&[("index.md", "# hello there")]);
        assert!(!stamp(&before, "index.md").is_current(&after));
    }

    #[test]
    fn a_deleted_file_invalidates_its_stamp() {
        let before = vfs(&[("index.md", "# hello")]);
        let after = vfs(&[]);
        assert!(!stamp(&before, "index.md").is_current(&after));
    }

    #[test]
    fn a_row_is_current_only_when_every_input_and_derived_value_is() {
        let files = vfs(&[("index.md", "# hello"), ("liyasa.json", "{}")]);
        let row = Row {
            inputs: vec![stamp(&files, "index.md"), stamp(&files, "liyasa.json")],
            derived: vec![Fingerprint::of("nav")],
            output: Fingerprint::of("html"),
        };
        assert!(row.is_current(&files, &[Fingerprint::of("nav")]));
        assert!(!row.is_current(&files, &[Fingerprint::of("other nav")]));
        assert_eq!(row.fingerprints().len(), 3);
    }

    #[test]
    fn the_index_round_trips_through_its_file() {
        let directory = std::env::temp_dir().join(format!(
            "liyasa-build-index-{}-{}",
            "roundtrip",
            std::process::id()
        ));
        let _ = fs::remove_dir_all(&directory);
        let path = directory.join(Index::FILE);
        let files = vfs(&[("index.md", "# hello")]);

        let mut index = Index::new();
        index.record(
            "page_html",
            "index.md",
            Row {
                inputs: vec![stamp(&files, "index.md")],
                derived: Vec::new(),
                output: Fingerprint::of("html"),
            },
        );
        index.save(&path).expect("the index saves");

        let (back, diagnostics) = Index::load(&path);
        assert!(diagnostics.is_empty());
        assert_eq!(back, index);
        assert_eq!(back.reachable(), vec![Fingerprint::of("html")]);
        let _ = fs::remove_dir_all(&directory);
    }

    #[test]
    fn a_corrupt_index_is_a_warning_and_an_empty_index() {
        let directory =
            std::env::temp_dir().join(format!("liyasa-build-index-corrupt-{}", std::process::id()));
        let _ = fs::remove_dir_all(&directory);
        fs::create_dir_all(&directory).expect("a temporary directory");
        let path = directory.join(Index::FILE);
        fs::write(&path, b"{ this is not json").expect("the file writes");

        let (index, diagnostics) = Index::load(&path);
        assert!(index.is_empty());
        let codes: Vec<_> = diagnostics.iter().map(|d| d.code.as_str()).collect();
        assert_eq!(codes, ["W0702"]);
        let _ = fs::remove_dir_all(&directory);
    }

    #[test]
    fn a_missing_index_is_simply_empty() {
        let (index, diagnostics) = Index::load(Path::new("/nonexistent/liyasa/index.json"));
        assert!(index.is_empty());
        assert!(diagnostics.is_empty());
    }

    #[test]
    fn a_row_can_be_forgotten() {
        let empty = || Row {
            inputs: Vec::new(),
            derived: Vec::new(),
            output: Fingerprint::of("out"),
        };
        let mut index = Index::new();
        index.record("page_html", "a.md", empty());
        index.record("page_html", "b.md", empty());
        index.forget("page_html", "a.md");
        assert!(index.row("page_html", "a.md").is_none());
        assert_eq!(index.len(), 1);
    }
}
