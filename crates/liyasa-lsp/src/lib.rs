//! The Liyasa Markdown language server (PRD §15.7 ED-61, §16.1 CLI-25).
//!
//! The server answers over the Source Document of §7.16: `scan` gives it every
//! segment with a byte span, so a position in the editor resolves to a segment
//! and the segment decides what the request means. A cursor in a
//! `DirectiveOpen` completes component names and props; one in a `Template`
//! completes variables and facts; one in a `Markdown` segment completes links.
//!
//! It adds no dependency the workspace did not already carry: the transport in
//! [`jsonrpc`] and the wire types in [`protocol`] are written here rather than
//! taken from `tower-lsp` or `lsp-types`, because PRD §6.2.1 names neither and
//! §31.6 item 7 forbids a dependency without a row. See
//! `plan/rfcs/3000-no-lsp-crate-is-named.md`.

pub mod jsonrpc;
pub mod protocol;
pub mod text;
