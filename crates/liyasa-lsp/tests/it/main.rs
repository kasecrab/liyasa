//! One test binary for the crate. Every test file is a module here; add a `mod`
//! line for a new file rather than a `[[test]]` row (RFC 0007).

mod analysis;
mod completion;
mod definition;
mod diagnostics;
mod hover;
mod jsonrpc;
mod locate;
mod text;
mod uri;
mod workspace;
