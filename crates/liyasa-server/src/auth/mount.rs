//! What WP-15 contributes to the application (RFC 1403, defect 65).
//!
//! RFC 1403 put this adapter in `routes/mount.rs` with a note saying it
//! "belongs in `auth/` … next to the router it builds" and was only there
//! because that directory is WP-15's and WP-14 may not write in it. This is
//! that move, and it is not only tidying: the endpoint table and the session
//! layer must come from **one** [`AuthState`], and two call sites building
//! their own is a defect with no status code.
//!
//! A second `AuthState` would have a second `Sessions` table. Signing in would
//! appear to work — `POST /_liyasa/auth/password` answers 200 and sets a
//! cookie — and every later request would resolve that cookie against the
//! other table, find nothing, and arrive anonymous. Nothing returns an error;
//! the dashboard is simply never signed in. [`contribute`] returns both halves
//! from one state so that cannot be written by accident.

use std::sync::Arc;

use liyasa_core::diagnostics::{Diagnostic, Diagnostics, code};

use crate::auth::config::AuthConfig;
use crate::auth::state::AuthState;
use crate::routes::AppState;
use crate::routes::mount::Mount;

/// Both halves of WP-15's contribution, from one state.
pub struct Contribution {
    /// The endpoint table, for the subtree the seam merges.
    pub routes: Mount,
    /// The state the session layer must use. `None` exactly when there is
    /// nothing to extract — a public site, or a configuration that did not
    /// load — in which case no layer should be mounted at all.
    pub state: Option<Arc<AuthState>>,
}

/// Builds WP-15's routes and the state its layer needs, once.
///
/// `application` should call this, merge `routes` like any other subtree, and
/// then wrap the **finished** router with
/// [`with_session`](crate::auth::layer::with_session) using `state` — outside
/// every `guarded`, because a guard can only succeed if the extraction ran
/// outside it.
pub fn contribute(app: &Arc<AppState>) -> Contribution {
    let config = match AuthConfig::from_site_config(&app.config.site_config) {
        Ok(config) => config,
        Err(diagnostic) => {
            let mut diagnostics = Diagnostics::new();
            diagnostics.push(diagnostic);
            // A config that does not parse is not a public site: saying so
            // would hide the mistake behind a working server.
            return Contribution {
                routes: Mount::skipped("the `auth` section could not be read")
                    .with_diagnostics(diagnostics),
                state: None,
            };
        }
    };
    if config.mode.is_public() {
        // AUTH-01: a public site has no auth code path at all, and that
        // includes the layer — there is nothing to extract and nothing that
        // would read it.
        return Contribution {
            routes: Mount::skipped("`auth.mode` is public, so there is nothing to sign in to"),
            state: None,
        };
    }

    let origins = origins(&app.config.site_config);
    match AuthState::new(config, &app.config.env, origins, Default::default()) {
        Ok((state, mut diagnostics)) => {
            let mut state = state.with_proxies(app.proxies.clone());
            // AUTH-05: the magic-link sender. A block that cannot work is a
            // diagnostic rather than a log line, because AUTH-09 makes the
            // endpoint answer identically whether or not a link was sent — so
            // an operator watching the site cannot tell that none arrive.
            match mail(app, state.origins.first().map(String::as_str)) {
                Ok(Some(mail)) => state = state.with_mail(mail),
                // No `mail` block. `NoMail` says so per attempt; nothing is
                // wrong with the configuration, it just does not send.
                Ok(None) => {}
                Err(diagnostic) => diagnostics.push(diagnostic),
            }
            // The role chain, assembled here because this is the only place
            // that can see both halves: `AppState::role_source` has no
            // `AuthConfig` and so cannot see `auth.operators`, and
            // `AuthConfig` has no organization. (WP-14 found that by trying to
            // build the chain on their side first.)
            //
            // **Operators first, and that is a decision.** It is a break-glass
            // credential and must work whatever the membership table says —
            // which is the same reason it survives a member being reduced or
            // removed. Reversed, it would stop working at the moment somebody
            // needs it, after a bad membership edit. `Chain` documents the
            // hazard that follows from the ordering; keep the two agreeing.
            let mut sources: Vec<Arc<dyn crate::auth::layer::Roles>> = Vec::new();
            if let Some(operators) = state.config.operator_roles() {
                sources.push(Arc::new(operators));
            }
            if let Some(members) = app.role_source() {
                sources.push(members);
            }
            // Empty means no source at all rather than one that answers
            // nothing: a `Chain` over zero sources would look configured and
            // elevate nobody, which is the pair of meanings this package keeps
            // having to keep apart.
            if !sources.is_empty() {
                state = state.with_roles(Arc::new(
                    sources.into_iter().collect::<crate::auth::layer::Chain>(),
                ));
            }
            // HOST-08: an offline instance makes no outbound request of any
            // kind, so it gets no client and `jwks_source` falls back to
            // `NoSource` rather than a client that would refuse per request.
            if !app.config.offline
                && let Some(http) = http_client()
            {
                state = state.with_http(http.clone());
                // AUTH-04: the token exchange. No client secret, because the
                // schema has no key for one — `auth.oidc` is `issuer`,
                // `clientId`, `scopes`, `groupsClaim` and nothing else. This
                // is a public client, which is exactly why PKCE is mandatory
                // rather than optional here.
                if let Some(client_id) = state.config.oidc.client_id.clone() {
                    let redirect_uri = format!(
                        "{}/_liyasa/auth/callback",
                        state.origins.first().map(String::as_str).unwrap_or("")
                    );
                    state = state.with_exchange(Arc::new(crate::auth::fetch::HttpExchange::new(
                        http,
                        &client_id,
                        None,
                        &redirect_uri,
                    )));
                }
            }
            let state = Arc::new(state);
            Contribution {
                routes: Mount::routes(crate::auth::routes::router(state.clone()))
                    .with_diagnostics(diagnostics),
                state: Some(state),
            }
        }
        // Not E0803: the configuration is fine and the system has no
        // randomness, which is a different failure from an invalid `auth`
        // section. Borrowing a code that nearly fits is how a code stops
        // meaning anything.
        Err(error) => Contribution {
            routes: Mount::skipped(format!("authentication could not be started: {error}")),
            state: None,
        },
    }
}

/// The `Subtree` table's `fn(&Arc<AppState>) -> Mount`.
///
/// It drops the state, so an `application` that mounts the subtree through
/// this and nothing else has no layer and every guard answers 401. Use
/// [`contribute`] instead; this exists so the table's signature is satisfiable
/// and so the two cannot drift.
pub fn mount(app: &Arc<AppState>) -> Mount {
    contribute(app).routes
}

/// The SMTP sender, when the site configures one.
///
/// `Ok(None)` is a site with no `mail` block: links are still minted and
/// [`NoMail`](crate::auth::state::NoMail) reports that nothing carried them.
/// `Err` is a block that exists and cannot work, which is E0816.
fn mail(
    app: &Arc<AppState>,
    origin: Option<&str>,
) -> Result<Option<Arc<dyn crate::auth::state::Mail>>, Diagnostic> {
    use crate::auth::mail::{MailConfig, SmtpMail};

    let Some(config) = MailConfig::from_site_config(&app.config.site_config)? else {
        return Ok(None);
    };
    // A link in an email cannot be relative: there is no page for the mail
    // client to resolve it against.
    let Some(origin) = origin else {
        return Err(Diagnostic::new(
            code::E0816,
            "`mail` is configured and `seo.canonicalOrigin` is not, so a sign-in link \
             would have nowhere to point"
                .to_owned(),
        ));
    };
    let secrets = app
        .store
        .as_deref()
        .map(|store| store.secrets_typed() as &dyn liyasa_core::verify::SecretSource);
    let password = config
        .password(secrets)
        .map_err(|error| Diagnostic::new(code::E0816, error.to_string()))?;
    SmtpMail::new(&config, password, origin)
        .map(|mail| Some(Arc::new(mail) as Arc<dyn crate::auth::state::Mail>))
        .map_err(|error| Diagnostic::new(code::E0816, error.to_string()))
}

/// One outbound client for this instance's identity-provider traffic.
///
/// `None` when the client cannot be built, which leaves `jwks_source` on
/// `NoSource` and the exchange unset — sign-in then fails with a reason rather
/// than the server failing to start.
fn http_client() -> Option<Arc<dyn liyasa_core::net::HttpClient>> {
    match liyasa_net::client::Client::new(liyasa_net::client::ClientOptions::default()) {
        Ok(client) => Some(Arc::new(client)),
        Err(error) => {
            tracing::warn!(
                target: "liyasa_server",
                %error,
                "no outbound client; JWKS fetches and the OIDC token exchange are unavailable"
            );
            None
        }
    }
}

/// The origins a state-changing request may come from: the canonical origin
/// and every alias (HOST-23, AUTH-09's CSRF check).
fn origins(config: &serde_json::Value) -> Vec<String> {
    let mut out = Vec::new();
    if let Some(canonical) = config
        .get("seo")
        .and_then(|seo| seo.get("canonicalOrigin"))
        .and_then(serde_json::Value::as_str)
    {
        out.push(canonical.trim_end_matches('/').to_owned());
    }
    if let Some(aliases) = config
        .get("domains")
        .and_then(|domains| domains.get("aliases"))
        .and_then(serde_json::Value::as_array)
    {
        out.extend(
            aliases
                .iter()
                .filter_map(serde_json::Value::as_str)
                .map(|alias| alias.trim_end_matches('/').to_owned()),
        );
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::routes::ServerConfig;

    fn app(site_config: serde_json::Value) -> Arc<AppState> {
        let config = ServerConfig {
            site_config: Arc::new(site_config),
            ..ServerConfig::default()
        };
        Arc::new(AppState::new(config))
    }

    #[test]
    fn a_public_site_contributes_no_routes_and_no_state_to_extract() {
        let contribution = contribute(&app(serde_json::json!({})));
        assert!(contribution.routes.router.is_none());
        assert!(
            contribution.state.is_none(),
            "AUTH-01: a public site has no auth code path, layer included"
        );
    }

    #[test]
    fn a_site_with_auth_contributes_routes_and_the_state_the_layer_needs() {
        let contribution = contribute(&app(serde_json::json!({
            "auth": { "mode": "password" },
            "seo": { "canonicalOrigin": "https://docs.acme.com" }
        })));
        assert!(contribution.routes.router.is_some());
        let state = contribution.state.expect("a state for the layer");
        assert_eq!(state.origins, ["https://docs.acme.com"]);
    }

    /// The defect this module exists to prevent: the routes and the layer must
    /// be able to see each other's sessions.
    #[test]
    fn the_routes_and_the_layer_share_one_session_table() {
        let contribution = contribute(&app(serde_json::json!({
            "auth": { "mode": "password" }
        })));
        let state = contribution.state.expect("a state");
        assert!(contribution.routes.router.is_some());

        let session = state
            .sessions
            .begin(crate::auth::session::Principal::new("reader-1"))
            .expect("a session");
        assert!(
            state.sessions.resolve(&session.id).is_ok(),
            "a cookie minted by the endpoint table must resolve for the layer"
        );
        assert_eq!(state.sessions.len(), 1);
    }

    #[test]
    fn an_auth_section_that_does_not_parse_is_reported_rather_than_treated_as_public() {
        let contribution = contribute(&app(serde_json::json!({
            "auth": { "mode": "password", "passwrod": {} }
        })));
        assert!(contribution.routes.router.is_none());
        assert!(contribution.state.is_none());
        assert_eq!(contribution.routes.diagnostics.len(), 1);
        let reason = contribution.routes.skipped.unwrap_or_default();
        assert!(reason.contains("could not be read"), "{reason}");
    }

    #[test]
    fn the_subtree_adapter_builds_the_same_routes_it_throws_the_state_away_from() {
        let site = serde_json::json!({ "auth": { "mode": "password" } });
        assert_eq!(
            mount(&app(site.clone())).router.is_some(),
            contribute(&app(site)).routes.router.is_some()
        );
    }

    /// The point of this packet: the fetchers must be *reached*, not merely
    /// exist. Before this, `jwks_source` was always `NoSource` and `exchange`
    /// was always `None`, so JWT mode could not fetch a key and an OIDC
    /// callback could not exchange a code.
    #[test]
    fn an_instance_with_auth_configured_can_actually_fetch() {
        let contribution = contribute(&app(serde_json::json!({
            "auth": { "mode": "jwt", "jwt": { "jwksUrl": "https://idp.example/jwks" } }
        })));
        let state = contribution.state.expect("a state");
        assert!(state.http.is_some(), "an online instance gets a client");
        assert!(
            !format!("{:?}", state.jwks_source()).contains("NoSource"),
            "the JWKS source must be a real one when `jwksUrl` is set"
        );
    }

    #[test]
    fn an_oidc_instance_gets_an_exchange_and_a_redirect_uri_on_its_own_origin() {
        let contribution = contribute(&app(serde_json::json!({
            "auth": { "mode": "oidc", "oidc": {
                "issuer": "https://idp.example", "clientId": "liyasa-docs"
            }},
            "seo": { "canonicalOrigin": "https://docs.acme.com" }
        })));
        let state = contribution.state.expect("a state");
        assert!(state.exchange.is_some(), "AUTH-04 needs a token exchange");
        // The redirect URI is on this site, and the client secret is absent
        // because the schema has nowhere to put one — hence PKCE is not
        // optional. `Debug` redacts it either way.
        let shown = format!("{:?}", state.exchange.as_ref().expect("an exchange"));
        assert!(
            shown.contains("https://docs.acme.com/_liyasa/auth/callback"),
            "{shown}"
        );
        assert!(shown.contains("client_secret: None"), "{shown}");
    }

    /// HOST-08: an offline instance makes no outbound request of any kind, so
    /// it gets no client rather than a client that refuses per request.
    #[test]
    fn an_offline_instance_fetches_nothing() {
        let mut config = ServerConfig {
            site_config: Arc::new(serde_json::json!({
                "auth": { "mode": "jwt", "jwt": { "jwksUrl": "https://idp.example/jwks" } }
            })),
            ..ServerConfig::default()
        };
        config.offline = true;
        let state = contribute(&Arc::new(AppState::new(config)))
            .state
            .expect("a state");
        assert!(state.http.is_none());
        assert!(state.exchange.is_none());
        assert!(
            format!("{:?}", state.jwks_source()).contains("NoSource"),
            "an offline instance resolves only what it was given"
        );
    }

    #[test]
    fn a_jwt_instance_with_no_jwks_url_falls_back_rather_than_pretending() {
        let state = contribute(&app(serde_json::json!({
            "auth": { "mode": "jwt" }
        })))
        .state
        .expect("a state");
        assert!(state.http.is_some(), "the client is still built");
        assert!(
            format!("{:?}", state.jwks_source()).contains("NoSource"),
            "nothing to fetch from"
        );
    }

    /// The bootstrap, end to end through `contribute`: a subject named in
    /// `auth.operators` holds the role the key gives them.
    #[test]
    fn a_configured_operator_is_elevated_by_the_state_the_layer_reads() {
        use crate::auth::roles::{Permission, Role};

        let state = contribute(&app(serde_json::json!({
            "auth": {
                "mode": "password",
                "operators": [{ "subject": "ana", "role": "owner" }]
            }
        })))
        .state
        .expect("a state");

        let roles = state.roles.as_ref().expect("a role source");
        let grant = roles.grant_for("ana").expect("ana is an operator");
        assert_eq!(grant.role, Role::Owner);
        assert!(
            grant.allows(Permission::SettingsWrite),
            "enough to add the first member"
        );
        assert!(roles.grant_for("somebody-else").is_none());
    }

    /// Absent elevates nobody, and says so by having no source at all rather
    /// than an empty one that looks configured.
    #[test]
    fn no_operators_means_no_role_source() {
        let state = contribute(&app(serde_json::json!({
            "auth": { "mode": "password" }
        })))
        .state
        .expect("a state");
        assert!(state.roles.is_none());

        let empty = contribute(&app(serde_json::json!({
            "auth": { "mode": "password", "operators": [] }
        })))
        .state
        .expect("a state");
        assert!(empty.roles.is_none());
    }

    /// A shared subject is refused by the layer whatever the key says. The
    /// operator key cannot be used to hand the site password an admin role,
    /// which is defect 95 approached from the configuration side.
    #[test]
    fn naming_the_shared_password_subject_as_an_operator_elevates_nobody() {
        use crate::auth::session::Principal;

        let state = contribute(&app(serde_json::json!({
            "auth": {
                "mode": "password",
                "operators": [{ "subject": "password:production", "role": "owner" }]
            }
        })))
        .state
        .expect("a state");

        // The source will answer for the string...
        assert!(
            state
                .roles
                .as_ref()
                .expect("a source")
                .grant_for("password:production")
                .is_some()
        );
        // ...and the layer never asks it for a shared principal.
        let shared = crate::auth::layer::principal_for_test(
            &state,
            Principal::new("password:production")
                .with_via("password")
                .shared(),
        );
        assert_eq!(shared.role, crate::auth::roles::Role::Reader);
        assert!(shared.grant.is_none());
    }

    fn with_mail(mail: serde_json::Value) -> serde_json::Value {
        serde_json::json!({
            "auth": { "mode": "managed" },
            "seo": { "canonicalOrigin": "https://docs.acme.com" },
            "mail": mail
        })
    }

    /// AUTH-05: a configured block reaches the state the magic-link handler
    /// reads. Without this the link is minted and `NoMail` drops it, which
    /// looks from outside exactly like a working sign-in.
    #[test]
    fn a_configured_mail_block_becomes_the_sender_the_handler_uses() {
        let contribution = contribute(&app(with_mail(serde_json::json!({
            "from": "Docs <docs@example.com>",
            "smtp": { "host": "smtp.example.com" }
        }))));
        assert!(
            !contribution
                .routes
                .diagnostics
                .iter()
                .any(|d| d.code == code::E0816),
            "a valid block raises nothing"
        );
        let state = contribution.state.expect("a state");
        assert!(
            state.mail.is_some(),
            "the handler has something to send with"
        );
    }

    #[test]
    fn no_mail_block_is_not_an_error_and_leaves_the_sender_unset() {
        let contribution = contribute(&app(serde_json::json!({
            "auth": { "mode": "managed" }
        })));
        let state = contribution.state.expect("a state");
        assert!(state.mail.is_none());
        assert!(
            !contribution
                .routes
                .diagnostics
                .iter()
                .any(|d| d.code == code::E0816)
        );
    }

    /// The failure nothing else can show. AUTH-09 makes the endpoint answer
    /// identically whether or not a link went out, so a block that cannot send
    /// is invisible from the site: it has to be a diagnostic at startup.
    #[test]
    fn a_mail_block_that_cannot_send_is_a_diagnostic_rather_than_a_silent_nothing() {
        let contribution = contribute(&app(with_mail(serde_json::json!({
            "from": "docs at example.com",
            "smtp": { "host": "smtp.example.com" }
        }))));
        let raised = contribution
            .routes
            .diagnostics
            .iter()
            .find(|d| d.code == code::E0816)
            .expect("a mail block that cannot send is reported");
        assert!(raised.message.contains("mail.from"), "{}", raised.message);
        assert!(
            contribution.state.expect("a state").mail.is_none(),
            "a sender that could not be built is absent, not a broken one"
        );
    }

    #[test]
    fn a_secret_reference_with_no_secret_store_is_reported_rather_than_read_as_no_password() {
        let contribution = contribute(&app(with_mail(serde_json::json!({
            "from": "docs@example.com",
            "smtp": {
                "host": "smtp.example.com",
                "username": "docs",
                "password": "secret:smtp"
            }
        }))));
        let raised = contribution
            .routes
            .diagnostics
            .iter()
            .find(|d| d.code == code::E0816)
            .expect("an unresolvable reference is reported");
        assert!(
            raised.message.contains("no secret store"),
            "{}",
            raised.message
        );
    }

    /// A link in an email has no page to resolve a relative URL against, so a
    /// site that configures mail and no canonical origin would send links
    /// nobody can click.
    #[test]
    fn mail_without_a_canonical_origin_is_refused_rather_than_sending_a_relative_link() {
        let contribution = contribute(&app(serde_json::json!({
            "auth": { "mode": "managed" },
            "mail": { "from": "docs@example.com", "smtp": { "host": "smtp.example.com" } }
        })));
        let raised = contribution
            .routes
            .diagnostics
            .iter()
            .find(|d| d.code == code::E0816)
            .expect("no origin is reported");
        assert!(
            raised.message.contains("canonicalOrigin"),
            "{}",
            raised.message
        );
    }

    /// A `contains("mail.from")` cannot see that the rest of the sentence is
    /// broken. WP-28 shipped a user-visible string with thirty spaces in the
    /// middle of it through a green gate on exactly that assertion: a Rust
    /// line-continuation written through a Python heredoc lost its backslash,
    /// so the source indentation ended up inside the literal. Nothing in the
    /// toolchain objects — it is valid Rust, `cargo fmt` leaves it alone and
    /// clippy has no opinion. The only detector is looking at the whole
    /// string, so this looks at every E0816 message rather than at any one of
    /// them.
    #[test]
    fn no_diagnostic_carries_collapsed_indentation() {
        let sites = [
            serde_json::json!({ "auth": { "mode": "managed" },
                                "mail": { "from": "docs at example.com",
                                          "smtp": { "host": "smtp.example.com" } },
                                "seo": { "canonicalOrigin": "https://docs.acme.com" } }),
            serde_json::json!({ "auth": { "mode": "managed" },
                                "mail": { "from": "docs@example.com",
                                          "smtp": { "host": "smtp.example.com" } } }),
            serde_json::json!({ "auth": { "mode": "managed" }, "mail": { "nonsense": true } }),
            serde_json::json!({ "auth": { "mode": "managed" },
                                "mail": { "from": "docs@example.com",
                                          "smtp": { "host": "h", "username": "u",
                                                    "password": "secret:smtp" } },
                                "seo": { "canonicalOrigin": "https://docs.acme.com" } }),
        ];
        let mut seen = 0;
        for site in sites {
            for raised in contribute(&app(site)).routes.diagnostics.iter() {
                seen += 1;
                assert!(
                    !raised.message.contains("  "),
                    "a run of spaces in a message means a lost line continuation: {:?}",
                    raised.message
                );
            }
        }
        assert_eq!(seen, 4, "each of the four sites raises exactly one");
    }

    #[test]
    fn the_origins_are_the_canonical_one_and_its_aliases() {
        let config = serde_json::json!({
            "seo": { "canonicalOrigin": "https://docs.acme.com/" },
            "domains": { "aliases": ["https://acme.dev", "https://docs.acme.io/"] }
        });
        assert_eq!(
            origins(&config),
            [
                "https://docs.acme.com",
                "https://acme.dev",
                "https://docs.acme.io"
            ]
        );
        assert!(origins(&serde_json::json!({})).is_empty());
    }
}
