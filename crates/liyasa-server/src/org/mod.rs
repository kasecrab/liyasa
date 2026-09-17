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

pub mod audit;
pub mod credits;
pub mod meter;
pub mod model;
pub mod notify;
pub mod plan;
pub mod region;
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
    Mount::routes(routes::router(Arc::new(state::OrgState::from_app(app))))
}
