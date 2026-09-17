//! Authentication, authorization and custom domains (PRD §19, WP-15).
//!
//! The subtree is self-contained on purpose. `AppState`, the store and the
//! migrations all belong to WP-14 and WP-15 owns only this directory, so the
//! auth state cannot be a field on `AppState` and cannot have a table. It
//! hangs off [`AuthState`] instead, and the endpoint table is an
//! [`axum::Router`] the server merges.
//!
//! The practical consequence for now is a good one: every unit here is
//! reachable from a test without a listener, a database or a browser.
//!
//! TODO(rfc-1501): fold `AuthState` into `AppState` and move the session,
//! magic-link and domain tables into `migrations/` once WP-14 can take them.

// `Diagnostic` is 160 bytes and is the workspace's user-facing error type,
// frozen in `liyasa-core`. Boxing it in this subtree alone would make these
// signatures differ from every other crate's, and every path that returns one
// here is a configuration or verification failure rather than a hot loop.
#![allow(clippy::result_large_err)]

pub mod base64url;
pub mod clock;
pub mod config;
pub mod cookie;
pub mod csrf;
pub mod dns;
pub mod domains;
pub mod groups;
pub mod password;
pub mod random;
pub mod roles;
pub mod session;
pub mod variant;

pub use config::{AuthConfig, Mode};
pub use domains::{Domain, Registry as DomainRegistry};
pub use groups::{Decision, Declared, SiteDefault};
pub use roles::{Grant, Permission, Role};
pub use session::{Principal, Session, Sessions};
pub use variant::{CacheKey, Entry, PrerenderedSet, ReaderFields, VariantCache};
