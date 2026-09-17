//! The Liyasa Markdown language server (PRD §15.7 ED-61, §16.1 CLI-25).
//!
//! Diagnostics and the preview come from the build's own page pipeline, so the
//! editor cannot disagree with the build about whether a page is valid.
//!
//! Completion and hover read the line the cursor is on rather than the segment
//! the scanner made of it. That is deliberate: `:::no`, `{{ facts.` and `](/gui`
//! are all states the scanner is entitled to call ordinary Markdown, and they
//! are exactly the states an author asks for a completion from. [`completion`]
//! reads what is behind the cursor, because that is all that has been typed;
//! [`locate`] reads the whole token around it, because hover and go-to-
//! definition are about a name already written.
//!
//! It adds no dependency the workspace did not already carry: the transport in
//! [`jsonrpc`] and the wire types in [`protocol`] are written here rather than
//! taken from `tower-lsp` or `lsp-types`, because PRD §6.2.1 names neither and
//! §31.6 item 7 forbids a dependency without a row. See
//! `plan/rfcs/3000-no-lsp-crate-is-named.md`.

pub mod analysis;
pub mod completion;
pub mod definition;
pub mod diagnostics;
pub mod hover;
pub mod jsonrpc;
pub mod locate;
pub mod protocol;
pub mod server;
pub mod text;
pub mod uri;
pub mod workspace;

pub use server::{Server, serve, serve_stdio};
