//! Shared types and contracts for Liyasa (PRD §6.11, §34.9).
//!
//! Every signature here is frozen after M0: adding a variant to a
//! `#[non_exhaustive]` enum or an optional field to a struct is a minor change,
//! anything else is an RFC.
//!
//! The crate performs no I/O of its own. File access goes through [`vfs::Vfs`]
//! and remote access through [`net::HttpClient`], both injected by the caller,
//! so the whole crate builds for `wasm32-unknown-unknown` (§6.2).

pub mod diagnostics;
pub mod span;

pub use diagnostics::{Code, Diagnostic, Diagnostics, Severity};
pub use span::{LineCol, SourceId, Span};
