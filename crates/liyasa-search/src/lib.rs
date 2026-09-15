//! Search: section documents, ranking, the tantivy index, and the
//! `liyasa-idx` browser format (PRD §12).
//!
//! Two halves that must agree. [`idx`] is the browser format: no tantivy, no
//! threads, no I/O, so it compiles to `wasm32-unknown-unknown`. The `server`
//! feature adds the tantivy index for `liyasa serve` and for the build that
//! exports `idx`. Both score against the same global statistics and tokenize
//! through the same module, which is what makes the parity corpus of §12.2
//! pass by construction rather than by luck.
//!
//! §6.2 gives the browser format a crate of its own; this package owns one
//! path, so it is a module here instead, written so the split is a directory
//! move. See `plan/rfcs/0700-idx-inside-search.md`.

pub mod doc;
pub mod error;
// TODO(rfc-0700): `idx` becomes the crate `liyasa-idx` once it may be committed.
pub mod idx;
pub mod section;
#[cfg(feature = "server")]
pub mod server;
