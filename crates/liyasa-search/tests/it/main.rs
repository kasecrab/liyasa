//! One test binary for the crate. Every former `tests/*.rs` is a module here;
//! add a `mod` line for a new file. One binary links the dependency tree once
//! instead of once per file, which is most of a test build's cost and disk.

mod cfg_50;
mod parity;
mod src_01;
mod src_02_tokenizers;
mod src_03_ranking;
mod src_04_query;
mod src_05_budget;
mod src_05_index_built;
mod src_06_hybrid;
mod src_07_incremental;
mod src_10_agents;
mod src_12_personalized;
mod support;
mod wasm_clean;
