//! Repository tasks, exposed as a library so the integration tests can call
//! the same code the `xtask` binary runs.

pub mod conformance;
pub mod corpus;
pub mod corpus_import;
pub mod corpus_seed;
pub mod flags;
pub mod parity;
pub mod schemas;
pub mod spike;
