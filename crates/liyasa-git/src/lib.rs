//! Git provider integrations (PRD §18, GIT-01..GIT-11).
//!
//! The crate root is shared: append your `pub mod` and `pub use` lines rather
//! than rewriting the file.
//!
//! Nothing here raises a diagnostic code. Codes 0800-0899 belong to
//! `liyasa-server` in the range table of `codes.toml` and this crate has no
//! range of its own, so every failure is a typed error and the caller —
//! `liyasa-server`'s deploy module — maps it to the code a user sees
//! (`plan/rfcs/1601-clone-policy-without-gix.md`).

pub mod event;
pub mod repo;
pub mod webhook;
