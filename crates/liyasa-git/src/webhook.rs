//! Inbound webhook verification (GIT-01).
//!
//! Every provider is held to the same three rules before a single byte of the
//! payload is parsed: an HMAC-SHA256 signature over the raw body compared in
//! constant time, a freshness window of five minutes where the provider sends
//! a timestamp, and rejection of a delivery ID seen in the last twenty-four
//! hours. A rejection returns [`Rejection`], which the server reports as
//! `E0808`; nothing downstream of here runs on a delivery that did not pass.
//!
//! GitLab is the exception to the first rule and not to the others: it sends a
//! shared token rather than a signature, so the comparison is constant-time
//! over the token. That is GitLab's protocol, not a weaker policy — the
//! replay and freshness rules still apply.

use std::collections::HashMap;
use std::sync::Mutex;
use std::time::Duration;

use hmac::{Hmac, KeyInit, Mac};
use sha2::Sha256;

/// The freshness window of GIT-01, for a provider that sends a timestamp.
pub const FRESHNESS: Duration = Duration::from_secs(5 * 60);

/// How long a delivery ID is remembered so a replay of it is refused.
pub const REPLAY_RETENTION: Duration = Duration::from_secs(24 * 60 * 60);

/// Past this many remembered deliveries, expired entries are swept before the
/// next insert. A sweep is linear, so it is amortised rather than per-request.
const SWEEP_AT: usize = 4096;

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum Provider {
    GitHub,
    GitLab,
    Bitbucket,
    /// Any other host, verified with the same envelope Liyasa's own outbound
    /// deliveries carry, so an operator's forwarder needs no second scheme.
    Generic,
}

impl Provider {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::GitHub => "github",
            Self::GitLab => "gitlab",
            Self::Bitbucket => "bitbucket",
            Self::Generic => "generic",
        }
    }

    pub fn parse(text: &str) -> Option<Self> {
        match text {
            "github" => Some(Self::GitHub),
            "gitlab" => Some(Self::GitLab),
            "bitbucket" => Some(Self::Bitbucket),
            "generic" => Some(Self::Generic),
            _ => None,
        }
    }

    /// The header carrying the signature, or the shared token for GitLab.
    pub fn signature_header(self) -> &'static str {
        match self {
            // GitHub Enterprise Server sends the same headers as github.com.
            Self::GitHub => "x-hub-signature-256",
            Self::GitLab => "x-gitlab-token",
            Self::Bitbucket => "x-hub-signature",
            Self::Generic => "x-liyasa-signature",
        }
    }

    pub fn delivery_header(self) -> &'static str {
        match self {
            Self::GitHub => "x-github-delivery",
            Self::GitLab => "x-gitlab-event-uuid",
            Self::Bitbucket => "x-request-uuid",
            Self::Generic => "x-liyasa-delivery",
        }
    }

    pub fn event_header(self) -> &'static str {
        match self {
            Self::GitHub => "x-github-event",
            Self::GitLab => "x-gitlab-event",
            Self::Bitbucket => "x-event-key",
            Self::Generic => "x-liyasa-event",
        }
    }

    /// The header carrying seconds since the epoch, for the providers that
    /// send one. GIT-01's freshness window applies only where there is one.
    pub fn timestamp_header(self) -> Option<&'static str> {
        match self {
            Self::Generic => Some("x-liyasa-timestamp"),
            _ => None,
        }
    }

    /// Whether the signature header holds an HMAC or a shared token.
    fn signs_the_body(self) -> bool {
        !matches!(self, Self::GitLab)
    }
}

#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
pub enum Rejection {
    #[error("no `{0}` header")]
    MissingSignature(&'static str),
    #[error("the signature does not match the body")]
    BadSignature,
    #[error("no `{0}` header, so a replay could not be ruled out")]
    MissingDelivery(&'static str),
    #[error("delivery `{id}` was already accepted")]
    Replayed { id: String },
    #[error("the timestamp is {skew}s away from now, outside the {window}s window")]
    Stale { skew: i64, window: u64 },
    #[error("`{0}` is not a Unix timestamp")]
    BadTimestamp(String),
    #[error("no `{0}` header")]
    MissingEvent(&'static str),
}

/// A delivery that passed every rule.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Verified {
    pub provider: Provider,
    /// The provider's event name, verbatim: `push`, `Push Hook`, `repo:push`.
    pub event: String,
    pub delivery: String,
}

/// The raw request, before anything has parsed it.
#[derive(Debug, Clone, Copy)]
pub struct Delivery<'a> {
    pub provider: Provider,
    pub headers: &'a [(String, String)],
    pub body: &'a [u8],
}

impl<'a> Delivery<'a> {
    pub fn header(&self, name: &str) -> Option<&'a str> {
        self.headers
            .iter()
            .find(|(key, _)| key.eq_ignore_ascii_case(name))
            .map(|(_, value)| value.as_str())
    }
}

/// `sha256=<hex>` over the raw body.
pub fn sign(secret: &str, body: &[u8]) -> String {
    // HMAC takes a key of any length, so this is infallible for any secret.
    let mut mac = <Hmac<Sha256> as KeyInit>::new_from_slice(secret.as_bytes())
        .unwrap_or_else(|_| <Hmac<Sha256> as KeyInit>::new_from_slice(&[]).expect("an empty key"));
    mac.update(body);
    let tag = mac.finalize().into_bytes();
    let mut out = String::from("sha256=");
    for byte in tag {
        use std::fmt::Write as _;
        let _ = write!(out, "{byte:02x}");
    }
    out
}

/// Constant time over the whole of both strings. Length is compared first
/// because it is not secret and a slice of differing length cannot be zipped.
pub fn constant_time_eq(a: &str, b: &str) -> bool {
    if a.len() != b.len() {
        return false;
    }
    a.bytes()
        .zip(b.bytes())
        .fold(0u8, |acc, (x, y)| acc | (x ^ y))
        == 0
}

/// The delivery IDs accepted recently, so a replay of one is refused
/// (GIT-01). In-process: a replica that restarts forgets, which is the
/// direction that fails safe for availability, and the provider's own retry of
/// an unacknowledged delivery is not a replay attack.
#[derive(Debug)]
pub struct ReplayCache {
    seen: Mutex<HashMap<String, i64>>,
    retention: Duration,
}

impl Default for ReplayCache {
    fn default() -> Self {
        Self::new(REPLAY_RETENTION)
    }
}

impl ReplayCache {
    pub fn new(retention: Duration) -> Self {
        Self {
            seen: Mutex::new(HashMap::new()),
            retention,
        }
    }

    /// Records `id` at `now`, returning false if it was already recorded
    /// inside the retention window.
    pub fn accept(&self, id: &str, now: i64) -> bool {
        let horizon = now.saturating_sub(self.retention.as_secs() as i64);
        let Ok(mut seen) = self.seen.lock() else {
            // A poisoned lock means another thread panicked mid-insert. Refuse
            // rather than wave the delivery through unchecked.
            return false;
        };
        if seen.len() >= SWEEP_AT {
            seen.retain(|_, at| *at > horizon);
        }
        match seen.get(id) {
            Some(at) if *at > horizon => false,
            _ => {
                seen.insert(id.to_owned(), now);
                true
            }
        }
    }

    pub fn len(&self) -> usize {
        self.seen.lock().map(|seen| seen.len()).unwrap_or(0)
    }

    pub fn is_empty(&self) -> bool {
        self.len() == 0
    }
}

/// One provider's secret and the replay memory that goes with it.
#[derive(Debug)]
pub struct Verifier {
    secret: String,
    window: Duration,
    replays: ReplayCache,
}

impl Verifier {
    pub fn new(secret: impl Into<String>) -> Self {
        Self {
            secret: secret.into(),
            window: FRESHNESS,
            replays: ReplayCache::default(),
        }
    }

    pub fn with_window(mut self, window: Duration) -> Self {
        self.window = window;
        self
    }

    pub fn with_retention(mut self, retention: Duration) -> Self {
        self.replays = ReplayCache::new(retention);
        self
    }

    pub fn replays(&self) -> &ReplayCache {
        &self.replays
    }

    /// Checks signature, then freshness, then replay, then the event name.
    ///
    /// The order matters: an unsigned request must not be able to put an entry
    /// in the replay cache, which is why the signature is first and the cache
    /// is written last.
    pub fn verify(&self, delivery: &Delivery<'_>, now: i64) -> Result<Verified, Rejection> {
        let provider = delivery.provider;
        let header = provider.signature_header();
        let presented = delivery
            .header(header)
            .ok_or(Rejection::MissingSignature(header))?;
        let expected = match provider.signs_the_body() {
            true => sign(&self.secret, delivery.body),
            false => self.secret.clone(),
        };
        if !constant_time_eq(&expected, presented) {
            return Err(Rejection::BadSignature);
        }

        if let Some(name) = provider.timestamp_header() {
            let raw = delivery
                .header(name)
                .ok_or(Rejection::MissingSignature(name))?;
            let sent: i64 = raw
                .trim()
                .parse()
                .map_err(|_| Rejection::BadTimestamp(raw.to_owned()))?;
            let skew = now - sent;
            if skew.abs() > self.window.as_secs() as i64 {
                return Err(Rejection::Stale {
                    skew,
                    window: self.window.as_secs(),
                });
            }
        }

        let delivery_header = provider.delivery_header();
        let id = delivery
            .header(delivery_header)
            .ok_or(Rejection::MissingDelivery(delivery_header))?;
        let event_header = provider.event_header();
        let event = delivery
            .header(event_header)
            .ok_or(Rejection::MissingEvent(event_header))?;

        if !self.replays.accept(id, now) {
            return Err(Rejection::Replayed { id: id.to_owned() });
        }
        Ok(Verified {
            provider,
            event: event.to_owned(),
            delivery: id.to_owned(),
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const SECRET: &str = "a-webhook-secret-of-some-length";
    const BODY: &[u8] = br#"{"ref":"refs/heads/main"}"#;

    fn headers(pairs: &[(&str, &str)]) -> Vec<(String, String)> {
        pairs
            .iter()
            .map(|(k, v)| ((*k).to_owned(), (*v).to_owned()))
            .collect()
    }

    fn github(extra: &[(&str, &str)]) -> Vec<(String, String)> {
        let signature = sign(SECRET, BODY);
        let mut rows = headers(&[("X-GitHub-Event", "push"), ("X-GitHub-Delivery", "d-1")]);
        rows.push(("X-Hub-Signature-256".to_owned(), signature));
        rows.extend(headers(extra));
        rows
    }

    #[test]
    fn a_signed_github_delivery_is_accepted_once_and_never_again() {
        let verifier = Verifier::new(SECRET);
        let rows = github(&[]);
        let delivery = Delivery {
            provider: Provider::GitHub,
            headers: &rows,
            body: BODY,
        };
        let verified = verifier
            .verify(&delivery, 1_000)
            .expect("a signed delivery");
        assert_eq!(verified.event, "push");
        assert_eq!(verified.delivery, "d-1");
        assert_eq!(
            verifier.verify(&delivery, 1_001),
            Err(Rejection::Replayed {
                id: "d-1".to_owned()
            }),
            "the same delivery ID must not be accepted twice"
        );
    }

    #[test]
    fn a_body_edited_after_signing_is_refused() {
        let verifier = Verifier::new(SECRET);
        let rows = github(&[]);
        let delivery = Delivery {
            provider: Provider::GitHub,
            headers: &rows,
            body: br#"{"ref":"refs/heads/evil"}"#,
        };
        assert_eq!(
            verifier.verify(&delivery, 1_000),
            Err(Rejection::BadSignature)
        );
    }

    #[test]
    fn another_secret_does_not_verify() {
        let verifier = Verifier::new("a-different-secret-entirely");
        let rows = github(&[]);
        let delivery = Delivery {
            provider: Provider::GitHub,
            headers: &rows,
            body: BODY,
        };
        assert_eq!(
            verifier.verify(&delivery, 1_000),
            Err(Rejection::BadSignature)
        );
    }

    #[test]
    fn an_unsigned_delivery_never_reaches_the_replay_cache() {
        let verifier = Verifier::new(SECRET);
        let rows = headers(&[("X-GitHub-Event", "push"), ("X-GitHub-Delivery", "d-9")]);
        let delivery = Delivery {
            provider: Provider::GitHub,
            headers: &rows,
            body: BODY,
        };
        assert_eq!(
            verifier.verify(&delivery, 1_000),
            Err(Rejection::MissingSignature("x-hub-signature-256"))
        );
        assert!(
            verifier.replays().is_empty(),
            "an unsigned request must not be able to burn a delivery ID"
        );
    }

    #[test]
    fn a_delivery_with_no_id_is_refused_rather_than_trusted() {
        let verifier = Verifier::new(SECRET);
        let mut rows = headers(&[("X-GitHub-Event", "push")]);
        rows.push(("X-Hub-Signature-256".to_owned(), sign(SECRET, BODY)));
        let delivery = Delivery {
            provider: Provider::GitHub,
            headers: &rows,
            body: BODY,
        };
        assert_eq!(
            verifier.verify(&delivery, 1_000),
            Err(Rejection::MissingDelivery("x-github-delivery"))
        );
    }

    #[test]
    fn a_replayed_id_is_accepted_again_once_it_falls_out_of_retention() {
        let verifier = Verifier::new(SECRET).with_retention(Duration::from_secs(100));
        let rows = github(&[]);
        let delivery = Delivery {
            provider: Provider::GitHub,
            headers: &rows,
            body: BODY,
        };
        assert!(verifier.verify(&delivery, 1_000).is_ok());
        assert!(verifier.verify(&delivery, 1_050).is_err());
        assert!(
            verifier.verify(&delivery, 1_200).is_ok(),
            "the memory is a window, not a permanent ledger"
        );
    }

    #[test]
    fn a_generic_delivery_outside_the_window_is_stale_in_both_directions() {
        let verifier = Verifier::new(SECRET);
        let signature = sign(SECRET, BODY);
        for (stamp, label) in [("600", "too old"), ("1500", "from the future")] {
            let mut rows = headers(&[
                ("X-Liyasa-Event", "push"),
                ("X-Liyasa-Delivery", "g-1"),
                ("X-Liyasa-Timestamp", stamp),
            ]);
            rows.push(("X-Liyasa-Signature".to_owned(), signature.clone()));
            let delivery = Delivery {
                provider: Provider::Generic,
                headers: &rows,
                body: BODY,
            };
            assert!(
                matches!(
                    verifier.verify(&delivery, 1_000),
                    Err(Rejection::Stale { .. })
                ),
                "{label}"
            );
        }
    }

    #[test]
    fn a_generic_delivery_inside_the_window_passes() {
        let verifier = Verifier::new(SECRET);
        let mut rows = headers(&[
            ("X-Liyasa-Event", "push"),
            ("X-Liyasa-Delivery", "g-2"),
            ("X-Liyasa-Timestamp", "900"),
        ]);
        rows.push(("X-Liyasa-Signature".to_owned(), sign(SECRET, BODY)));
        let delivery = Delivery {
            provider: Provider::Generic,
            headers: &rows,
            body: BODY,
        };
        assert!(verifier.verify(&delivery, 1_000).is_ok());
    }

    #[test]
    fn gitlab_compares_a_token_and_is_still_held_to_replay() {
        let verifier = Verifier::new(SECRET);
        let rows = headers(&[
            ("X-Gitlab-Event", "Push Hook"),
            ("X-Gitlab-Event-UUID", "u-1"),
            ("X-Gitlab-Token", SECRET),
        ]);
        let delivery = Delivery {
            provider: Provider::GitLab,
            headers: &rows,
            body: BODY,
        };
        let verified = verifier.verify(&delivery, 10).expect("the token matches");
        assert_eq!(verified.event, "Push Hook");
        assert!(verifier.verify(&delivery, 11).is_err(), "still no replays");

        let wrong = headers(&[
            ("X-Gitlab-Event", "Push Hook"),
            ("X-Gitlab-Event-UUID", "u-2"),
            ("X-Gitlab-Token", "not-the-token"),
        ]);
        assert_eq!(
            verifier.verify(
                &Delivery {
                    provider: Provider::GitLab,
                    headers: &wrong,
                    body: BODY,
                },
                12
            ),
            Err(Rejection::BadSignature)
        );
    }

    #[test]
    fn bitbucket_signs_the_body_under_its_own_header_names() {
        let verifier = Verifier::new(SECRET);
        let mut rows = headers(&[("X-Event-Key", "repo:push"), ("X-Request-UUID", "b-1")]);
        rows.push(("X-Hub-Signature".to_owned(), sign(SECRET, BODY)));
        let delivery = Delivery {
            provider: Provider::Bitbucket,
            headers: &rows,
            body: BODY,
        };
        assert_eq!(
            verifier.verify(&delivery, 5).expect("signed").event,
            "repo:push"
        );
    }

    #[test]
    fn a_known_vector_pins_the_signature_format() {
        // The same vector `liyasa-server` pins for outbound deliveries, so a
        // forwarder that re-signs what it received computes the same string.
        assert_eq!(
            sign("key", b"body"),
            "sha256=515aae133b435d4000956731f68ae5cf5eb85d4f0dc6a546d2bfcd3595ec1ae1"
        );
    }

    #[test]
    fn every_provider_name_round_trips() {
        for provider in [
            Provider::GitHub,
            Provider::GitLab,
            Provider::Bitbucket,
            Provider::Generic,
        ] {
            assert_eq!(Provider::parse(provider.as_str()), Some(provider));
        }
        assert_eq!(Provider::parse("gitea"), None);
    }

    #[test]
    fn a_header_lookup_ignores_case() {
        let rows = headers(&[("X-GitHub-Event", "push")]);
        let delivery = Delivery {
            provider: Provider::GitHub,
            headers: &rows,
            body: BODY,
        };
        assert_eq!(delivery.header("x-github-event"), Some("push"));
        assert_eq!(delivery.header("X-GITHUB-EVENT"), Some("push"));
        assert_eq!(delivery.header("x-github-delivery"), None);
    }

    #[test]
    fn the_replay_cache_sweeps_rather_than_growing_without_bound() {
        let cache = ReplayCache::new(Duration::from_secs(10));
        for n in 0..SWEEP_AT {
            assert!(cache.accept(&format!("old-{n}"), 0));
        }
        assert!(cache.accept("new", 1_000));
        assert!(
            cache.len() < SWEEP_AT,
            "entries past retention are swept, not kept for ever"
        );
    }
}
