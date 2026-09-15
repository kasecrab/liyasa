//! The two-tier cache of PRD §6.6: an on-disk, fingerprint-keyed artifact
//! store in `.liyasa/cache`, and the verification index that lets a rebuild
//! skip a query without first doing the work that would reveal its key.
//!
//! A query execution records what it read; the result is stored under
//! `hash(query name, input hashes, liyasa version)`. Because that key is only
//! knowable after the query runs, the index keeps a row per
//! `(query, primary key)` with the input stamps as last seen and the output
//! key, so a rebuild re-stats those inputs and fetches the output directly.

pub mod index;

use std::fs;
use std::path::{Path, PathBuf};
use std::time::{Duration, SystemTime};

use liyasa_core::build::{ArtifactCache, CacheError, GcReport};
use liyasa_core::ids::Fingerprint;
use liyasa_core::vfs::Bytes;
use serde::{Deserialize, Serialize};

pub use index::{Index, InputStamp, Row};

/// The Liyasa version that keys every artifact: an upgrade must not serve the
/// previous release's rendering.
pub const VERSION: &str = env!("CARGO_PKG_VERSION");

/// `hash(query name, input hashes, liyasa version)` (§6.6).
pub fn key(query: &str, inputs: &[Fingerprint]) -> Fingerprint {
    let mut parts: Vec<Vec<u8>> = Vec::with_capacity(inputs.len() + 2);
    parts.push(query.as_bytes().to_vec());
    parts.push(VERSION.as_bytes().to_vec());
    for input in inputs {
        parts.push(input.0.to_vec());
    }
    Fingerprint::of_parts(parts.iter().map(Vec::as_slice))
}

#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
struct Meta {
    bytes: u64,
    #[serde(default)]
    inputs: Vec<Fingerprint>,
}

/// `.liyasa/cache`, in the style of Bazel's action cache.
#[derive(Debug, Clone)]
pub struct DiskCache {
    root: PathBuf,
}

impl DiskCache {
    pub fn new(root: impl AsRef<Path>) -> Self {
        Self {
            root: root.as_ref().to_path_buf(),
        }
    }

    pub fn root(&self) -> &Path {
        &self.root
    }

    fn objects(&self) -> PathBuf {
        self.root.join("objects")
    }

    /// Two hex characters of fan-out: a 10,000-page site's artifacts land in
    /// 256 directories rather than one.
    fn object_path(&self, key: &Fingerprint) -> PathBuf {
        let hex = key.to_hex();
        let (shard, rest) = hex.split_at(2);
        self.objects().join(shard).join(rest)
    }

    fn meta_path(&self, key: &Fingerprint) -> PathBuf {
        self.object_path(key).with_extension("json")
    }

    /// Writes through a temporary file in the same directory, so a reader never
    /// sees a half-written artifact and a crash leaves no corrupt entry.
    fn write_atomic(path: &Path, bytes: &[u8]) -> Result<(), CacheError> {
        let parent = path
            .parent()
            .ok_or_else(|| CacheError(format!("no parent directory for {}", path.display())))?;
        fs::create_dir_all(parent).map_err(|error| CacheError(error.to_string()))?;
        let temporary = parent.join(format!(
            ".{}.{}.tmp",
            path.file_name()
                .and_then(|name| name.to_str())
                .unwrap_or("entry"),
            std::process::id()
        ));
        fs::write(&temporary, bytes).map_err(|error| CacheError(error.to_string()))?;
        match fs::rename(&temporary, path) {
            Ok(()) => Ok(()),
            Err(error) => {
                let _ = fs::remove_file(&temporary);
                Err(CacheError(error.to_string()))
            }
        }
    }

    /// Every artifact, as `(object path, size, last modified)`.
    fn entries(&self) -> Vec<(PathBuf, u64, SystemTime)> {
        let mut out = Vec::new();
        let Ok(shards) = fs::read_dir(self.objects()) else {
            return out;
        };
        for shard in shards.flatten() {
            let Ok(files) = fs::read_dir(shard.path()) else {
                continue;
            };
            for file in files.flatten() {
                let path = file.path();
                if path.extension().is_some_and(|ext| ext == "json") {
                    continue;
                }
                let Ok(meta) = file.metadata() else { continue };
                if !meta.is_file() {
                    continue;
                }
                let modified = meta.modified().unwrap_or(SystemTime::UNIX_EPOCH);
                out.push((path, meta.len(), modified));
            }
        }
        out
    }

    fn remove(&self, path: &Path, report: &mut GcReport, bytes: u64) {
        if fs::remove_file(path).is_ok() {
            report.removed += 1;
            report.bytes_freed += bytes;
            let _ = fs::remove_file(path.with_extension("json"));
        }
    }

    /// What `put` recorded as this artifact's inputs, for reachability.
    pub fn inputs_of(&self, key: &Fingerprint) -> Vec<Fingerprint> {
        fs::read(self.meta_path(key))
            .ok()
            .and_then(|bytes| serde_json::from_slice::<Meta>(&bytes).ok())
            .map(|meta| meta.inputs)
            .unwrap_or_default()
    }

    pub fn total_bytes(&self) -> u64 {
        self.entries().iter().map(|(_, size, _)| size).sum()
    }
}

impl ArtifactCache for DiskCache {
    fn get(&self, key: &Fingerprint) -> Option<Bytes> {
        fs::read(self.object_path(key)).ok().map(Bytes::from)
    }

    fn put(
        &self,
        key: &Fingerprint,
        value: Bytes,
        inputs: &[Fingerprint],
    ) -> Result<(), CacheError> {
        let meta = Meta {
            bytes: value.len() as u64,
            inputs: inputs.to_vec(),
        };
        Self::write_atomic(&self.object_path(key), &value)?;
        let encoded = serde_json::to_vec(&meta).map_err(|error| CacheError(error.to_string()))?;
        Self::write_atomic(&self.meta_path(key), &encoded)
    }

    /// Age first, then size: an artifact nothing has touched since `max_age` is
    /// gone whatever the budget, and what remains is trimmed oldest-first until
    /// it fits `max_bytes`.
    fn gc(&self, max_bytes: u64, max_age: Duration) -> Result<GcReport, CacheError> {
        let mut report = GcReport::default();
        let now = SystemTime::now();
        let mut kept: Vec<(PathBuf, u64, SystemTime)> = Vec::new();

        for (path, size, modified) in self.entries() {
            let age = now.duration_since(modified).unwrap_or_default();
            if age > max_age {
                self.remove(&path, &mut report, size);
            } else {
                kept.push((path, size, modified));
            }
        }

        kept.sort_by_key(|(path, _, modified)| (*modified, path.clone()));
        let mut total: u64 = kept.iter().map(|(_, size, _)| size).sum();
        for (path, size, _) in &kept {
            if total <= max_bytes {
                break;
            }
            self.remove(path, &mut report, *size);
            total = total.saturating_sub(*size);
        }
        Ok(report)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    struct Temp(PathBuf);

    impl Temp {
        fn new(name: &str) -> Self {
            let path = std::env::temp_dir()
                .join(format!("liyasa-build-cache-{name}-{}", std::process::id()));
            let _ = fs::remove_dir_all(&path);
            fs::create_dir_all(&path).expect("a temporary directory");
            Self(path)
        }
    }

    impl Drop for Temp {
        fn drop(&mut self) {
            let _ = fs::remove_dir_all(&self.0);
        }
    }

    fn bytes(text: &str) -> Bytes {
        Bytes::from(text.as_bytes().to_vec())
    }

    #[test]
    fn a_key_covers_the_query_name_the_inputs_and_the_version() {
        let one = Fingerprint::of("a");
        let two = Fingerprint::of("b");
        assert_ne!(key("page_html", &[one]), key("page_markdown", &[one]));
        assert_ne!(key("page_html", &[one]), key("page_html", &[two]));
        assert_ne!(key("page_html", &[one, two]), key("page_html", &[two, one]));
        assert_eq!(key("page_html", &[one]), key("page_html", &[one]));
    }

    #[test]
    fn what_goes_in_comes_back_out() {
        let temp = Temp::new("roundtrip");
        let cache = DiskCache::new(&temp.0);
        let id = key("page_html", &[Fingerprint::of("source")]);
        assert!(cache.get(&id).is_none());
        cache
            .put(&id, bytes("<h1>hi</h1>"), &[Fingerprint::of("source")])
            .expect("the artifact stores");
        assert_eq!(cache.get(&id).as_deref(), Some(&b"<h1>hi</h1>"[..]));
        assert_eq!(cache.inputs_of(&id), vec![Fingerprint::of("source")]);
    }

    #[test]
    fn a_second_put_replaces_the_artifact() {
        let temp = Temp::new("replace");
        let cache = DiskCache::new(&temp.0);
        let id = Fingerprint::of("k");
        cache.put(&id, bytes("old"), &[]).expect("first put");
        cache.put(&id, bytes("new"), &[]).expect("second put");
        assert_eq!(cache.get(&id).as_deref(), Some(&b"new"[..]));
        assert_eq!(cache.total_bytes(), 3);
    }

    #[test]
    fn garbage_collection_drops_what_does_not_fit_the_budget() {
        let temp = Temp::new("gc-size");
        let cache = DiskCache::new(&temp.0);
        for n in 0..4u8 {
            cache
                .put(&Fingerprint::of([n]), bytes("0123456789"), &[])
                .expect("the artifact stores");
        }
        assert_eq!(cache.total_bytes(), 40);
        let report = cache
            .gc(20, Duration::from_secs(3600))
            .expect("garbage collection runs");
        assert!(report.removed >= 2, "removed {}", report.removed);
        assert!(cache.total_bytes() <= 20);
        assert_eq!(report.bytes_freed, report.removed * 10);
    }

    #[test]
    fn garbage_collection_drops_what_is_older_than_the_age() {
        let temp = Temp::new("gc-age");
        let cache = DiskCache::new(&temp.0);
        let id = Fingerprint::of("old");
        cache.put(&id, bytes("stale"), &[]).expect("put");
        let report = cache
            .gc(u64::MAX, Duration::from_secs(0))
            .expect("garbage collection runs");
        assert_eq!(report.removed, 1);
        assert!(cache.get(&id).is_none());
    }

    #[test]
    fn a_missing_cache_directory_is_a_miss_not_an_error() {
        let cache = DiskCache::new(
            std::env::temp_dir().join(format!("liyasa-build-cache-absent-{}", std::process::id())),
        );
        assert!(cache.get(&Fingerprint::of("k")).is_none());
        assert_eq!(cache.total_bytes(), 0);
        let report = cache
            .gc(0, Duration::from_secs(0))
            .expect("garbage collection runs on an empty cache");
        assert_eq!(report, GcReport::default());
    }
}
