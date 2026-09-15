//! OpenAPI reference documentation (PRD §13).
//!
//! A spec is read once into an order-preserving tree ([`tree`]), normalized to
//! the 3.1 JSON Schema 2020-12 shape whatever dialect it arrived in, and then
//! deserialized into the typed model the rest of Liyasa renders. Downstream
//! consumers handle one shape; 3.0's `nullable`, boolean `exclusiveMinimum`,
//! and single `type` never leave this crate.

pub mod tree;
pub mod version;

pub use version::SpecVersion;
