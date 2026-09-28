//! The GitHub App flow: an RS256 JWT exchanged for an installation token
//! (GIT-01, `plan/rfcs/1602-the-app-signature-is-a-seam.md`).
//!
//! RFC 1602 deferred this because no RSA implementation was in the tree and
//! choosing one is a security decision with a `cargo deny` surface. That
//! premise expired: WP-15 added `ring 0.17` for JWT verification (RFC 1500),
//! and `ring::signature::RsaKeyPair` signs RSA-PKCS1-SHA256, which is RS256.
//! So the seam stays — `JwtSigner` is still how a caller supplies a signature
//! — and it finally has an implementation.
//!
//! **No private key is embedded anywhere in this repository.** The operator
//! supplies the PEM their GitHub App issued; every test here either builds a
//! signer from deliberately invalid bytes to prove the refusal, or injects a
//! stub signer to exercise the flow. The one thing not covered locally is that
//! `ring` produces a correct RS256 signature over the bytes it is handed,
//! which is `ring`'s own tested property rather than this crate's.

use std::sync::Mutex;
use std::time::Duration;

use base64::Engine as _;
use liyasa_core::net::{BoxFut, HttpClient, HttpPolicy, Method};
use serde_json::{Value, json};
use zeroize::Zeroizing;

use crate::provider::{Endpoint, GitError, JwtSigner, TokenSource, json_request, policy, send};

const NAME: &str = "github";

/// GitHub refuses a JWT whose `exp` is more than ten minutes ahead. Nine
/// leaves room for a slow request without tripping it.
pub const JWT_LIFETIME: Duration = Duration::from_secs(9 * 60);

/// GitHub's own guidance: backdate `iat` to tolerate a clock a little fast,
/// because a JWT from the future is refused outright.
pub const JWT_BACKDATE: Duration = Duration::from_secs(60);

/// How long before an installation token's stated expiry it is replaced. A
/// token that expires mid-request is a failure an operator cannot act on.
pub const RENEW_MARGIN: Duration = Duration::from_secs(5 * 60);

fn b64(bytes: &[u8]) -> String {
    base64::engine::general_purpose::URL_SAFE_NO_PAD.encode(bytes)
}

/// The two PEM labels a GitHub App private key arrives under.
///
/// GitHub issues PKCS#1 (`BEGIN RSA PRIVATE KEY`); a key round-tripped through
/// most tooling comes back as PKCS#8 (`BEGIN PRIVATE KEY`). Both are accepted
/// because an operator should not have to know which they have.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum KeyForm {
    Pkcs1,
    Pkcs8,
}

/// Strips PEM armour and returns the DER body with which form it was in.
pub fn der_of_pem(pem: &str) -> Result<(KeyForm, Vec<u8>), GitError> {
    let forms = [
        (
            "-----BEGIN RSA PRIVATE KEY-----",
            "-----END RSA PRIVATE KEY-----",
            KeyForm::Pkcs1,
        ),
        (
            "-----BEGIN PRIVATE KEY-----",
            "-----END PRIVATE KEY-----",
            KeyForm::Pkcs8,
        ),
    ];
    for (open, close, form) in forms {
        if let Some(rest) = pem.find(open).map(|at| &pem[at + open.len()..])
            && let Some(body) = rest.find(close).map(|at| &rest[..at])
        {
            let packed: String = body.chars().filter(|c| !c.is_whitespace()).collect();
            let der = base64::engine::general_purpose::STANDARD
                .decode(packed)
                .map_err(|_| {
                    GitError::BadUrl("the private key's base64 is not valid".to_owned())
                })?;
            return Ok((form, der));
        }
    }
    Err(GitError::NoCredential(
        "a GitHub App private key in PEM form",
    ))
}

/// An RS256 signer over a key the operator supplied.
pub struct RingSigner {
    key: ring::signature::RsaKeyPair,
    form: KeyForm,
}

impl std::fmt::Debug for RingSigner {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        // Never the key, on any path, including a panic message.
        f.debug_struct("RingSigner")
            .field("form", &self.form)
            .finish_non_exhaustive()
    }
}

impl RingSigner {
    /// Parses a PEM private key. A key this refuses is refused before any
    /// request is made, so a misconfigured app fails at startup rather than on
    /// the first webhook.
    pub fn from_pem(pem: &str) -> Result<Self, GitError> {
        let (form, der) = der_of_pem(pem)?;
        let key = match form {
            KeyForm::Pkcs1 => ring::signature::RsaKeyPair::from_der(&der),
            KeyForm::Pkcs8 => ring::signature::RsaKeyPair::from_pkcs8(&der),
        }
        .map_err(|_| {
            GitError::NoCredential("a usable RSA private key; this PEM did not parse as one")
        })?;
        Ok(Self { key, form })
    }

    pub fn form(&self) -> KeyForm {
        self.form
    }
}

impl JwtSigner for RingSigner {
    fn sign_rs256(&self, signing_input: &str) -> Result<String, GitError> {
        let mut signature = vec![0; self.key.public().modulus_len()];
        self.key
            .sign(
                &ring::signature::RSA_PKCS1_SHA256,
                &ring::rand::SystemRandom::new(),
                signing_input.as_bytes(),
                &mut signature,
            )
            .map_err(|_| GitError::NoCredential("a key that can sign; this one could not"))?;
        Ok(b64(&signature))
    }
}

/// The `header.claims` a JWT is signed over, and the claims GitHub requires.
///
/// Split from signing so the shape is testable without a key.
pub fn signing_input(app_id: &str, now_unix: i64) -> String {
    let header = json!({ "alg": "RS256", "typ": "JWT" });
    let claims = json!({
        "iat": now_unix - JWT_BACKDATE.as_secs() as i64,
        "exp": now_unix + JWT_LIFETIME.as_secs() as i64,
        "iss": app_id,
    });
    format!(
        "{}.{}",
        b64(header.to_string().as_bytes()),
        b64(claims.to_string().as_bytes())
    )
}

/// Milliseconds since the epoch, so a test can move time.
pub type Clock = std::sync::Arc<dyn Fn() -> i64 + Send + Sync>;

/// A cached installation token and when it stops being usable.
#[derive(Debug, Clone)]
struct Cached {
    token: Zeroizing<String>,
    /// Milliseconds since the epoch, already reduced by [`RENEW_MARGIN`].
    good_until_ms: i64,
}

/// The app flow as a [`TokenSource`]: mint a JWT, exchange it for an
/// installation token, and reuse that token until it is nearly expired.
pub struct AppInstallation {
    app_id: String,
    installation: u64,
    endpoint: Endpoint,
    http: std::sync::Arc<dyn HttpClient>,
    signer: std::sync::Arc<dyn JwtSigner>,
    policy: HttpPolicy,
    clock: Clock,
    cached: Mutex<Option<Cached>>,
}

impl std::fmt::Debug for AppInstallation {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("AppInstallation")
            .field("app_id", &self.app_id)
            .field("installation", &self.installation)
            .finish_non_exhaustive()
    }
}

impl AppInstallation {
    pub fn new(
        app_id: impl Into<String>,
        installation: u64,
        endpoint: Endpoint,
        http: std::sync::Arc<dyn HttpClient>,
        signer: std::sync::Arc<dyn JwtSigner>,
    ) -> Self {
        Self {
            app_id: app_id.into(),
            installation,
            endpoint,
            http,
            signer,
            policy: policy(liyasa_core::net::HostSet::default()),
            clock: std::sync::Arc::new(default_clock),
            cached: Mutex::new(None),
        }
    }

    /// Replaces the clock, so a test can drive expiry without waiting.
    ///
    /// An `Arc<dyn Fn>` rather than a `fn` pointer because the renewal margin
    /// is only testable with a clock that *moves*, and a bare `fn` cannot
    /// capture the cell that moves it.
    pub fn with_clock(mut self, clock: Clock) -> Self {
        self.clock = clock;
        self
    }

    pub fn with_allowed_hosts(mut self, hosts: liyasa_core::net::HostSet) -> Self {
        self.policy = policy(hosts);
        self
    }

    /// The JWT this app authenticates as itself with.
    pub fn jwt(&self) -> Result<String, GitError> {
        let input = signing_input(&self.app_id, (self.clock)() / 1000);
        let signature = self.signer.sign_rs256(&input)?;
        Ok(format!("{input}.{signature}"))
    }

    /// Whether a usable token is already held, for a test and for a caller
    /// that wants to know without triggering an exchange.
    pub fn cached_token(&self) -> Option<Zeroizing<String>> {
        let now = (self.clock)();
        let held = self.cached.lock().ok()?;
        held.as_ref()
            .filter(|c| c.good_until_ms > now)
            .map(|c| c.token.clone())
    }

    async fn exchange(&self) -> Result<Zeroizing<String>, GitError> {
        let jwt = self.jwt()?;
        let url = self.endpoint.join(&format!(
            "app/installations/{}/access_tokens",
            self.installation
        ))?;
        let request = json_request(
            Method::POST,
            url,
            &jwt,
            &[
                ("accept", "application/vnd.github+json"),
                ("x-github-api-version", crate::github::API_VERSION),
            ],
            // A body-less POST; GitHub reads the installation from the path.
            None,
        );
        let value = send(self.http.as_ref(), NAME, request, &self.policy).await?;
        let token = value
            .get("token")
            .and_then(Value::as_str)
            .ok_or(GitError::Malformed {
                provider: NAME,
                field: "token",
            })?;
        let good_until_ms = expiry_ms(value.get("expires_at").and_then(Value::as_str))
            .unwrap_or_else(|| (self.clock)() + RENEW_MARGIN.as_millis() as i64);
        let cached = Cached {
            token: Zeroizing::new(token.to_owned()),
            good_until_ms: good_until_ms - RENEW_MARGIN.as_millis() as i64,
        };
        let out = cached.token.clone();
        if let Ok(mut held) = self.cached.lock() {
            *held = Some(cached);
        }
        Ok(out)
    }
}

fn default_clock() -> i64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|since| since.as_millis() as i64)
        .unwrap_or_default()
}

/// `2026-09-28T12:00:00Z` to milliseconds. GitHub sends RFC 3339 with a `Z`,
/// and no date crate is in the tree, so this reads the fixed-width form and
/// declines anything else rather than guessing.
pub fn expiry_ms(text: Option<&str>) -> Option<i64> {
    let text = text?;
    let bytes = text.as_bytes();
    if bytes.len() < 20 || bytes[4] != b'-' || bytes[10] != b'T' || !text.ends_with('Z') {
        return None;
    }
    let n = |from: usize, to: usize| text.get(from..to)?.parse::<i64>().ok();
    let (y, mo, d) = (n(0, 4)?, n(5, 7)?, n(8, 10)?);
    let (h, mi, s) = (n(11, 13)?, n(14, 16)?, n(17, 19)?);
    if !(1..=12).contains(&mo) || !(1..=31).contains(&d) {
        return None;
    }
    // Days from the epoch by the civil-from-days algorithm, which needs no
    // date crate and is exact for every date GitHub will ever send.
    let y = if mo <= 2 { y - 1 } else { y };
    let era = if y >= 0 { y } else { y - 399 } / 400;
    let yoe = y - era * 400;
    let mp = (mo + 9) % 12;
    let doy = (153 * mp + 2) / 5 + d - 1;
    let doe = yoe * 365 + yoe / 4 - yoe / 100 + doy;
    let days = era * 146_097 + doe - 719_468;
    Some(((days * 86_400) + h * 3_600 + mi * 60 + s) * 1_000)
}

impl TokenSource for AppInstallation {
    fn token<'a>(&'a self) -> BoxFut<'a, Result<Zeroizing<String>, GitError>> {
        Box::pin(async move {
            match self.cached_token() {
                Some(token) => Ok(token),
                None => self.exchange().await,
            }
        })
    }
}

#[cfg(test)]
mod tests {
    use std::sync::Arc;

    use super::*;
    use crate::recorder::{Exchange, Recorded};

    /// Signs nothing and says so. The flow's shape is what these tests are
    /// about; `ring`'s signature correctness is `ring`'s own test suite.
    #[derive(Debug)]
    struct StubSigner(&'static str);

    impl JwtSigner for StubSigner {
        fn sign_rs256(&self, _input: &str) -> Result<String, GitError> {
            Ok(self.0.to_owned())
        }
    }

    /// A clock a test can move, which is what makes the renewal margin
    /// testable at all.
    fn movable(start: i64) -> (Clock, Arc<std::sync::atomic::AtomicI64>) {
        let now = Arc::new(std::sync::atomic::AtomicI64::new(start));
        let handle = now.clone();
        let clock: Clock = Arc::new(move || handle.load(std::sync::atomic::Ordering::SeqCst));
        (clock, now)
    }

    fn fixed(ms: i64) -> Clock {
        Arc::new(move || ms)
    }

    fn app(exchanges: Vec<Exchange>, clock: Clock) -> (AppInstallation, Arc<Recorded>) {
        let recorded = Arc::new(Recorded::new(exchanges));
        let app = AppInstallation::new(
            "123456",
            44556677,
            Endpoint::github_com(),
            recorded.clone(),
            Arc::new(StubSigner("sig")),
        )
        .with_clock(clock);
        (app, recorded)
    }

    fn token_response(expires_at: &str) -> String {
        json!({ "token": "ghs_installation", "expires_at": expires_at }).to_string()
    }

    #[test]
    fn a_jwt_carries_the_three_claims_github_requires() {
        let input = signing_input("123456", 1_800_000_000);
        let (header, claims) = input.split_once('.').expect("two segments");
        let decode = |s: &str| {
            let raw = base64::engine::general_purpose::URL_SAFE_NO_PAD
                .decode(s)
                .expect("base64url");
            serde_json::from_slice::<Value>(&raw).expect("json")
        };
        assert_eq!(decode(header)["alg"], "RS256");
        assert_eq!(decode(header)["typ"], "JWT");
        let claims = decode(claims);
        assert_eq!(claims["iss"], "123456");
        assert_eq!(
            claims["iat"], 1_799_999_940,
            "iat is backdated a minute, because a JWT from the future is refused"
        );
        assert_eq!(claims["exp"], 1_800_000_540);
    }

    #[test]
    fn the_jwt_never_outlives_githubs_ten_minute_cap() {
        let input = signing_input("1", 0);
        let claims = input.split_once('.').expect("two segments").1;
        let raw = base64::engine::general_purpose::URL_SAFE_NO_PAD
            .decode(claims)
            .expect("base64url");
        let claims: Value = serde_json::from_slice(&raw).expect("json");
        let span = claims["exp"].as_i64().expect("exp") - claims["iat"].as_i64().expect("iat");
        assert!(
            span <= 600,
            "iat to exp is {span}s; GitHub refuses a JWT spanning more than 600"
        );
    }

    #[test]
    fn a_jwt_is_three_base64url_segments_with_no_padding() {
        let (app, _) = app(Vec::new(), fixed(1_800_000_000_000));
        let jwt = app.jwt().expect("a jwt");
        assert_eq!(jwt.matches('.').count(), 2);
        assert!(!jwt.contains('='), "base64url for a JWT is unpadded: {jwt}");
        assert!(jwt.ends_with(".sig"));
    }

    #[test]
    fn both_pem_labels_are_accepted_and_anything_else_is_named() {
        let body = base64::engine::general_purpose::STANDARD.encode([1, 2, 3]);
        let pkcs1 =
            format!("-----BEGIN RSA PRIVATE KEY-----\n{body}\n-----END RSA PRIVATE KEY-----\n");
        let pkcs8 = format!("-----BEGIN PRIVATE KEY-----\n{body}\n-----END PRIVATE KEY-----\n");
        assert_eq!(der_of_pem(&pkcs1).expect("pkcs1").0, KeyForm::Pkcs1);
        assert_eq!(der_of_pem(&pkcs8).expect("pkcs8").0, KeyForm::Pkcs8);
        assert_eq!(der_of_pem(&pkcs1).expect("pkcs1").1, vec![1, 2, 3]);

        let error = der_of_pem("-----BEGIN CERTIFICATE-----\nYQ==\n-----END CERTIFICATE-----")
            .expect_err("a certificate is not a key");
        assert!(error.to_string().contains("private key"), "{error}");
    }

    #[test]
    fn a_pem_whose_body_is_not_base64_is_refused_before_any_request() {
        let pem = "-----BEGIN RSA PRIVATE KEY-----\nnot base64!!\n-----END RSA PRIVATE KEY-----";
        assert!(der_of_pem(pem).is_err());
    }

    #[test]
    fn a_pem_that_parses_but_is_not_a_key_is_refused_by_name() {
        // Three bytes of valid base64 that are not an RSA key: the PEM layer
        // accepts them and `ring` must be the one to say no.
        let body = base64::engine::general_purpose::STANDARD.encode([1, 2, 3]);
        let pem = format!("-----BEGIN RSA PRIVATE KEY-----\n{body}\n-----END RSA PRIVATE KEY-----");
        let error = RingSigner::from_pem(&pem).expect_err("not a key");
        assert!(error.to_string().contains("RSA private key"), "{error}");
    }

    #[test]
    fn a_signer_never_prints_its_key() {
        let body = base64::engine::general_purpose::STANDARD.encode([1, 2, 3]);
        let pem = format!("-----BEGIN RSA PRIVATE KEY-----\n{body}\n-----END RSA PRIVATE KEY-----");
        // It cannot be constructed from junk, which is the point of the test
        // above; this pins that the *error* does not carry the key either.
        let error = format!("{:?}", RingSigner::from_pem(&pem).expect_err("not a key"));
        assert!(!error.contains(&body), "{error}");
    }

    #[tokio::test]
    async fn the_jwt_is_exchanged_for_an_installation_token() {
        let (app, recorded) = app(
            vec![Exchange::post(
                "/app/installations/44556677/access_tokens",
                201,
                &token_response("2026-09-28T13:00:00Z"),
            )],
            fixed(1_790_000_000_000),
        );
        let token = app.token().await.expect("a token");
        assert_eq!(&*token, "ghs_installation");

        let made = recorded.made();
        assert_eq!(made.len(), 1);
        let auth = made[0]
            .header("authorization")
            .expect("an authorization header");
        assert!(auth.starts_with("Bearer "), "{auth}");
        assert!(
            auth.ends_with(".sig"),
            "the app authenticates with its JWT: {auth}"
        );
        assert_eq!(
            made[0].header("x-github-api-version"),
            Some(crate::github::API_VERSION)
        );
    }

    #[tokio::test]
    async fn a_held_token_is_reused_rather_than_exchanged_again() {
        // One recorded exchange and two calls: the second must not reach the
        // network, which is the only way this can pass.
        let (app, recorded) = app(
            vec![Exchange::post(
                "/app/installations/44556677/access_tokens",
                201,
                &token_response("2026-09-28T13:00:00Z"),
            )],
            fixed(1_790_000_000_000),
        );
        assert_eq!(&*app.token().await.expect("a token"), "ghs_installation");
        assert_eq!(
            &*app.token().await.expect("the held token"),
            "ghs_installation"
        );
        assert_eq!(recorded.made().len(), 1, "the second call reused the token");
        assert_eq!(recorded.unused(), 0);
    }

    #[tokio::test]
    async fn a_token_inside_the_renewal_margin_is_replaced() {
        let expires = expiry_ms(Some("2026-09-28T13:00:00Z")).expect("an expiry");
        let (clock, now) = movable(expires - 60 * 60 * 1_000);
        let (app, recorded) = app(
            vec![
                Exchange::post(
                    "/app/installations/44556677/access_tokens",
                    201,
                    &token_response("2026-09-28T13:00:00Z"),
                ),
                Exchange::post(
                    "/app/installations/44556677/access_tokens",
                    201,
                    &json!({ "token": "ghs_second", "expires_at": "2026-09-28T14:00:00Z" })
                        .to_string(),
                ),
            ],
            clock,
        );

        assert_eq!(&*app.token().await.expect("a token"), "ghs_installation");
        assert!(app.cached_token().is_some(), "well clear of the margin");

        // One instance, one cache, time moved to four minutes before expiry —
        // inside the five-minute margin, so the held token must not be offered.
        now.store(
            expires - 4 * 60 * 1_000,
            std::sync::atomic::Ordering::SeqCst,
        );
        assert!(
            app.cached_token().is_none(),
            "a token expiring in four minutes is inside the renewal margin"
        );
        assert_eq!(&*app.token().await.expect("a fresh token"), "ghs_second");
        assert_eq!(recorded.made().len(), 2, "it exchanged again");
        assert_eq!(recorded.unused(), 0);
    }

    #[test]
    fn an_expiry_is_read_or_declined_rather_than_guessed() {
        // Every constant here was computed rather than recalled. The first
        // draft of this test carried 1_790_380_800_000 for the date below,
        // which is two days early: the assertion was wrong and the arithmetic
        // was right, so "fix the code until the test passes" would have broken
        // working code. Cross-checked against Python's `datetime`.
        assert_eq!(
            expiry_ms(Some("2026-09-28T00:00:00Z")),
            Some(1_790_553_600_000)
        );
        assert_eq!(
            expiry_ms(Some("2026-09-28T13:00:00Z")),
            Some(1_790_600_400_000),
            "the hour, minute and second are carried, not just the date"
        );
        assert_eq!(
            expiry_ms(Some("2024-02-29T12:00:00Z")),
            Some(1_709_208_000_000),
            "a leap day, which the civil-from-days arithmetic has to get right"
        );
        assert_eq!(
            expiry_ms(Some("2000-03-01T00:00:00Z")),
            Some(951_868_800_000),
            "the day after a century leap year, where the era arithmetic turns over"
        );
        assert_eq!(
            expiry_ms(Some("1970-01-01T00:00:01Z")),
            Some(1_000),
            "the epoch itself, as the arithmetic's fixed point"
        );
        for bad in [
            None,
            Some(""),
            Some("2026-09-28"),
            Some("not a date at all"),
            Some("2026-13-01T00:00:00Z"),
            Some("2026-09-28T00:00:00+01:00"),
        ] {
            assert_eq!(expiry_ms(bad), None, "{bad:?}");
        }
    }

    #[tokio::test]
    async fn a_refused_exchange_says_what_github_said() {
        let (app, _) = app(
            vec![Exchange::post(
                "/app/installations/44556677/access_tokens",
                401,
                &json!({ "message": "A JWT could not be decoded" }).to_string(),
            )],
            fixed(1_790_000_000_000),
        );
        let error = app.token().await.expect_err("a refusal");
        assert!(
            error.to_string().contains("A JWT could not be decoded"),
            "{error}"
        );
    }

    #[tokio::test]
    async fn a_response_with_no_token_is_malformed_rather_than_an_empty_token() {
        let (app, _) = app(
            vec![Exchange::post(
                "/app/installations/44556677/access_tokens",
                201,
                &json!({ "expires_at": "2026-09-28T13:00:00Z" }).to_string(),
            )],
            fixed(1_790_000_000_000),
        );
        let error = app.token().await.expect_err("a refusal");
        assert_eq!(
            error,
            GitError::Malformed {
                provider: "github",
                field: "token"
            }
        );
    }
}
