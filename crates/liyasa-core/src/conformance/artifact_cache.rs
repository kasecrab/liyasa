//! What every `ArtifactCache` must do (PRD §6.6, §34.9).

use std::time::Duration;

use super::require;
use crate::build::ArtifactCache;
use crate::ids::Fingerprint;

pub fn check(cache: &dyn ArtifactCache) {
    let key = Fingerprint::of(b"conformance/key");
    let other = Fingerprint::of(b"conformance/other");
    let input = Fingerprint::of(b"conformance/input");
    let value = crate::vfs::Bytes::from_static(b"cached value");

    require!(
        cache.get(&other).is_none(),
        "a key that was never put must miss"
    );

    cache
        .put(&key, value.clone(), &[input])
        .expect("put succeeds");
    require!(
        cache.get(&key).as_deref() == Some(value.as_ref()),
        "get returns exactly what put stored"
    );
    require!(
        cache.get(&other).is_none(),
        "put under one key must not populate another"
    );

    // The cache is content-addressed, so storing the same key twice is a no-op
    // rather than an error: two builds computing the same artifact race.
    cache
        .put(&key, value.clone(), &[input])
        .expect("putting the same key twice is not an error");
    require!(
        cache.get(&key).is_some(),
        "a repeated put must not evict the entry"
    );

    let report = cache.gc(0, Duration::ZERO).expect("gc succeeds");
    require!(
        report.removed > 0 || cache.get(&key).is_none(),
        "gc to a zero budget must either report what it removed or leave nothing behind"
    );
}
