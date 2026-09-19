//! Everything the auth endpoints share.
//!
//! `AppState` belongs to WP-14 and this cannot be a field on it (RFC 1501), so
//! the auth state is its own value and the endpoint table is a router that
//! carries it. Nothing here reaches the network on its own: the two seams that
//! would — the token exchange and sending a magic link — are traits the caller
//! supplies, which is also what makes the whole table testable in process.

use std::num::NonZeroUsize;
use std::sync::Arc;

use liyasa_core::diagnostics::Diagnostics;

use crate::auth::clock::Clock;
use crate::auth::config::AuthConfig;
use crate::auth::domains::Registry as DomainRegistry;
use crate::auth::jwks::Jwks;
use crate::auth::magic::Magic;
use crate::auth::oidc::{Exchange, Flow};
use crate::auth::password::Passwords;
use crate::auth::random::NoEntropy;
use crate::auth::session::{Policy, Sessions};
use crate::auth::tokens::Tokens;
use crate::auth::variant::VariantCache;
use crate::routes::client_ip::TrustedProxies;

/// How many renderings the server's variant LRU holds (§6.6.3 item 5).
pub const DEFAULT_VARIANT_CAPACITY: usize = 1_024;

/// Sending a magic link. A deployment supplies an SMTP or provider client;
/// a test supplies one that records.
pub trait Mail: std::fmt::Debug + Send + Sync {
    fn send_link(&self, address: &str, token: &str);
}

/// A deployment with no mail configured. The link is minted and nothing
/// carries it, which is a misconfiguration rather than a silent sign-in.
#[derive(Debug, Default, Clone, Copy)]
pub struct NoMail;

impl Mail for NoMail {
    fn send_link(&self, _address: &str, _token: &str) {
        tracing::warn!(
            target: "liyasa_server",
            "a magic link was requested and no mail sender is configured"
        );
    }
}

pub struct AuthState {
    pub config: AuthConfig,
    /// Which environment this instance serves; the password and the preview
    /// rules are per environment (AUTH-02, AUTH-40).
    pub env: String,
    /// The origins a state-changing request may come from: the docs host and
    /// every alias (HOST-23).
    pub origins: Vec<String>,
    pub sessions: Sessions,
    pub passwords: Passwords,
    pub magic: Magic,
    pub jwks: Jwks,
    pub tokens: Tokens,
    pub domains: DomainRegistry,
    pub variants: VariantCache,
    pub oidc: Option<Flow>,
    pub exchange: Option<Arc<dyn Exchange>>,
    /// The outbound client for JWKS fetches and the token exchange. `None` on
    /// an offline instance, which fetches nothing (HOST-08).
    pub http: Option<Arc<dyn liyasa_core::net::HttpClient>>,
    pub mail: Option<Arc<dyn Mail>>,
    pub proxies: TrustedProxies,
    /// Where a subject's role comes from (defect 65). `None` elevates nobody,
    /// which gates a dashboard shut rather than open.
    pub roles: Option<Arc<dyn crate::auth::layer::Roles>>,
    pub clock: Clock,
}

// `HttpClient` is not `Debug`, and this prints what identifies an instance
// rather than everything it holds — several fields here are credentials or
// live tables, and neither belongs in a log line.
impl std::fmt::Debug for AuthState {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("AuthState")
            .field("mode", &self.config.mode)
            .field("env", &self.env)
            .field("origins", &self.origins)
            .field("has_http", &self.http.is_some())
            .field("has_roles", &self.roles.is_some())
            .field("has_exchange", &self.exchange.is_some())
            .finish_non_exhaustive()
    }
}

impl AuthState {
    /// Builds the state from a checked configuration. The diagnostics the
    /// configuration produced come back with it rather than being swallowed:
    /// a server that starts with a bad `auth` section should say so once, at
    /// startup, and not once per request.
    pub fn new(
        config: AuthConfig,
        env: &str,
        origins: Vec<String>,
        clock: Clock,
    ) -> Result<(Self, Diagnostics), NoEntropy> {
        let diagnostics = config.check();
        let redirect_uri = format!(
            "{}/_liyasa/auth/callback",
            origins.first().map(String::as_str).unwrap_or("")
        );
        let oidc = match config.mode {
            crate::auth::config::Mode::Oidc => {
                Some(Flow::new(config.oidc.clone(), &redirect_uri, clock.clone()))
            }
            _ => None,
        };
        let state = Self {
            sessions: Sessions::new(Policy::from(&config.session), clock.clone()),
            passwords: Passwords::new(config.argon2(), clock.clone()),
            magic: Magic::new(
                config.managed.allow_domains.clone(),
                config.managed.ttl(),
                clock.clone(),
            )?,
            jwks: Jwks::new(clock.clone()),
            tokens: Tokens::new(clock.clone()),
            domains: DomainRegistry::new("", clock.clone()),
            variants: VariantCache::new(
                NonZeroUsize::new(DEFAULT_VARIANT_CAPACITY).unwrap_or(NonZeroUsize::MIN),
            ),
            oidc,
            exchange: None,
            http: None,
            mail: None,
            roles: None,
            proxies: TrustedProxies::default(),
            env: env.to_owned(),
            origins,
            config,
            clock,
        };
        Ok((state, diagnostics))
    }

    pub fn with_exchange(mut self, exchange: Arc<dyn Exchange>) -> Self {
        self.exchange = Some(exchange);
        self
    }

    pub fn with_http(mut self, http: Arc<dyn liyasa_core::net::HttpClient>) -> Self {
        self.http = Some(http);
        self
    }

    /// The JWKS source this instance fetches with, or [`jwks::NoSource`] when
    /// it has no client or no `auth.jwt.jwksUrl` (HOST-08).
    ///
    /// Built per call rather than stored because it borrows nothing: the
    /// caching, the refresh ceiling and the negative cache are all in
    /// [`Jwks`](crate::auth::jwks::Jwks), which *is* stored, and a source that
    /// cached as well would be a second expiry policy able to disagree with it.
    pub fn jwks_source(&self) -> Box<dyn crate::auth::jwks::Source> {
        let Some(http) = self.http.clone() else {
            return Box::new(crate::auth::jwks::NoSource);
        };
        let Some(url) = self.config.jwt.jwks_url.as_deref() else {
            return Box::new(crate::auth::jwks::NoSource);
        };
        match crate::auth::fetch::HttpJwks::new(http, url) {
            Some(source) => Box::new(source),
            None => Box::new(crate::auth::jwks::NoSource),
        }
    }

    /// Fetches the OIDC provider's endpoints if they are not known yet.
    ///
    /// Lazy rather than at mount, for two reasons: `contribute` is not async,
    /// and a provider that was down when this server started would otherwise
    /// leave sign-in broken until a restart. Called from the login handler, so
    /// the first reader after the provider recovers is the one who pays for it.
    pub async fn discover_if_needed(&self) -> Result<(), String> {
        let Some(flow) = self.oidc.as_ref() else {
            return Err("this instance is not configured for OIDC".to_owned());
        };
        if flow.endpoints().is_some() {
            return Ok(());
        }
        let Some(http) = self.http.as_ref() else {
            return Err("an offline instance cannot discover a provider (HOST-08)".to_owned());
        };
        let Some(issuer) = self.config.oidc.issuer.as_deref() else {
            return Err("`auth.oidc.issuer` is not set".to_owned());
        };
        let endpoints = crate::auth::fetch::discover(http.as_ref(), issuer).await?;
        flow.install_endpoints(endpoints);
        Ok(())
    }

    pub fn with_mail(mut self, mail: Arc<dyn Mail>) -> Self {
        self.mail = Some(mail);
        self
    }

    pub fn with_roles(mut self, roles: Arc<dyn crate::auth::layer::Roles>) -> Self {
        self.roles = Some(roles);
        self
    }

    pub fn with_proxies(mut self, proxies: TrustedProxies) -> Self {
        self.proxies = proxies;
        self
    }

    pub fn with_domains(mut self, domains: DomainRegistry) -> Self {
        self.domains = domains;
        self
    }

    /// The site default a request against this environment is served under,
    /// which is not the same thing as the production site's (AUTH-40).
    pub fn site_default(&self) -> crate::auth::groups::SiteDefault {
        crate::auth::preview::site_default(
            &self.env,
            &self.config,
            self.passwords.is_set(&self.env),
        )
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::auth::config::Mode;
    use crate::auth::groups::SiteDefault;

    fn state(mode: Mode) -> AuthState {
        let config = AuthConfig {
            mode,
            ..AuthConfig::default()
        };
        AuthState::new(
            config,
            "production",
            vec!["https://docs.example.com".to_owned()],
            Clock::manual(),
        )
        .expect("entropy")
        .0
    }

    #[test]
    fn a_bad_configuration_reports_once_at_startup() {
        let config = AuthConfig::from_site_config(&serde_json::json!({
            "auth": { "mode": "jwt" }
        }))
        .expect("a parse");
        let (_, diagnostics) = AuthState::new(
            config,
            "production",
            vec!["https://docs.example.com".to_owned()],
            Clock::manual(),
        )
        .expect("entropy");
        assert_eq!(diagnostics.len(), 1, "`auth.jwt.jwksUrl` is missing");
    }

    #[test]
    fn an_oidc_site_has_a_flow_and_the_others_do_not() {
        assert!(state(Mode::Oidc).oidc.is_some());
        for mode in [Mode::Public, Mode::Password, Mode::Jwt, Mode::Managed] {
            assert!(state(mode).oidc.is_none(), "{mode:?}");
        }
    }

    #[test]
    fn an_oidc_flow_has_no_endpoints_until_discovery_has_run() {
        let state = state(Mode::Oidc);
        assert!(
            state.oidc.as_ref().expect("a flow").endpoints().is_none(),
            "a flow must not invent endpoints for an issuer it has not read"
        );
    }

    #[test]
    fn a_public_site_is_public_and_a_private_one_is_not() {
        assert_eq!(state(Mode::Public).site_default(), SiteDefault::Public);
        assert_eq!(state(Mode::Oidc).site_default(), SiteDefault::Private);
    }

    #[test]
    fn a_preview_is_private_even_on_a_public_site() {
        let config = AuthConfig::default();
        let preview = AuthState::new(
            config,
            "pr-42",
            vec!["https://pr-42.docs.example.com".to_owned()],
            Clock::manual(),
        )
        .expect("entropy")
        .0;
        assert_eq!(preview.site_default(), SiteDefault::Private);
    }
}
