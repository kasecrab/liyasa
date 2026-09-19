//! The organization and cloud control plane (ORG-01..03, ORG-10, ORG-20..21,
//! ORG-30..33, HOST-10..13, MIG-11).
//!
//! Everything here is one subtree of the server in the sense of RFC 1403, and
//! every piece of its state is held in [`state::OrgState`] rather than in the
//! store, for the reason RFC 1501 gives for `AuthState`: `liyasa-store` and
//! `migrations/` are WP-14's paths, and a package that cannot add a table can
//! still be complete, testable and honest about where its rows live. What that
//! costs is written down in RFC 2800 rather than left to be discovered.
//!
//! ORG-32 is the shape of the whole module: an OSS instance runs
//! [`plan::Plan::unlimited`] and takes the same code path a Free project takes.
//! No handler asks which edition it is in.

// `Diagnostic` is 160 bytes and is the workspace's user-facing error type,
// frozen in `liyasa-core`. Every path here that returns one is a plan refusal
// or a validation failure rather than a hot loop, and boxing it in this
// subtree alone would make these signatures differ from `auth/`, which made
// the same call for the same reason.
#![allow(clippy::result_large_err)]

pub mod audit;
pub mod credits;
pub mod meter;
pub mod model;
pub mod notify;
pub mod plan;
pub mod region;
pub mod roles;
pub mod routes;
pub mod slo;
pub mod state;

use std::sync::Arc;

use crate::routes::AppState;
use crate::routes::mount::Mount;

/// RFC 1403's registration. One entry: the three route groups share one
/// [`state::OrgState`], and `mount` is a bare function pointer with nowhere to
/// keep it, so three entries would build three organizations rather than three
/// views of one. The permissions are in [`routes::TABLE`] and applied with the
/// seam's own `guarded`.
///
/// The line this package would add to `routes::mount::subtrees`, which is
/// WP-14's file (RFC 2800):
///
/// ```text
/// Subtree { name: "org", permission: None, mount: crate::org::mount },
/// ```
pub fn mount(app: &Arc<AppState>) -> Mount {
    mount_from(state(app))
}

/// The organization this instance serves, built from what `AppState` already
/// holds and nothing else (RFC 2800: there is no `organization` config key and
/// this package will not invent one).
///
/// Call this **once** per server. Everything that needs the organization —
/// the router, the role source — takes the same `Arc`, because two of them
/// are two organizations: a project created through one is invisible to the
/// other, and a role source built over the second would never see the members
/// the first has. That is WP-15's two-`AuthState` trap in another crate, and
/// it fails the same silent way.
pub fn state(app: &Arc<AppState>) -> Arc<state::OrgState> {
    Arc::new(state::OrgState::from_app(app))
}

/// The subtree over an organization somebody else built. This is the one to
/// use from `routes::application` once the role source is wired, because the
/// role source needs the same `Arc` (RFC 2802).
pub fn mount_from(state: Arc<state::OrgState>) -> Mount {
    Mount::routes(routes::router(state))
}

/// Organization membership as `auth::layer::Roles`, over the same
/// organization the subtree serves (defect 65, RFC 2802).
///
/// Returns the concrete type rather than `Arc<dyn Roles>` because
/// `auth::layer` is not on `main` yet; `AuthState::with_roles` takes the trait
/// object, and the three-line `impl` that makes this coerce lands with WP-15.
pub fn role_source(state: Arc<state::OrgState>) -> Arc<roles::MembershipRoles> {
    Arc::new(roles::MembershipRoles::new(state))
}
