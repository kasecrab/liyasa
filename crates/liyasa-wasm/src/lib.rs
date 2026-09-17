//! The editor's and the search worker's WebAssembly bindings (PRD §6.2,
//! ED-06, ED-07).
//!
//! Two objects cross the boundary. [`Session`] is the editor's: it holds the
//! component registry and the template environment once, so a keystroke pays
//! for expansion and parsing and nothing else, and answers [`Session::parse`],
//! [`Session::validate`], [`Session::preview`] and [`Session::serialize`].
//! [`Searcher`] is the search worker's, and answers [`Searcher::search`] over
//! `liyasa-idx` shard bytes the worker fetched.
//!
//! Everything in [`api`] is the frozen shape `web/editor` and `web/reader`
//! program against; `ts/liyasa-wasm.d.ts` is generated from it and checked in,
//! so a change to a field is a visible change to a committed file.
//!
//! The crate does no I/O. Paths resolve through [`vfs::EditorVfs`], which is
//! ED-07's lazy, server-backed resolver, and the fetch itself is the host's.

pub mod api;
pub mod bindings;
pub mod blocks;
pub mod budget;
pub mod search;
pub mod session;
pub mod ts;
pub mod vfs;

pub use api::{
    ParseRequest, ParseResponse, PreviewRequest, PreviewResponse, SearchRequest, SearchResponse,
    SerializeRequest, SerializeResponse, ValidateRequest, ValidateResponse,
};
pub use search::Searcher;
pub use session::Session;
pub use vfs::EditorVfs;
