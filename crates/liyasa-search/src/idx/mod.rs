//! The `liyasa-idx` static index format (PRD §12.2).
//!
//! Nothing under this module may reach for tantivy, a thread pool, or the file
//! system: the browser reads this format in a Web Worker, which has none of
//! them. The writer is handed section documents and returns bytes; the reader
//! is handed bytes and returns hits.

pub mod docs;
pub mod field;
pub mod postings;
pub mod snippets;
pub mod tokenize;
pub(crate) mod varint;
