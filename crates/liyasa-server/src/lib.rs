//! The Liyasa server (PRD §18, §30.2.5).
//!
//! The crate root is shared: several packages own subtrees of it. Append your
//! `pub mod` and `pub use` lines; do not rewrite the file. WP-14 contributes
//! `routes`; WP-16 contributes `deploy`.

pub mod deploy;
pub mod routes;

// WP-15 contributes `auth`: the endpoint table of AUTH-09, the session and
// group rules, the variant cache key of AUTH-13, and custom domains.
pub mod auth;

// WP-28 contributes `org`: the organization and project model, plan policy,
// credits, the audit log and notification routing (RFC 2800).
pub mod org;
