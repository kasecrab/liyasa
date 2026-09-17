//! The truth graph: what a page depends on, and what a change reaches (§14.12).
//!
//! Two of §14.12's contracts live here. [`PageExtractor`] is the pure half: a
//! Rendered AST and the page's expansion record in, a sorted list of
//! [`Edge`](liyasa_core::document::Edge)s out. The store is the stateful half,
//! and the only stateful piece of the truth engine — `SnapshotDiffer` and
//! `ImpactQuery` (WP-20b) and `DriftEngine` (WP-20c) read it and keep no state
//! of their own.

pub mod extract;
pub mod store;

pub use extract::PageExtractor;
pub use store::{DependencyRecord, MemoryGraph};
