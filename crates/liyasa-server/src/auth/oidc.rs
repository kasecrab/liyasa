//! OAuth 2.0 and OpenID Connect (AUTH-04).
//!
//! PKCE always — not "for public clients", not "when the provider supports
//! it". A documentation site's client secret ends up in a configuration file
//! on someone's laptop sooner or later, and PKCE is what makes an intercepted
//! authorization code useless without the verifier that never left this
//! process.
//!
//! Auth0, Okta, Entra ID, Google and Keycloak are all OIDC and are configured
//! by issuer alone, through discovery. GitHub is OAuth 2.0 with no discovery
//! document and no ID token, so its endpoints are named here and its groups
//! come from the organization and team membership the operator maps.

use std::collections::BTreeMap;
use std::sync::RwLock;
use std::time::Duration;

use liyasa_core::net::BoxFut;
use ring::digest;
use serde::{Deserialize, Serialize};

use crate::auth::base64url;
use crate::auth::clock::{Clock, millis};
use crate::auth::config::OidcConfig;
use crate::auth::random::{self, NoEntropy, constant_time_eq};
use crate::auth::session::Principal;

/// How long an authorization request may sit before its `state` is forgotten.
/// Long enough for a password manager and a second factor, short enough that
/// an abandoned tab is not a live credential.
pub const FLOW_TTL: Duration = Duration::from_secs(600);

/// The endpoints a flow needs, however they were obtained.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub struct Endpoints {
    pub issuer: String,
    pub authorization_endpoint: String,
    pub token_endpoint: String,
    #[serde(default)]
    pub jwks_uri: Option<String>,
    #[serde(default)]
    pub userinfo_endpoint: Option<String>,
}

impl Endpoints {
    /// `<issuer>/.well-known/openid-configuration`, which is where every OIDC
    /// provider in AUTH-04 but GitHub publishes this.
    pub fn discovery_url(issuer: &str) -> String {
        format!(
            "{}/.well-known/openid-configuration",
            issuer.trim_end_matches('/')
        )
    }

    pub fn parse(text: &str) -> Option<Self> {
        serde_json::from_str(text).ok()
    }

    /// GitHub is OAuth 2.0 and publishes no discovery document.
    pub fn github() -> Self {
        Self {
            issuer: "https://github.com".to_owned(),
            authorization_endpoint: "https://github.com/login/oauth/authorize".to_owned(),
            token_endpoint: "https://github.com/login/oauth/access_token".to_owned(),
            jwks_uri: None,
            userinfo_endpoint: Some("https://api.github.com/user".to_owned()),
        }
    }

    /// Whether the document actually describes the issuer it was fetched for.
    /// A discovery document that names a different issuer is how a provider
    /// mix-up becomes a sign-in as somebody else.
    pub fn matches_issuer(&self, issuer: &str) -> bool {
        self.issuer.trim_end_matches('/') == issuer.trim_end_matches('/')
    }
}

/// One authorization request in flight.
#[derive(Debug, Clone)]
struct Pending {
    verifier: String,
    nonce: String,
    /// Where to send the reader once they are signed in. Always a path on this
    /// site: an open redirect here is a phishing primitive.
    return_to: String,
    created_ms: i64,
}

/// What the login endpoint hands back.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Started {
    pub state: String,
    pub authorize_url: String,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Refused {
    /// No such `state`, or it has already been used. Both are the same to
    /// whoever sent it.
    UnknownState,
    StateExpired,
    /// The provider said no.
    Provider(String),
    Exchange(String),
    /// The ID token's `nonce` is not the one this flow sent.
    WrongNonce,
    NoSubject,
}

/// The token endpoint. Behind a trait because a test cannot reach Auth0 and
/// an offline instance must not try (HOST-08).
pub trait Exchange: std::fmt::Debug + Send + Sync {
    fn exchange<'a>(
        &'a self,
        endpoint: &'a str,
        code: &'a str,
        verifier: &'a str,
    ) -> BoxFut<'a, Result<TokenResponse, String>>;
}

#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub struct TokenResponse {
    #[serde(default)]
    pub access_token: String,
    #[serde(default)]
    pub id_token: Option<String>,
    #[serde(default)]
    pub token_type: String,
    /// Claims the caller has already verified out of the ID token, or fetched
    /// from `userinfo`. The flow does not verify signatures itself; that is
    /// [`super::jwt`]'s job against the provider's JWKS.
    #[serde(default)]
    pub claims: serde_json::Value,
}

#[derive(Debug)]
pub struct Flow {
    pending: RwLock<BTreeMap<String, Pending>>,
    config: OidcConfig,
    endpoints: RwLock<Option<Endpoints>>,
    redirect_uri: String,
    clock: Clock,
}

impl Flow {
    pub fn new(config: OidcConfig, redirect_uri: &str, clock: Clock) -> Self {
        Self {
            pending: RwLock::new(BTreeMap::new()),
            config,
            endpoints: RwLock::new(None),
            redirect_uri: redirect_uri.to_owned(),
            clock,
        }
    }

    pub fn with_endpoints(self, endpoints: Endpoints) -> Self {
        *self.endpoints.write().unwrap_or_else(|e| e.into_inner()) = Some(endpoints);
        self
    }

    pub fn clock(&self) -> &Clock {
        &self.clock
    }

    pub fn endpoints(&self) -> Option<Endpoints> {
        self.endpoints
            .read()
            .unwrap_or_else(|e| e.into_inner())
            .clone()
    }

    pub fn in_flight(&self) -> usize {
        self.pending.read().unwrap_or_else(|e| e.into_inner()).len()
    }

    /// `GET /_liyasa/auth/login`.
    pub fn begin(&self, return_to: &str) -> Result<Started, NoEntropy> {
        let endpoints = self.endpoints().ok_or(NoEntropy)?;
        let state = random::token()?;
        let verifier = random::token()?;
        let nonce = random::token()?;
        let challenge = pkce_challenge(&verifier);

        let mut url = format!(
            "{}?response_type=code&client_id={}&redirect_uri={}&state={}&nonce={}\
             &code_challenge={challenge}&code_challenge_method=S256",
            endpoints.authorization_endpoint,
            escape(self.config.client_id.as_deref().unwrap_or_default()),
            escape(&self.redirect_uri),
            escape(&state),
            escape(&nonce),
        );
        if !self.config.scopes.is_empty() {
            url.push_str(&format!("&scope={}", escape(&self.config.scopes.join(" "))));
        }

        self.pending
            .write()
            .unwrap_or_else(|e| e.into_inner())
            .insert(
                state.clone(),
                Pending {
                    verifier,
                    nonce,
                    return_to: safe_return_to(return_to),
                    created_ms: self.clock.now_ms(),
                },
            );
        Ok(Started {
            state,
            authorize_url: url,
        })
    }

    /// `GET /_liyasa/auth/callback`. Validates `state`, then exchanges the
    /// code with the verifier this flow kept.
    pub async fn finish(
        &self,
        state: &str,
        code: &str,
        exchange: &dyn Exchange,
    ) -> Result<(Principal, String), Refused> {
        let endpoints = self
            .endpoints()
            .ok_or_else(|| Refused::Exchange("no provider endpoints".to_owned()))?;
        // Taken, not read: a `state` is single use, so a replayed callback
        // finds nothing.
        let pending = self
            .pending
            .write()
            .unwrap_or_else(|e| e.into_inner())
            .remove(state);
        let Some(pending) = pending else {
            return Err(Refused::UnknownState);
        };
        if self.clock.now_ms().saturating_sub(pending.created_ms) >= millis(FLOW_TTL) {
            return Err(Refused::StateExpired);
        }

        let response = exchange
            .exchange(&endpoints.token_endpoint, code, &pending.verifier)
            .await
            .map_err(Refused::Exchange)?;

        if let Some(nonce) = response.claims.get("nonce").and_then(|v| v.as_str())
            && !constant_time_eq(nonce.as_bytes(), pending.nonce.as_bytes())
        {
            return Err(Refused::WrongNonce);
        }

        let principal = self.principal(&response.claims)?;
        Ok((principal, pending.return_to))
    }

    fn principal(&self, claims: &serde_json::Value) -> Result<Principal, Refused> {
        let subject = claims
            .get("sub")
            .and_then(|v| v.as_str())
            .filter(|s| !s.is_empty())
            .ok_or(Refused::NoSubject)?;
        let groups = match claims.get(&self.config.groups_claim) {
            Some(serde_json::Value::Array(items)) => items
                .iter()
                .filter_map(|v| v.as_str())
                .map(str::to_owned)
                .collect(),
            Some(serde_json::Value::String(text)) => text
                .split([',', ' '])
                .map(str::trim)
                .filter(|g| !g.is_empty())
                .map(str::to_owned)
                .collect(),
            _ => Default::default(),
        };
        Ok(Principal {
            subject: subject.to_owned(),
            groups,
            region: None,
            locale: claims
                .get("locale")
                .and_then(|v| v.as_str())
                .map(str::to_owned),
            data: BTreeMap::new(),
            role: crate::auth::roles::Role::Reader,
            via: "oidc".to_owned(),
        })
    }

    /// Drops flows nobody came back from.
    pub fn sweep(&self) -> usize {
        let now = self.clock.now_ms();
        let ttl = millis(FLOW_TTL);
        let mut pending = self.pending.write().unwrap_or_else(|e| e.into_inner());
        let before = pending.len();
        pending.retain(|_, flow| now.saturating_sub(flow.created_ms) < ttl);
        before - pending.len()
    }
}

/// RFC 7636 S256: the challenge is the base64url SHA-256 of the verifier.
pub fn pkce_challenge(verifier: &str) -> String {
    base64url::encode(digest::digest(&digest::SHA256, verifier.as_bytes()).as_ref())
}

/// A return path on this site. Anything absolute, protocol-relative or
/// otherwise off-site becomes `/`: an open redirect on a login endpoint is
/// what makes a phishing link look legitimate.
pub fn safe_return_to(path: &str) -> String {
    let trimmed = path.trim();
    let safe = trimmed.starts_with('/')
        && !trimmed.starts_with("//")
        && !trimmed.starts_with("/\\")
        && !trimmed.contains('\n')
        && !trimmed.contains('\r');
    match safe {
        true => trimmed.to_owned(),
        false => "/".to_owned(),
    }
}

/// Percent-encoding for a query parameter value. Only the unreserved set of
/// RFC 3986 passes through unescaped.
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

    #[derive(Debug)]
    struct Recording {
        claims: serde_json::Value,
        seen: std::sync::Mutex<Vec<(String, String)>>,
    }

    impl Recording {
        fn with(claims: serde_json::Value) -> Self {
            Self {
                claims,
                seen: std::sync::Mutex::new(Vec::new()),
            }
        }

        fn seen(&self) -> Vec<(String, String)> {
            self.seen.lock().unwrap_or_else(|e| e.into_inner()).clone()
        }
    }

    impl Exchange for Recording {
        fn exchange<'a>(
            &'a self,
            _endpoint: &'a str,
            code: &'a str,
            verifier: &'a str,
        ) -> BoxFut<'a, Result<TokenResponse, String>> {
            self.seen
                .lock()
                .unwrap_or_else(|e| e.into_inner())
                .push((code.to_owned(), verifier.to_owned()));
            let claims = self.claims.clone();
            Box::pin(async move {
                Ok(TokenResponse {
                    access_token: "at".to_owned(),
                    claims,
                    ..TokenResponse::default()
                })
            })
        }
    }

    fn endpoints() -> Endpoints {
        Endpoints {
            issuer: "https://idp.example".to_owned(),
            authorization_endpoint: "https://idp.example/authorize".to_owned(),
            token_endpoint: "https://idp.example/oauth/token".to_owned(),
            jwks_uri: Some("https://idp.example/.well-known/jwks.json".to_owned()),
            userinfo_endpoint: None,
        }
    }

    fn flow() -> Flow {
        Flow::new(
            OidcConfig {
                issuer: Some("https://idp.example".to_owned()),
                client_id: Some("liyasa-docs".to_owned()),
                ..OidcConfig::default()
            },
            "https://docs.example.com/_liyasa/auth/callback",
            Clock::manual(),
        )
        .with_endpoints(endpoints())
    }

    fn claims() -> serde_json::Value {
        serde_json::json!({ "sub": "reader-1", "groups": ["partner"], "locale": "en" })
    }

    #[test]
    fn an_authorization_request_always_carries_a_pkce_s256_challenge() {
        let started = flow().begin("/guides/install").expect("a flow");
        assert!(
            started.authorize_url.contains("code_challenge_method=S256"),
            "{}",
            started.authorize_url
        );
        assert!(started.authorize_url.contains("code_challenge="));
        assert!(started.authorize_url.contains("state="));
        assert!(started.authorize_url.contains("client_id=liyasa-docs"));
        assert!(
            started
                .authorize_url
                .contains("scope=openid%20profile%20email"),
            "{}",
            started.authorize_url
        );
    }

    #[test]
    fn the_challenge_is_the_sha_256_of_the_verifier() {
        // RFC 7636 appendix B.
        assert_eq!(
            pkce_challenge("dBjftJeZ4CVP-mB92K27uhbUJU1p1r_wW1gFWFOEjXk"),
            "E9Melhoa2OwvFrEMTJguCHaoeK1t8URWbuGJSstw-cM"
        );
    }

    #[tokio::test]
    async fn the_verifier_never_leaves_this_process_until_the_exchange() {
        let flow = flow();
        let started = flow.begin("/guides/install").expect("a flow");
        assert!(
            !started.authorize_url.contains("code_verifier"),
            "the verifier must not be in the authorization URL"
        );

        let exchange = Recording::with(claims());
        let (principal, return_to) = flow
            .finish(&started.state, "the-code", &exchange)
            .await
            .expect("a sign-in");
        assert_eq!(principal.subject, "reader-1");
        assert!(principal.groups.contains("partner"));
        assert_eq!(return_to, "/guides/install");

        let seen = exchange.seen();
        assert_eq!(seen.len(), 1);
        assert_eq!(seen[0].0, "the-code");
        assert_eq!(seen[0].1.len(), 43, "the verifier went with the exchange");
        assert_eq!(pkce_challenge(&seen[0].1).len(), 43);
    }

    #[tokio::test]
    async fn a_state_is_single_use() {
        let flow = flow();
        let started = flow.begin("/").expect("a flow");
        let exchange = Recording::with(claims());
        assert!(flow.finish(&started.state, "c", &exchange).await.is_ok());
        assert_eq!(
            flow.finish(&started.state, "c", &exchange).await,
            Err(Refused::UnknownState),
            "a replayed callback must not sign anyone in"
        );
    }

    #[tokio::test]
    async fn a_state_nobody_issued_is_refused() {
        let flow = flow();
        let exchange = Recording::with(claims());
        assert_eq!(
            flow.finish("invented", "c", &exchange).await,
            Err(Refused::UnknownState)
        );
        assert!(exchange.seen().is_empty(), "nothing was exchanged");
    }

    #[tokio::test]
    async fn an_abandoned_flow_expires() {
        let flow = flow();
        let started = flow.begin("/").expect("a flow");
        flow.clock().advance(FLOW_TTL);
        assert_eq!(
            flow.finish(&started.state, "c", &Recording::with(claims()))
                .await,
            Err(Refused::StateExpired)
        );
    }

    #[tokio::test]
    async fn an_id_token_nonce_that_is_not_ours_is_refused() {
        let flow = flow();
        let started = flow.begin("/").expect("a flow");
        let mut claims = claims();
        claims["nonce"] = serde_json::json!("somebody-elses-nonce");
        assert_eq!(
            flow.finish(&started.state, "c", &Recording::with(claims))
                .await,
            Err(Refused::WrongNonce)
        );
    }

    #[tokio::test]
    async fn a_provider_that_returns_no_subject_signs_nobody_in() {
        let flow = flow();
        let started = flow.begin("/").expect("a flow");
        assert_eq!(
            flow.finish(
                &started.state,
                "c",
                &Recording::with(serde_json::json!({ "groups": ["partner"] }))
            )
            .await,
            Err(Refused::NoSubject)
        );
    }

    #[test]
    fn a_return_path_off_this_site_becomes_the_root() {
        assert_eq!(safe_return_to("/guides/install"), "/guides/install");
        assert_eq!(safe_return_to("/"), "/");
        for hostile in [
            "https://evil.example",
            "//evil.example",
            "/\\evil.example",
            "guides/install",
            "",
            "/ok\nLocation: https://evil.example",
        ] {
            assert_eq!(safe_return_to(hostile), "/", "{hostile}");
        }
    }

    #[test]
    fn discovery_is_the_well_known_path_under_the_issuer() {
        assert_eq!(
            Endpoints::discovery_url("https://idp.example"),
            "https://idp.example/.well-known/openid-configuration"
        );
        assert_eq!(
            Endpoints::discovery_url("https://idp.example/"),
            "https://idp.example/.well-known/openid-configuration"
        );
    }

    #[test]
    fn a_discovery_document_naming_another_issuer_is_not_this_provider() {
        let document = endpoints();
        assert!(document.matches_issuer("https://idp.example"));
        assert!(document.matches_issuer("https://idp.example/"));
        assert!(!document.matches_issuer("https://evil.example"));
    }

    #[test]
    fn a_discovery_document_parses_and_ignores_what_we_do_not_read() {
        let document = Endpoints::parse(
            r#"{"issuer":"https://idp.example","authorization_endpoint":"https://idp.example/a",
                "token_endpoint":"https://idp.example/t","jwks_uri":"https://idp.example/j",
                "response_types_supported":["code"],"claims_supported":["sub"]}"#,
        )
        .expect("a document");
        assert_eq!(document.issuer, "https://idp.example");
        assert_eq!(document.jwks_uri.as_deref(), Some("https://idp.example/j"));
        assert!(
            Endpoints::parse("{}").is_none(),
            "the endpoints are required"
        );
    }

    #[test]
    fn github_is_configured_without_discovery() {
        let github = Endpoints::github();
        assert!(github.jwks_uri.is_none(), "GitHub issues no ID token");
        assert!(github.userinfo_endpoint.is_some());
        assert!(
            github
                .authorization_endpoint
                .starts_with("https://github.com/")
        );
    }

    #[test]
    fn a_sweep_drops_flows_nobody_came_back_from() {
        let flow = flow();
        flow.begin("/").expect("a flow");
        flow.begin("/").expect("a flow");
        assert_eq!(flow.in_flight(), 2);
        flow.clock().advance(FLOW_TTL);
        assert_eq!(flow.sweep(), 2);
        assert_eq!(flow.in_flight(), 0);
    }

    #[test]
    fn a_query_value_is_escaped_rather_than_pasted_in() {
        assert_eq!(escape("openid profile"), "openid%20profile");
        assert_eq!(escape("a&b=c"), "a%26b%3Dc");
        assert_eq!(escape("safe-._~"), "safe-._~");
    }
}
