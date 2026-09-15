//! Shared types and contracts for Liyasa (PRD §6.11, §34.9).
//!
//! Every signature here is frozen after M0: adding a variant to a
//! `#[non_exhaustive]` enum or an optional field to a struct is a minor change,
//! anything else is an RFC.
//!
//! The crate performs no I/O of its own. File access goes through [`vfs::Vfs`]
//! and remote access through [`net::HttpClient`], both injected by the caller,
//! so the whole crate builds for `wasm32-unknown-unknown` (§6.2).
//!
//! `net`, `build`, `server`, `verify`, `ai`, `store`, `components`, and
//! `markdown` hold the §34.9 contracts that §6.11 assigns to other crates;
//! those crates re-export from here. See `plan/rfcs/0002-contract-home-crate.md`.

pub mod diagnostics;
pub mod ids;
pub mod net; // TODO(rfc-0002): re-exported by liyasa-net
pub mod source_map;
pub mod span;
pub mod vfs;

pub use diagnostics::{Code, Diagnostic, Diagnostics, Severity};
pub use ids::{BlockId, BuildId, Fingerprint, Locale, PageId, Route, Version};
pub use net::{BoxFut, BoxStream, HttpClient};
pub use source_map::{SourceFile, SourceMap};
pub use span::{LineCol, SourceId, Span};
pub use vfs::{Bytes, Vfs, VfsPath};
