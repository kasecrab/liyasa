//! Where a subtree router is mounted (RFC 1403).
//!
//! `liyasa serve` is more than one package's routes. Auth, deployments, MCP
//! and the organization API each own a subtree of this crate and each build
//! their own router from their own state. This module is the one place they
//! are composed, so that the application the binary runs and the application a
//! test asserts against are the same object.
//!
//! A package registers by writing `mount` beside its router and appending one
//! line to [`subtrees`]. Append; do not reorder.

use std::sync::Arc;

use axum::Router;
use liyasa_core::diagnostics::Diagnostics;

use super::AppState;

/// One subtree of the server.
pub struct Subtree {
    pub name: &'static str,
    /// Builds the subtree's router from the shared state, or explains why this
    /// instance has none.
    pub mount: fn(&Arc<AppState>) -> Mount,
}

impl std::fmt::Debug for Subtree {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("Subtree").field("name", &self.name).finish()
    }
}

/// What a subtree contributed. Not configured is not the same as broken: a
/// subtree this instance has no configuration for mounts nothing and says so,
/// rather than mounting routes that fail per request or vanishing silently.
pub struct Mount {
    pub router: Option<Router>,
    /// Present exactly when `router` is `None`.
    pub skipped: Option<String>,
    /// Raised once at startup rather than once per request.
    pub diagnostics: Diagnostics,
}

impl Mount {
    pub fn routes(router: Router) -> Self {
        Self {
            router: Some(router),
            skipped: None,
            diagnostics: Diagnostics::new(),
        }
    }

    pub fn skipped(reason: impl Into<String>) -> Self {
        Self {
            router: None,
            skipped: Some(reason.into()),
            diagnostics: Diagnostics::new(),
        }
    }

    pub fn with_diagnostics(mut self, diagnostics: Diagnostics) -> Self {
        self.diagnostics = diagnostics;
        self
    }
}

/// Every subtree, in mount order. One line per package (RFC 1403).
///
/// WP-19 adds `Subtree { name: "mcp", mount: crate::mcp::mount }` and WP-28
/// `Subtree { name: "org", mount: crate::org::mount }` once those modules
/// exist.
pub fn subtrees() -> &'static [Subtree] {
    &[
        Subtree {
            name: "auth",
            mount: auth,
        },
        Subtree {
            name: "deploy",
            mount: deploy,
        },
    ]
}

// The two adapters below belong in `auth/` and `deploy/`, next to the routers
// they build. They are here because those directories are WP-15's and WP-16's
// paths and this package may not write in them. Moving one is a cut and paste
// plus changing its line in `subtrees` (RFC 1403).

/// WP-15. Belongs in `crate::auth` as `pub fn mount`.
fn auth(app: &Arc<AppState>) -> Mount {
    use crate::auth::config::AuthConfig;
    use crate::auth::state::AuthState;

    let config = match AuthConfig::from_site_config(&app.config.site_config) {
        Ok(config) => config,
        Err(diagnostic) => {
            let mut diagnostics = Diagnostics::new();
            diagnostics.push(diagnostic);
            // A config that does not parse is not a public site: saying so
            // would hide the mistake behind a working server.
            return Mount::skipped("the `auth` section could not be read")
                .with_diagnostics(diagnostics);
        }
    };
    if config.mode.is_public() {
        // AUTH-01: a public site has no auth code path at all.
        return Mount::skipped("`auth.mode` is public, so there is nothing to sign in to");
    }

    let origins = origins(&app.config.site_config);
    match AuthState::new(config, &app.config.env, origins, Default::default()) {
        Ok((state, diagnostics)) => {
            let state = Arc::new(state.with_proxies(app.proxies.clone()));
            Mount::routes(crate::auth::routes::router(state)).with_diagnostics(diagnostics)
        }
        // Not E0803: the configuration is fine and the system has no
        // randomness, which is a different failure from an invalid `auth`
        // section. Borrowing a code that nearly fits is how a code stops
        // meaning anything.
        Err(error) => Mount::skipped(format!("authentication could not be started: {error}")),
    }
}

/// WP-16. Belongs in `crate::deploy` as `pub fn mount`.
fn deploy(app: &Arc<AppState>) -> Mount {
    use crate::deploy::service::DeployState;

    match DeployState::new(app.clone()) {
        Some(state) => Mount::routes(crate::deploy::routes::router(Arc::new(state))),
        // `DeployState::new` returns `None` without a store, because every
        // deploy route needs one.
        None => Mount::skipped("no store is configured, and every deploy route needs one"),
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

    #[test]
    fn a_mount_says_why_exactly_when_it_has_no_router() {
        let skipped = Mount::skipped("no store");
        assert!(skipped.router.is_none() && skipped.skipped.is_some());
        let mounted = Mount::routes(Router::new());
        assert!(mounted.router.is_some() && mounted.skipped.is_none());
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

    #[test]
    fn no_subtree_is_registered_twice() {
        let names: Vec<&str> = subtrees().iter().map(|s| s.name).collect();
        let mut sorted = names.clone();
        sorted.sort_unstable();
        sorted.dedup();
        assert_eq!(names.len(), sorted.len(), "{names:?}");
    }
}
