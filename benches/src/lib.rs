//! The release benchmark suite (NFR-01).
//!
//! `bench run` generates a site at each of the page counts NFR-01 names,
//! measures the §6.6 scenarios on it, prints a Markdown table for the changelog
//! and judges every figure against its budget. The library half exists so the
//! tests drive the same code the release runs, rather than a second copy of it.

pub mod budget;
pub mod measure;
pub mod report;
pub mod site;
