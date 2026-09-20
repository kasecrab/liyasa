//! Repository tasks, exposed as a library so the integration tests can call
//! the same code the `xtask` binary runs.

pub mod codes;
pub mod conformance;
pub mod corpus;
pub mod corpus_import;
pub mod corpus_seed;
/// Native only: it reads the CLI's clap tree, and `liyasa-cli` does not build
/// for wasm. See the target section in `Cargo.toml`.
#[cfg(not(target_family = "wasm"))]
pub mod flags;
pub mod licences;
pub mod lints;
pub mod notices;
/// Native only: it maintains the `tests/pins/*.txt` ratchets from
/// [`flags::audit`], so it inherits that module's dependency on the CLI.
#[cfg(not(target_family = "wasm"))]
pub mod pins;
pub mod parity;
pub mod schemas;
pub mod spike;
pub mod workflows;
pub mod workspace;
