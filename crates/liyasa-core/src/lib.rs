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
//! `ai`, `build`, `components`, `markdown`, `net`, `server`, `store`, and
//! `verify` hold the §34.9 contracts that §6.11 assigns to crates that do not
//! exist until M1; those crates re-export from here, which costs nothing
//! because §34.7 already makes `liyasa-core` the root of the dependency tree.
//! See `plan/rfcs/0002-contract-home-crate.md`.

pub mod ai; // TODO(rfc-0002): re-exported by liyasa-ai
pub mod build; // TODO(rfc-0002): re-exported by liyasa-build
pub mod components; // TODO(rfc-0002): re-exported by liyasa-components
#[cfg(feature = "conformance")]
pub mod conformance;
pub mod diagnostics;
pub mod document;
pub mod frontmatter;
pub mod ids;
pub mod markdown; // TODO(rfc-0002): re-exported by liyasa-markdown
pub mod net; // TODO(rfc-0002): re-exported by liyasa-net
pub mod serde_time;
pub mod server; // TODO(rfc-0002): re-exported by liyasa-server
pub mod site;
pub mod source_map;
pub mod span;
pub mod store; // TODO(rfc-0002): re-exported by liyasa-store
pub mod verify; // TODO(rfc-0002): re-exported by liyasa-verify
pub mod vfs;
pub mod yaml;

pub use diagnostics::{Code, Diagnostic, Diagnostics, Severity};
pub use document::{Block, BlockKind, Document, Inline, Node, Origin, Segment, SourceDocument};
pub use frontmatter::{Frontmatter, FrontmatterFields};
pub use ids::{BlockId, BuildId, Fingerprint, Locale, PageId, Route, Version};
pub use net::{BoxFut, BoxStream, HttpClient};
pub use source_map::{SourceFile, SourceMap};
pub use span::{LineCol, SourceId, Span};
pub use vfs::{Bytes, Vfs, VfsPath};
