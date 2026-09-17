//! The editor's and the search worker's WebAssembly bindings (PRD §6.2,
//! ED-06, ED-07).
//!
//! Everything in [`api`] is the frozen shape `web/editor` and `web/reader`
//! program against.
//!
//! The crate does no I/O.

pub mod api;
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
