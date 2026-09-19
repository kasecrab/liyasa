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

use liyasa_core::diagnostics::Diagnostics;

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
        Ok((state, diagnostics)) => {
            let state = Arc::new(state.with_proxies(app.proxies.clone()));
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
