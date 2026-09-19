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
use axum::response::IntoResponse;
use http::StatusCode;
use liyasa_core::diagnostics::Diagnostics;

use super::AppState;
use super::problem::Problem;
use crate::auth::roles::Permission;
use crate::auth::session::Principal;

/// One subtree of the server.
pub struct Subtree {
    pub name: &'static str,
    /// What a caller must hold for every route this entry mounts. `None` is a
    /// surface that is public, or that authorizes inside its own handlers.
    ///
    /// It is declared here rather than applied by the subtree because
    /// `Permission` lives in this crate and `liyasa-server` depends on the
    /// crates most subtrees live in (§34.7) — so a subtree outside this crate
    /// cannot name a permission without a dependency cycle. A subtree that
    /// needs two different permissions, or one gated surface and one public
    /// route, registers two entries.
    pub permission: Option<Permission>,
    /// Builds the subtree's router from the shared state, or explains why this
    /// instance has none.
    pub mount: fn(&Arc<AppState>) -> Mount,
}

impl std::fmt::Debug for Subtree {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("Subtree")
            .field("name", &self.name)
            .field("permission", &self.permission)
            .finish()
    }
}

/// Wraps a subtree's router so every route in it requires `permission`.
///
/// A caller with no session at all is told to sign in; a caller whose role
/// does not carry the permission is refused. The two are different answers
/// because they need different actions from whoever reads them.
pub fn guarded(router: Router, permission: Permission) -> Router {
    router.layer(axum::middleware::from_fn(
        move |request: axum::extract::Request, next: axum::middleware::Next| async move {
            match request.extensions().get::<Principal>() {
                None => Problem::new(StatusCode::UNAUTHORIZED, "Not signed in")
                    .detail("this endpoint needs a session")
                    .into_response(),
                // `allows` belongs to a membership, which carries custom roles as
                // well; a `Principal` carries one role, so this asks that role.
                Some(principal) if !principal.role.permissions().contains(&permission) => {
                    Problem::new(StatusCode::FORBIDDEN, "Not permitted")
                        .detail(format!(
                            "this endpoint needs `{permission:?}`, and this role does not carry it"
                        ))
                        .into_response()
                }
                Some(_) => next.run(request).await,
            }
        },
    ))
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
/// WP-19 adds `Subtree { name: "mcp", mount: crate::mcp::mount }` once that
/// module exists.
pub fn subtrees() -> &'static [Subtree] {
    &[
        Subtree {
            name: "auth",
            // Signing in cannot require being signed in.
            permission: None,
            mount: auth,
        },
        Subtree {
            // Authorizes per handler against the request's `Actor`, which is
            // finer than one permission for the whole subtree.
            name: "deploy",
            permission: None,
            mount: deploy,
        },
        Subtree {
            // Four route groups with three different answers to "who may",
            // so `org::routes::TABLE` applies the permissions with this
            // module's own `guarded`. A permission here would double-gate
            // HOST-10's public SLA, which is published for people deciding
            // whether to buy.
            name: "org",
            permission: None,
            mount: crate::org::mount,
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
            // The endpoint table is one consumer of this state and the
            // session layer is the other. Published here, from the single
            // place it is built, so the layer cannot get a different one
            // (RFC 1403, "One state, two consumers").
            app.publish_auth_state(state.clone());
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
