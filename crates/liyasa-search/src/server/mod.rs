//! The server-side index (`liyasa serve`): tantivy for retrieval, `idx` for
//! ranking (SRC-06, SRC-11, plan/rfcs/0703-server-ranks-with-idx.md).
//!
//! Never built for `wasm32`: tantivy depends on rayon unconditionally and its
//! `Directory` API is synchronous (§12.1 SRC-05).

pub mod schema;
pub mod searcher;
pub mod writer;

pub use schema::SearchSchema;
pub use searcher::{ServerSearcher, hybrid};
pub use writer::{IndexStats, ServerIndex};
