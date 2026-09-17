//! The JWKS cache (AUTH-03).
//!
//! Key rotation means a `kid` this server has never seen is normal, and the
//! answer is to refetch. That is also an amplifier: an attacker sending tokens
//! with random `kid`s turns one cheap request into one fetch against the
//! operator's identity provider each. AUTH-03 closes it with two limits that
//! have to work together —
//!
//! * a refresh happens **at most once per minute**, whatever arrives, and
//! * an unknown `kid` is **negatively cached for 60 s**, so the same unknown
//!   `kid` does not even consider a refresh again inside the window.
//!
//! Either alone is not enough. The refresh throttle alone still lets a flood
//! of distinct `kid`s each mark the cache stale; the negative cache alone still
//! lets a flood of distinct `kid`s each trigger a fetch.

use std::collections::BTreeMap;
use std::sync::RwLock;
use std::time::Duration;

use liyasa_core::net::BoxFut;
use serde::{Deserialize, Serialize};

use crate::auth::clock::{Clock, millis};

pub const REFRESH_INTERVAL: Duration = Duration::from_secs(60);
pub const NEGATIVE_TTL: Duration = Duration::from_secs(60);

/// One key, in the JWK members this server reads. Anything else in the
/// document is ignored rather than refused: a JWKS carries keys for consumers
/// that are not us.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct Jwk {
    #[serde(default)]
    pub kid: String,
    #[serde(default)]
    pub kty: String,
    #[serde(default)]
    pub alg: Option<String>,
    #[serde(rename = "use", default)]
    pub usage: Option<String>,
    /// RSA modulus and exponent.
    #[serde(default)]
    pub n: Option<String>,
    #[serde(default)]
    pub e: Option<String>,
    /// EC and OKP.
    #[serde(default)]
    pub crv: Option<String>,
    #[serde(default)]
    pub x: Option<String>,
    #[serde(default)]
    pub y: Option<String>,
    /// Symmetric.
    #[serde(default)]
    pub k: Option<String>,
}

#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct JwkSet {
    #[serde(default)]
    pub keys: Vec<Jwk>,
}

impl JwkSet {
    pub fn parse(text: &str) -> Option<Self> {
        serde_json::from_str(text).ok()
    }
}

/// Where a JWKS comes from. The production implementation is an HTTP fetch
/// through `liyasa-net`'s policy-checked client; a test supplies its own and
/// counts what it was asked for.
pub trait Source: std::fmt::Debug + Send + Sync {
    fn fetch(&self) -> BoxFut<'_, Result<JwkSet, String>>;
}

/// A source that never fetches, for an offline instance (HOST-08).
#[derive(Debug, Default, Clone, Copy)]
pub struct NoSource;

impl Source for NoSource {
    fn fetch(&self) -> BoxFut<'_, Result<JwkSet, String>> {
        Box::pin(async { Err("an offline instance fetches no JWKS (HOST-08)".to_owned()) })
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Resolution {
    Found(Box<Jwk>),
    /// The `kid` is not in the set and a refresh either happened and did not
    /// produce it, or was not allowed to happen yet.
    Unknown,
}

#[derive(Debug)]
pub struct Jwks {
    keys: RwLock<BTreeMap<String, Jwk>>,
    /// `kid` to the instant its negative entry expires.
    negative: RwLock<BTreeMap<String, i64>>,
    /// When the last fetch was attempted, successful or not: a failing
    /// provider must not be retried per request either.
    last_fetch_ms: RwLock<Option<i64>>,
    fetches: RwLock<u64>,
    clock: Clock,
}

impl Jwks {
    pub fn new(clock: Clock) -> Self {
        Self {
            keys: RwLock::new(BTreeMap::new()),
            negative: RwLock::new(BTreeMap::new()),
            last_fetch_ms: RwLock::new(None),
            fetches: RwLock::new(0),
            clock,
        }
    }

    /// Seeds the cache without a fetch, which is what a warm start and a test
    /// both want.
    pub fn install(&self, set: JwkSet) {
        let mut keys = self.keys.write().unwrap_or_else(|e| e.into_inner());
        for key in set.keys {
            // A key with no `kid` cannot be selected by one. It is kept under
            // the empty string so a JWKS with a single unnamed key still works,
            // which is what a small self-hosted provider looks like.
            keys.insert(key.kid.clone(), key);
        }
    }

    pub fn clock(&self) -> &Clock {
        &self.clock
    }

    /// How many times a source was actually asked. The anti-amplification
    /// property is a statement about this number.
    pub fn fetches(&self) -> u64 {
        *self.fetches.read().unwrap_or_else(|e| e.into_inner())
    }

    pub fn len(&self) -> usize {
        self.keys.read().unwrap_or_else(|e| e.into_inner()).len()
    }

    pub fn is_empty(&self) -> bool {
        self.len() == 0
    }

    pub async fn resolve(&self, kid: &str, source: &dyn Source) -> Resolution {
        if let Some(key) = self.cached(kid) {
            return Resolution::Found(Box::new(key));
        }
        let now = self.clock.now_ms();
        if self.negatively_cached(kid, now) {
            return Resolution::Unknown;
        }
        if !self.may_fetch(now) {
            // Not refetching is itself an answer about this `kid` for the rest
            // of the window: without this the flood merely moves from fetches
            // to negative-cache misses.
            self.remember_unknown(kid, now);
            return Resolution::Unknown;
        }

        *self
            .last_fetch_ms
            .write()
            .unwrap_or_else(|e| e.into_inner()) = Some(now);
        *self.fetches.write().unwrap_or_else(|e| e.into_inner()) += 1;
        match source.fetch().await {
            Ok(set) => self.install(set),
            Err(error) => {
                tracing::warn!(target: "liyasa_server", %error, "the JWKS could not be fetched");
            }
        }

        match self.cached(kid) {
            Some(key) => Resolution::Found(Box::new(key)),
            None => {
                self.remember_unknown(kid, now);
                Resolution::Unknown
            }
        }
    }

    fn cached(&self, kid: &str) -> Option<Jwk> {
        self.keys
            .read()
            .unwrap_or_else(|e| e.into_inner())
            .get(kid)
            .cloned()
    }

    fn negatively_cached(&self, kid: &str, now: i64) -> bool {
        let mut negative = self.negative.write().unwrap_or_else(|e| e.into_inner());
        match negative.get(kid) {
            Some(until) if *until > now => true,
            Some(_) => {
                negative.remove(kid);
                false
            }
            None => false,
        }
    }

    fn may_fetch(&self, now: i64) -> bool {
        match *self.last_fetch_ms.read().unwrap_or_else(|e| e.into_inner()) {
            None => true,
            Some(last) => now.saturating_sub(last) >= millis(REFRESH_INTERVAL),
        }
    }

    fn remember_unknown(&self, kid: &str, now: i64) {
        self.negative
            .write()
            .unwrap_or_else(|e| e.into_inner())
            .insert(kid.to_owned(), now + millis(NEGATIVE_TTL));
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::atomic::{AtomicU64, Ordering};

    #[derive(Debug)]
    struct Counting {
        set: JwkSet,
        calls: AtomicU64,
    }

    impl Counting {
        fn with(kids: &[&str]) -> Self {
            Self {
                set: JwkSet {
                    keys: kids
                        .iter()
                        .map(|kid| Jwk {
                            kid: (*kid).to_owned(),
                            kty: "RSA".to_owned(),
                            ..Jwk::default()
                        })
                        .collect(),
                },
                calls: AtomicU64::new(0),
            }
        }

        fn calls(&self) -> u64 {
            self.calls.load(Ordering::SeqCst)
        }
    }

    impl Source for Counting {
        fn fetch(&self) -> BoxFut<'_, Result<JwkSet, String>> {
            self.calls.fetch_add(1, Ordering::SeqCst);
            let set = self.set.clone();
            Box::pin(async move { Ok(set) })
        }
    }

    #[tokio::test]
    async fn a_known_kid_resolves_without_a_fetch() {
        let jwks = Jwks::new(Clock::manual());
        jwks.install(JwkSet {
            keys: vec![Jwk {
                kid: "a".to_owned(),
                kty: "RSA".to_owned(),
                ..Jwk::default()
            }],
        });
        let source = Counting::with(&[]);
        assert!(matches!(
            jwks.resolve("a", &source).await,
            Resolution::Found(_)
        ));
        assert_eq!(source.calls(), 0, "a cached key needs no fetch");
    }

    #[tokio::test]
    async fn an_unknown_kid_refreshes_once_and_then_resolves() {
        let jwks = Jwks::new(Clock::manual());
        let source = Counting::with(&["rotated"]);
        assert!(matches!(
            jwks.resolve("rotated", &source).await,
            Resolution::Found(_)
        ));
        assert_eq!(source.calls(), 1);
        // And again, from the cache.
        assert!(matches!(
            jwks.resolve("rotated", &source).await,
            Resolution::Found(_)
        ));
        assert_eq!(source.calls(), 1);
    }

    #[tokio::test]
    async fn a_flood_of_random_kids_produces_one_fetch_a_minute() {
        let jwks = Jwks::new(Clock::manual());
        let source = Counting::with(&["real"]);
        for n in 0..1_000 {
            assert_eq!(
                jwks.resolve(&format!("random-{n}"), &source).await,
                Resolution::Unknown
            );
        }
        assert_eq!(
            source.calls(),
            1,
            "a thousand unknown kids must not be a thousand fetches"
        );

        jwks.clock().advance(REFRESH_INTERVAL);
        for n in 1_000..2_000 {
            jwks.resolve(&format!("random-{n}"), &source).await;
        }
        assert_eq!(source.calls(), 2, "one more window, one more fetch");
    }

    #[tokio::test]
    async fn the_same_unknown_kid_is_negatively_cached_for_sixty_seconds() {
        let jwks = Jwks::new(Clock::manual());
        let source = Counting::with(&["real"]);
        assert_eq!(jwks.resolve("absent", &source).await, Resolution::Unknown);
        assert_eq!(source.calls(), 1);

        // Past the refresh window, but inside the negative window: still no
        // fetch, because this particular kid is known to be absent.
        jwks.clock().advance(REFRESH_INTERVAL);
        assert_eq!(jwks.resolve("absent", &source).await, Resolution::Unknown);
        assert_eq!(source.calls(), 1, "a negative entry suppresses the refetch");

        // Past both: the key may have appeared since.
        jwks.clock().advance(NEGATIVE_TTL);
        assert_eq!(jwks.resolve("absent", &source).await, Resolution::Unknown);
        assert_eq!(source.calls(), 2);
    }

    #[tokio::test]
    async fn a_rotated_key_is_picked_up_once_the_window_passes() {
        let jwks = Jwks::new(Clock::manual());
        let empty = Counting::with(&[]);
        assert_eq!(jwks.resolve("new", &empty).await, Resolution::Unknown);

        let rotated = Counting::with(&["new"]);
        jwks.clock().advance(REFRESH_INTERVAL + NEGATIVE_TTL);
        assert!(matches!(
            jwks.resolve("new", &rotated).await,
            Resolution::Found(_)
        ));
    }

    #[tokio::test]
    async fn a_failing_source_is_not_retried_per_request() {
        #[derive(Debug, Default)]
        struct Failing(AtomicU64);
        impl Source for Failing {
            fn fetch(&self) -> BoxFut<'_, Result<JwkSet, String>> {
                self.0.fetch_add(1, Ordering::SeqCst);
                Box::pin(async { Err("the provider is down".to_owned()) })
            }
        }
        let jwks = Jwks::new(Clock::manual());
        let source = Failing::default();
        for _ in 0..100 {
            assert_eq!(jwks.resolve("a", &source).await, Resolution::Unknown);
        }
        assert_eq!(source.0.load(Ordering::SeqCst), 1);
    }

    #[tokio::test]
    async fn an_offline_instance_resolves_only_what_it_was_given() {
        let jwks = Jwks::new(Clock::manual());
        jwks.install(JwkSet {
            keys: vec![Jwk {
                kid: "seeded".to_owned(),
                kty: "oct".to_owned(),
                ..Jwk::default()
            }],
        });
        assert!(matches!(
            jwks.resolve("seeded", &NoSource).await,
            Resolution::Found(_)
        ));
        assert_eq!(jwks.resolve("other", &NoSource).await, Resolution::Unknown);
    }

    #[test]
    fn a_jwks_document_parses_and_ignores_members_this_server_does_not_read() {
        let set = JwkSet::parse(
            r#"{"keys":[{"kid":"a","kty":"RSA","alg":"RS256","use":"sig","n":"AQAB","e":"AQAB",
                "x5c":["ignored"],"x5t":"ignored"}]}"#,
        )
        .expect("a JWKS");
        assert_eq!(set.keys.len(), 1);
        assert_eq!(set.keys[0].kid, "a");
        assert_eq!(set.keys[0].usage.as_deref(), Some("sig"));
        assert!(JwkSet::parse("not json").is_none());
        assert_eq!(JwkSet::parse("{}").map(|s| s.keys.len()), Some(0));
    }
}
