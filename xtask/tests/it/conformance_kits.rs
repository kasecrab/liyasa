//! The conformance kits, run against `liyasa-core`'s reference implementations.
//!
//! A kit that passes everything is worth nothing, so each test here also feeds
//! the kit an implementation that breaks one rule and asserts the kit catches
//! it. `xtask` hosts these because it is the one crate that may turn the
//! `conformance` feature on.

use std::time::Duration;

use liyasa_core::build::{ArtifactCache, CacheError, GcReport};
use liyasa_core::conformance::fixtures::{FixedWindowLimiter, MapSecrets, MemoryCache, MemoryVfs};
use liyasa_core::conformance::{artifact_cache, rate_limiter, secret_source, vfs};
use liyasa_core::ids::Fingerprint;
use liyasa_core::server::{RateLimit, RateLimitKey, RateLimitPool, RateLimiter, RetryAfter};
use liyasa_core::vfs::{Bytes, Vfs, VfsError, VfsMeta, VfsPath};

fn vfs_fixture() -> (MemoryVfs, vfs::Fixture) {
    let vfs = MemoryVfs::new()
        .with("docs/install.md", b"# Install\n".to_vec())
        .with("docs/usage.md", b"# Usage\n".to_vec());
    let fixture = vfs::Fixture {
        file: (VfsPath::new("docs/install.md"), b"# Install\n".to_vec()),
        dir: VfsPath::new("docs"),
        missing: VfsPath::new("docs/nope.md"),
        denied: None,
    };
    (vfs, fixture)
}

#[test]
fn the_reference_vfs_passes_its_kit() {
    let (vfs, fixture) = vfs_fixture();
    vfs::check(&vfs, &fixture);
}

/// A `Vfs` whose fingerprint is not the hash of the bytes it returns, which is
/// the failure mode that silently breaks the artifact cache.
struct LyingFingerprint(MemoryVfs);

impl Vfs for LyingFingerprint {
    fn read(&self, path: &VfsPath) -> Result<Bytes, VfsError> {
        self.0.read(path)
    }
    fn metadata(&self, path: &VfsPath) -> Result<VfsMeta, VfsError> {
        self.0.metadata(path)
    }
    fn list(&self, dir: &VfsPath) -> Result<Vec<VfsPath>, VfsError> {
        self.0.list(dir)
    }
    fn fingerprint(&self, _path: &VfsPath) -> Result<Fingerprint, VfsError> {
        Ok(Fingerprint::of(b"always the same"))
    }
}

#[test]
#[should_panic(expected = "fingerprint is not blake3")]
fn the_vfs_kit_catches_a_fingerprint_that_is_not_the_content_hash() {
    let (vfs, fixture) = vfs_fixture();
    vfs::check(&LyingFingerprint(vfs), &fixture);
}

/// A `Vfs` that reports success for a path it cannot read.
struct NeverNotFound(MemoryVfs);

impl Vfs for NeverNotFound {
    fn read(&self, path: &VfsPath) -> Result<Bytes, VfsError> {
        Ok(self.0.read(path).unwrap_or_default())
    }
    fn metadata(&self, path: &VfsPath) -> Result<VfsMeta, VfsError> {
        self.0.metadata(path)
    }
    fn list(&self, dir: &VfsPath) -> Result<Vec<VfsPath>, VfsError> {
        self.0.list(dir)
    }
    fn fingerprint(&self, path: &VfsPath) -> Result<Fingerprint, VfsError> {
        self.0.fingerprint(path)
    }
}

#[test]
#[should_panic(expected = "not NotFound")]
fn the_vfs_kit_catches_a_missing_path_reported_as_empty() {
    let (vfs, fixture) = vfs_fixture();
    vfs::check(&NeverNotFound(vfs), &fixture);
}

#[test]
fn the_reference_cache_passes_its_kit() {
    artifact_cache::check(&MemoryCache::new());
}

/// A cache that drops writes, which a build would experience as a cache that
/// never warms rather than as an error.
#[derive(Default)]
struct ForgetfulCache;

impl ArtifactCache for ForgetfulCache {
    fn get(&self, _key: &Fingerprint) -> Option<Bytes> {
        None
    }
    fn put(&self, _: &Fingerprint, _: Bytes, _: &[Fingerprint]) -> Result<(), CacheError> {
        Ok(())
    }
    fn gc(&self, _: u64, _: Duration) -> Result<GcReport, CacheError> {
        Ok(GcReport::default())
    }
}

#[test]
#[should_panic(expected = "get returns exactly what put stored")]
fn the_cache_kit_catches_dropped_writes() {
    artifact_cache::check(&ForgetfulCache);
}

#[test]
fn the_reference_limiter_passes_its_kit() {
    rate_limiter::check(&FixedWindowLimiter::new());
}

/// A limiter that counts every pool and subject together, the mistake that
/// lets one noisy address throttle a whole site.
#[derive(Default)]
struct GlobalLimiter {
    used: std::sync::Mutex<u32>,
}

impl RateLimiter for GlobalLimiter {
    fn check(&self, _key: &RateLimitKey) -> Result<(), RetryAfter> {
        let Ok(mut used) = self.used.lock() else {
            return Err(RetryAfter(Duration::from_secs(60)));
        };
        *used += 1;
        if *used > 2 {
            Err(RetryAfter(Duration::from_secs(60)))
        } else {
            Ok(())
        }
    }
    fn configure(&self, _pool: RateLimitPool, _limit: RateLimit) {}
}

#[test]
#[should_panic(expected = "must not limit another")]
fn the_limiter_kit_catches_a_shared_counter() {
    rate_limiter::check(&GlobalLimiter::default());
}

#[test]
fn the_reference_secret_source_passes_its_kit() {
    let secrets = MapSecrets::new().with("stripe_key", "sk_test_123");
    secret_source::check(&secrets, "stripe_key", "sk_test_123", "nope");
}

/// A secret source that returns an empty string for an unknown name, which
/// turns a missing credential into a silently failing request.
struct EmptyForUnknown;

impl liyasa_core::verify::SecretSource for EmptyForUnknown {
    fn get(&self, name: &str) -> Option<zeroize::Zeroizing<String>> {
        Some(zeroize::Zeroizing::new(if name == "stripe_key" {
            "sk_test_123".to_owned()
        } else {
            String::new()
        }))
    }
}

#[test]
#[should_panic(expected = "never to an empty string")]
fn the_secret_kit_catches_an_empty_string_for_a_missing_name() {
    secret_source::check(&EmptyForUnknown, "stripe_key", "sk_test_123", "nope");
}
