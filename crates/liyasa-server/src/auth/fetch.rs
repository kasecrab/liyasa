//! The outbound half of JWT and OIDC (AUTH-03, AUTH-04).
//!
//! [`super::jwks::Source`] and [`super::oidc::Exchange`] were traits with test
//! doubles and no production implementation, so JWT mode could never fetch a
//! JWKS and an OIDC callback could never exchange a code. Both surfaces
//! answered correctly and reached nothing — the pattern this package's
//! requirement rows are full of.
//!
//! Everything here goes through `liyasa-net`'s client, which is the only thing
//! in the workspace allowed to make an outbound request: it resolves with
//! hickory, validates every address against the denied classes, pins the
//! validated set for the hop, and never follows a redirect on its own
//! (PRD §30.2.3). An identity provider is exactly the sort of host an SSRF
//! wants to be pointed at, so none of that is optional here.
//!
//! **Why `Purpose::FactSource`.** `liyasa_core::net::Purpose` has no variant
//! for an identity provider, and `liyasa_net::policy::requires_https` enforces
//! TLS for `FactSource`, `SpecRef` and `AgentFetch` only. A JWKS fetched over
//! plaintext is a signing-key substitution — an attacker who can rewrite the
//! response chooses who is signed in — so the purpose is picked for the
//! behaviour it enforces rather than for its name.
// TODO(rfc-1504): `Purpose::Auth`, so the label matches. `Purpose` is a frozen
// core contract and the variant is WP-00's to add.

use std::sync::Arc;
use std::time::Duration;

use liyasa_core::net::{
    BoxFut, HostSet, HttpClient, HttpPolicy, HttpRequest, HttpResponse, Method, Purpose, Url,
};

use crate::auth::jwks::{JwkSet, Source};
use crate::auth::oidc::{Endpoints, Exchange, TokenResponse};

/// A JWKS is a handful of keys; a token response is smaller. Anything larger
/// than this from an identity provider is not one, and reading it would be
/// doing an attacker's memory allocation for them.
const MAX_BYTES: u64 = 256 * 1024;
const TIMEOUT: Duration = Duration::from_secs(10);

/// The policy every request here runs under.
///
/// `allow_private` is false: an identity provider is on the public internet,
/// and a `jwksUrl` pointing at `169.254.169.254` or `10.0.0.1` is either a
/// mistake or an attack. `max_redirects` is zero — a JWKS URL that redirects
/// is one an attacker may be steering, and the configured URL is the one that
/// was trusted.
fn policy() -> HttpPolicy {
    HttpPolicy {
        allow_hosts: HostSet::default(),
        deny_hosts: HostSet::default(),
        allow_private: false,
        max_redirects: 0,
        max_bytes: MAX_BYTES,
        timeout: TIMEOUT,
        purpose: Purpose::FactSource,
    }
}

fn get(url: Url) -> HttpRequest {
    HttpRequest {
        method: Method::GET,
        url,
        headers: vec![("accept".to_owned(), "application/json".to_owned())],
        body: None,
    }
}

/// A 2xx body, or a reason. A non-2xx from an identity provider is reported
/// with its status because "the provider said 403" and "the provider is
/// unreachable" need different things done about them.
fn body_of(response: HttpResponse) -> Result<Vec<u8>, String> {
    match response.status {
        200..=299 => Ok(response.body.to_vec()),
        status => Err(format!("the provider answered {status}")),
    }
}

/// `auth.jwt.jwksUrl`, fetched.
///
/// The caching, the once-a-minute refresh ceiling and the negative cache all
/// live in [`super::jwks::Jwks`] and are deliberately **not** repeated here:
/// this is the thing that makes a request, and it makes one every time it is
/// called. Putting a second cache here would mean two expiry policies that
/// could disagree, and the one that matters is the one AUTH-03 specifies.
pub struct HttpJwks {
    client: Arc<dyn HttpClient>,
    url: Url,
}

// `HttpClient` is not `Debug` and `Source` requires it, so these print what
// identifies the source and not the client behind it.
impl std::fmt::Debug for HttpJwks {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("HttpJwks")
            .field("url", &self.url.as_str())
            .finish_non_exhaustive()
    }
}

impl HttpJwks {
    /// `None` when the configured value is not a URL — reported at startup by
    /// the caller rather than per request.
    pub fn new(client: Arc<dyn HttpClient>, jwks_url: &str) -> Option<Self> {
        Some(Self {
            client,
            url: Url::parse(jwks_url).ok()?,
        })
    }

    pub fn url(&self) -> &Url {
        &self.url
    }
}

impl Source for HttpJwks {
    fn fetch(&self) -> BoxFut<'_, Result<JwkSet, String>> {
        Box::pin(async move {
            let response = self
                .client
                .fetch(get(self.url.clone()), &policy())
                .await
                .map_err(|error| error.to_string())?;
            let body = body_of(response)?;
            // A JWKS that does not parse is not an empty JWKS. Returning
            // `Ok(empty)` would install nothing and look like a provider with
            // no keys, and every `kid` would then be negatively cached as
            // absent rather than reported as a broken document.
            JwkSet::parse(&String::from_utf8_lossy(&body))
                .ok_or_else(|| "the JWKS did not parse as a JWKS document".to_owned())
        })
    }
}

/// OIDC discovery: `<issuer>/.well-known/openid-configuration`.
///
/// Verifies that the document names the issuer it was fetched for. A provider
/// mix-up is how a reader signs in against one identity provider and is
/// admitted as the equivalent subject at another.
pub async fn discover(client: &dyn HttpClient, issuer: &str) -> Result<Endpoints, String> {
    let url = Url::parse(&Endpoints::discovery_url(issuer))
        .map_err(|error| format!("`{issuer}` is not a URL: {error}"))?;
    let response = client
        .fetch(get(url), &policy())
        .await
        .map_err(|error| error.to_string())?;
    let body = body_of(response)?;
    let text = String::from_utf8_lossy(&body);
    let endpoints = Endpoints::parse(&text)
        .ok_or_else(|| "the discovery document is not an OIDC configuration".to_owned())?;
    if !endpoints.matches_issuer(issuer) {
        return Err(format!(
            "the discovery document at `{issuer}` names issuer `{}`",
            endpoints.issuer
        ));
    }
    Ok(endpoints)
}

/// The authorization-code exchange (AUTH-04).
///
/// The client secret is held here rather than read per request, because it
/// comes from the secret store and a request path that reads secrets is a
/// request path that can be made to read them often.
pub struct HttpExchange {
    client: Arc<dyn HttpClient>,
    client_id: String,
    client_secret: Option<String>,
    redirect_uri: String,
}

/// Never prints `client_secret`. A `Debug` that renders a secret puts it in
/// every log line that formats the struct, which is how a credential leaves a
/// process without anyone deciding it should.
impl std::fmt::Debug for HttpExchange {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("HttpExchange")
            .field("client_id", &self.client_id)
            .field("redirect_uri", &self.redirect_uri)
            .field(
                "client_secret",
                &self.client_secret.as_ref().map(|_| "<redacted>"),
            )
            .finish_non_exhaustive()
    }
}

impl HttpExchange {
    pub fn new(
        client: Arc<dyn HttpClient>,
        client_id: &str,
        client_secret: Option<String>,
        redirect_uri: &str,
    ) -> Self {
        Self {
            client,
            client_id: client_id.to_owned(),
            client_secret,
            redirect_uri: redirect_uri.to_owned(),
        }
    }
}

impl Exchange for HttpExchange {
    fn exchange<'a>(
        &'a self,
        endpoint: &'a str,
        code: &'a str,
        verifier: &'a str,
    ) -> BoxFut<'a, Result<TokenResponse, String>> {
        Box::pin(async move {
            let url = Url::parse(endpoint)
                .map_err(|error| format!("`{endpoint}` is not a URL: {error}"))?;
            let mut form = vec![
                ("grant_type", "authorization_code"),
                ("code", code),
                ("redirect_uri", self.redirect_uri.as_str()),
                ("client_id", self.client_id.as_str()),
                ("code_verifier", verifier),
            ];
            if let Some(secret) = &self.client_secret {
                form.push(("client_secret", secret.as_str()));
            }
            let request = HttpRequest {
                method: Method::POST,
                url,
                headers: vec![
                    (
                        "content-type".to_owned(),
                        "application/x-www-form-urlencoded".to_owned(),
                    ),
                    ("accept".to_owned(), "application/json".to_owned()),
                ],
                body: Some(form_encode(&form).into_bytes().into()),
            };
            let response = self
                .client
                .fetch(request, &policy())
                .await
                .map_err(|error| error.to_string())?;
            let body = body_of(response)?;
            serde_json::from_slice::<TokenResponse>(&body)
                .map_err(|error| format!("the token response did not parse: {error}"))
        })
    }
}

/// `application/x-www-form-urlencoded`. Only the unreserved set of RFC 3986
/// passes through, and a space is `%20` rather than `+`: both are legal, and
/// one of them is legal everywhere.
fn form_encode(pairs: &[(&str, &str)]) -> String {
    pairs
        .iter()
        .map(|(key, value)| format!("{}={}", escape(key), escape(value)))
        .collect::<Vec<_>>()
        .join("&")
}

fn escape(value: &str) -> String {
    let mut out = String::with_capacity(value.len());
    for byte in value.bytes() {
        match byte {
            b'A'..=b'Z' | b'a'..=b'z' | b'0'..=b'9' | b'-' | b'.' | b'_' | b'~' => {
                out.push(byte as char)
            }
            _ => out.push_str(&format!("%{byte:02X}")),
        }
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::auth::clock::Clock;
    use crate::auth::jwks::{Jwks, Resolution};
    use liyasa_core::net::NetError;
    use std::sync::Mutex;
    use std::sync::atomic::{AtomicU64, Ordering};

    /// An identity provider that records what it was asked and answers what it
    /// was told to. Counting is the point: rule 17's questions here are all
    /// about the second call.
    #[derive(Debug)]
    struct Provider {
        calls: AtomicU64,
        seen: Mutex<Vec<HttpRequest>>,
        answer: Mutex<Result<(u16, String), NetError>>,
    }

    impl Provider {
        fn answering(status: u16, body: &str) -> Self {
            Self {
                calls: AtomicU64::new(0),
                seen: Mutex::new(Vec::new()),
                answer: Mutex::new(Ok((status, body.to_owned()))),
            }
        }

        fn failing(error: NetError) -> Self {
            Self {
                calls: AtomicU64::new(0),
                seen: Mutex::new(Vec::new()),
                answer: Mutex::new(Err(error)),
            }
        }

        fn calls(&self) -> u64 {
            self.calls.load(Ordering::SeqCst)
        }

        fn last(&self) -> HttpRequest {
            self.seen
                .lock()
                .unwrap_or_else(|e| e.into_inner())
                .last()
                .cloned()
                .expect("a request was made")
        }
    }

    impl HttpClient for Provider {
        fn fetch<'a>(
            &'a self,
            req: HttpRequest,
            policy: &'a HttpPolicy,
        ) -> BoxFut<'a, Result<HttpResponse, NetError>> {
            self.calls.fetch_add(1, Ordering::SeqCst);
            let url = req.url.clone();
            self.seen
                .lock()
                .unwrap_or_else(|e| e.into_inner())
                .push(req);
            // Every request this module makes must carry the hardened policy;
            // asserting it here means no call site can quietly relax it.
            assert!(!policy.allow_private, "an identity provider is not private");
            assert_eq!(
                policy.max_redirects, 0,
                "a redirected JWKS is not the one configured"
            );
            assert_eq!(policy.purpose, Purpose::FactSource);
            let answer = self
                .answer
                .lock()
                .unwrap_or_else(|e| e.into_inner())
                .clone();
            Box::pin(async move {
                let (status, body) = answer?;
                Ok(HttpResponse {
                    status,
                    headers: Vec::new(),
                    body: body.into_bytes().into(),
                    final_url: url,
                })
            })
        }
    }

    const KEYS: &str = r#"{"keys":[{"kid":"k1","kty":"RSA","alg":"RS256","n":"AQAB","e":"AQAB"}]}"#;

    fn jwks_source(provider: Arc<Provider>) -> HttpJwks {
        HttpJwks::new(provider, "https://idp.example/.well-known/jwks.json").expect("a url")
    }

    #[tokio::test]
    async fn a_jwks_is_fetched_and_parsed() {
        let provider = Arc::new(Provider::answering(200, KEYS));
        let set = jwks_source(provider.clone()).fetch().await.expect("a JWKS");
        assert_eq!(set.keys.len(), 1);
        assert_eq!(set.keys[0].kid, "k1");
        assert_eq!(provider.last().method, Method::GET);
        assert_eq!(
            provider.last().url.as_str(),
            "https://idp.example/.well-known/jwks.json"
        );
    }

    /// Rule 17. The cache is the thing being claimed, so the test that matters
    /// is the second call — a JWKS that refetches every time is not a cache,
    /// and AUTH-03's whole anti-amplification property is about this number.
    #[tokio::test]
    async fn a_second_resolve_inside_the_window_does_not_reach_the_provider() {
        let provider = Arc::new(Provider::answering(200, KEYS));
        let source = jwks_source(provider.clone());
        let jwks = Jwks::new(Clock::manual());

        assert!(matches!(
            jwks.resolve("k1", &source).await,
            Resolution::Found(_)
        ));
        assert_eq!(provider.calls(), 1);

        for _ in 0..50 {
            assert!(matches!(
                jwks.resolve("k1", &source).await,
                Resolution::Found(_)
            ));
        }
        assert_eq!(provider.calls(), 1, "a cached key must not be refetched");

        // And a flood of unknown kids is still one fetch, which is the
        // property that stops this being an amplifier against the provider.
        for n in 0..500 {
            assert_eq!(
                jwks.resolve(&format!("random-{n}"), &source).await,
                Resolution::Unknown
            );
        }
        assert_eq!(provider.calls(), 1);
    }

    #[tokio::test]
    async fn a_provider_that_is_down_is_reported_rather_than_read_as_empty() {
        let provider = Arc::new(Provider::failing(NetError::Timeout));
        let error = jwks_source(provider)
            .fetch()
            .await
            .expect_err("a timeout is not a JWKS");
        assert!(error.contains("timed out"), "{error}");
    }

    #[tokio::test]
    async fn a_non_success_status_is_reported_with_its_status() {
        let provider = Arc::new(Provider::answering(503, "upstream is down"));
        let error = jwks_source(provider).fetch().await.expect_err("503");
        assert!(error.contains("503"), "{error}");
    }

    /// A body that is not a JWKS must not resolve as a provider with no keys:
    /// that would negatively cache every `kid` as absent rather than say the
    /// document is broken.
    #[tokio::test]
    async fn a_body_that_is_not_a_jwks_is_an_error_rather_than_an_empty_set() {
        let mut accepted = Vec::new();
        for body in ["<html>sign in</html>", "null", "[]", "\"a string\"", "42"] {
            let provider = Arc::new(Provider::answering(200, body));
            match jwks_source(provider).fetch().await {
                Err(error) => assert!(error.contains("did not parse"), "{body}: {error}"),
                Ok(set) => accepted.push((body, set.keys.len())),
            }
        }
        assert!(
            accepted.is_empty(),
            "these are not JWKS documents and parsed anyway: {accepted:?}"
        );
    }

    #[test]
    fn a_jwks_url_that_is_not_a_url_is_refused_at_construction() {
        let provider = Arc::new(Provider::answering(200, KEYS));
        assert!(HttpJwks::new(provider.clone(), "not a url").is_none());
        assert!(HttpJwks::new(provider, "/relative/jwks.json").is_none());
    }

    // ---- discovery ----

    const DISCOVERY: &str = r#"{"issuer":"https://idp.example",
        "authorization_endpoint":"https://idp.example/authorize",
        "token_endpoint":"https://idp.example/oauth/token",
        "jwks_uri":"https://idp.example/jwks"}"#;

    #[tokio::test]
    async fn discovery_reads_the_well_known_path_under_the_issuer() {
        let provider = Arc::new(Provider::answering(200, DISCOVERY));
        let endpoints = discover(provider.as_ref(), "https://idp.example")
            .await
            .expect("a document");
        assert_eq!(endpoints.token_endpoint, "https://idp.example/oauth/token");
        assert_eq!(
            provider.last().url.as_str(),
            "https://idp.example/.well-known/openid-configuration"
        );
    }

    /// A document naming another issuer is how a provider mix-up becomes a
    /// sign-in as somebody else.
    #[tokio::test]
    async fn a_discovery_document_naming_another_issuer_is_refused() {
        let provider = Arc::new(Provider::answering(200, DISCOVERY));
        let error = discover(provider.as_ref(), "https://other.example")
            .await
            .expect_err("the issuer does not match");
        assert!(error.contains("names issuer"), "{error}");
    }

    #[tokio::test]
    async fn a_discovery_document_that_is_not_one_is_refused() {
        let provider = Arc::new(Provider::answering(200, "{}"));
        let error = discover(provider.as_ref(), "https://idp.example")
            .await
            .expect_err("no endpoints");
        assert!(error.contains("not an OIDC configuration"), "{error}");
    }

    // ---- token exchange ----

    const TOKENS: &str = r#"{"access_token":"at","token_type":"Bearer"}"#;

    fn exchange(provider: Arc<Provider>) -> HttpExchange {
        HttpExchange::new(
            provider,
            "liyasa-docs",
            Some("s3cret".to_owned()),
            "https://docs.example.com/_liyasa/auth/callback",
        )
    }

    #[tokio::test]
    async fn an_exchange_posts_the_code_and_the_verifier() {
        let provider = Arc::new(Provider::answering(200, TOKENS));
        let response = exchange(provider.clone())
            .exchange(
                "https://idp.example/oauth/token",
                "the-code",
                "the-verifier",
            )
            .await
            .expect("a token");
        assert_eq!(response.access_token, "at");

        let request = provider.last();
        assert_eq!(request.method, Method::POST);
        let body = String::from_utf8_lossy(&request.body.expect("a body")).into_owned();
        for expected in [
            "grant_type=authorization_code",
            "code=the-code",
            "code_verifier=the-verifier",
            "client_id=liyasa-docs",
            "client_secret=s3cret",
        ] {
            assert!(body.contains(expected), "{expected} missing from {body}");
        }
        // PKCE is the point: the verifier must be on the wire, and the
        // challenge must not.
        assert!(!body.contains("code_challenge"), "{body}");
    }

    #[tokio::test]
    async fn a_public_client_sends_no_secret() {
        let provider = Arc::new(Provider::answering(200, TOKENS));
        HttpExchange::new(provider.clone(), "liyasa-docs", None, "https://docs/cb")
            .exchange("https://idp.example/oauth/token", "c", "v")
            .await
            .expect("a token");
        let body = String::from_utf8_lossy(&provider.last().body.expect("a body")).into_owned();
        assert!(!body.contains("client_secret"), "{body}");
        assert!(body.contains("code_verifier=v"), "{body}");
    }

    /// Rule 17, and the one where the second call is a security property
    /// rather than a performance one. The provider is what enforces
    /// single-use on an authorization code, so this asserts the refusal is
    /// carried rather than swallowed — a replay that returned `Ok` would sign
    /// somebody in twice from one code.
    #[tokio::test]
    async fn a_replayed_code_is_refused_rather_than_reported_as_a_sign_in() {
        let provider = Arc::new(Provider::answering(200, TOKENS));
        let exchange = exchange(provider.clone());
        assert!(
            exchange
                .exchange("https://idp.example/oauth/token", "the-code", "v")
                .await
                .is_ok()
        );

        // The provider now answers as a real one does for a spent code.
        *provider.answer.lock().unwrap_or_else(|e| e.into_inner()) =
            Ok((400, r#"{"error":"invalid_grant"}"#.to_owned()));
        let error = exchange
            .exchange("https://idp.example/oauth/token", "the-code", "v")
            .await
            .expect_err("a spent code is not a sign-in");
        assert!(error.contains("400"), "{error}");
        assert_eq!(
            provider.calls(),
            2,
            "the second attempt did reach the provider"
        );
    }

    #[tokio::test]
    async fn a_token_response_that_is_not_one_is_refused() {
        let provider = Arc::new(Provider::answering(200, "<html>error</html>"));
        let error = exchange(provider)
            .exchange("https://idp.example/oauth/token", "c", "v")
            .await
            .expect_err("html is not a token response");
        assert!(error.contains("did not parse"), "{error}");
    }

    #[test]
    fn form_encoding_escapes_everything_outside_the_unreserved_set() {
        assert_eq!(
            form_encode(&[("redirect_uri", "https://a/b?c=d&e")]),
            "redirect_uri=https%3A%2F%2Fa%2Fb%3Fc%3Dd%26e"
        );
        assert_eq!(form_encode(&[("a", "b c")]), "a=b%20c");
        assert_eq!(form_encode(&[("safe", "-._~")]), "safe=-._~");
        // A value that could otherwise inject a second parameter.
        let injected = form_encode(&[("code", "x&client_secret=stolen")]);
        assert!(!injected.contains("&client_secret=stolen"), "{injected}");
    }
}
