//! Reference implementations of the frozen traits.
//!
//! They exist to prove the kits catch what they claim to, and they double as
//! test doubles for every downstream package: a build test that needs a file
//! system wants [`MemoryVfs`], not a temporary directory.

use std::collections::BTreeMap;
use std::sync::Mutex;
use std::time::{Duration, SystemTime};

use crate::build::{ArtifactCache, CacheError, GcReport};
use crate::ids::Fingerprint;
use crate::server::{RateLimit, RateLimitKey, RateLimitPool, RateLimiter, RetryAfter};
use crate::verify::SecretSource;
use crate::vfs::{Bytes, Vfs, VfsError, VfsKind, VfsMeta, VfsPath};

/// A file system held in a map. Directories are implied by the paths.
#[derive(Debug, Default)]
pub struct MemoryVfs {
    files: BTreeMap<VfsPath, Vec<u8>>,
}

impl MemoryVfs {
    pub fn new() -> Self {
        Self::default()
    }

    #[must_use]
    pub fn with(mut self, path: impl AsRef<str>, bytes: impl Into<Vec<u8>>) -> Self {
        self.files.insert(VfsPath::new(path), bytes.into());
        self
    }

    fn get(&self, path: &VfsPath) -> Result<&Vec<u8>, VfsError> {
        self.files
            .get(path)
            .ok_or_else(|| VfsError::NotFound(path.clone()))
    }

    fn is_dir(&self, path: &VfsPath) -> bool {
        let prefix = format!("{path}/");
        self.files.keys().any(|p| p.as_str().starts_with(&prefix))
    }
}

impl Vfs for MemoryVfs {
    fn read(&self, path: &VfsPath) -> Result<Bytes, VfsError> {
        self.get(path).map(|bytes| Bytes::copy_from_slice(bytes))
    }

    fn metadata(&self, path: &VfsPath) -> Result<VfsMeta, VfsError> {
        if let Ok(bytes) = self.get(path) {
            return Ok(VfsMeta {
                size: bytes.len() as u64,
                mtime: Some(SystemTime::UNIX_EPOCH),
                kind: VfsKind::File,
            });
        }
        if self.is_dir(path) {
            return Ok(VfsMeta {
                size: 0,
                mtime: None,
                kind: VfsKind::Dir,
            });
        }
        Err(VfsError::NotFound(path.clone()))
    }

    fn list(&self, dir: &VfsPath) -> Result<Vec<VfsPath>, VfsError> {
        if !self.is_dir(dir) {
            return Err(VfsError::NotFound(dir.clone()));
        }
        let prefix = format!("{dir}/");
        Ok(self
            .files
            .keys()
            .filter(|p| p.as_str().starts_with(&prefix))
            .cloned()
            .collect())
    }

    fn fingerprint(&self, path: &VfsPath) -> Result<Fingerprint, VfsError> {
        self.get(path).map(Fingerprint::of)
    }
}

/// An artifact cache with no eviction policy beyond what `gc` is told.
#[derive(Debug, Default)]
pub struct MemoryCache {
    entries: Mutex<BTreeMap<Fingerprint, Vec<u8>>>,
}

impl MemoryCache {
    pub fn new() -> Self {
        Self::default()
    }
}

impl ArtifactCache for MemoryCache {
    fn get(&self, key: &Fingerprint) -> Option<Bytes> {
        let entries = self.entries.lock().ok()?;
        entries.get(key).map(|bytes| Bytes::copy_from_slice(bytes))
    }

    fn put(
        &self,
        key: &Fingerprint,
        value: Bytes,
        _inputs: &[Fingerprint],
    ) -> Result<(), CacheError> {
        let mut entries = self
            .entries
            .lock()
            .map_err(|_| CacheError("cache lock poisoned".to_owned()))?;
        entries.insert(*key, value.to_vec());
        Ok(())
    }

    fn gc(&self, max_bytes: u64, _max_age: Duration) -> Result<GcReport, CacheError> {
        let mut entries = self
            .entries
            .lock()
            .map_err(|_| CacheError("cache lock poisoned".to_owned()))?;
        let mut report = GcReport::default();
        let mut total: u64 = entries.values().map(|v| v.len() as u64).sum();
        while total > max_bytes {
            let Some(key) = entries.keys().next().copied() else {
                break;
            };
            if let Some(value) = entries.remove(&key) {
                report.removed += 1;
                report.bytes_freed += value.len() as u64;
                total -= value.len() as u64;
            }
        }
        Ok(report)
    }
}

/// A fixed-window limiter: enough to satisfy the contract, not enough to ship.
#[derive(Debug, Default)]
pub struct FixedWindowLimiter {
    limits: Mutex<BTreeMap<RateLimitPool, RateLimit>>,
    used: Mutex<BTreeMap<String, u32>>,
}

impl FixedWindowLimiter {
    pub fn new() -> Self {
        Self::default()
    }
}

impl RateLimiter for FixedWindowLimiter {
    fn check(&self, key: &RateLimitKey) -> Result<(), RetryAfter> {
        let limit = self
            .limits
            .lock()
            .ok()
            .and_then(|limits| limits.get(&key.pool).copied())
            .unwrap_or(RateLimit {
                per_minute: 60,
                burst: 1,
                daily: None,
            });
        let Ok(mut used) = self.used.lock() else {
            return Err(RetryAfter(Duration::from_secs(60)));
        };
        let slot = format!("{:?}/{:?}", key.pool, key.subject);
        let count = used.entry(slot).or_insert(0);
        if *count >= limit.burst {
            return Err(RetryAfter(Duration::from_secs(60)));
        }
        *count += 1;
        Ok(())
    }

    fn configure(&self, pool: RateLimitPool, limit: RateLimit) {
        if let Ok(mut limits) = self.limits.lock() {
            limits.insert(pool, limit);
        }
    }
}

/// Secrets from a map. Never used outside tests: real secrets are zeroized on
/// drop by their store, not held in a `BTreeMap<String, String>`.
#[derive(Debug, Default)]
pub struct MapSecrets {
    values: BTreeMap<String, String>,
}

impl MapSecrets {
    pub fn new() -> Self {
        Self::default()
    }

    #[must_use]
    pub fn with(mut self, name: impl Into<String>, value: impl Into<String>) -> Self {
        self.values.insert(name.into(), value.into());
        self
    }
}

impl SecretSource for MapSecrets {
    fn get(&self, name: &str) -> Option<zeroize::Zeroizing<String>> {
        self.values.get(name).cloned().map(zeroize::Zeroizing::new)
    }
}
