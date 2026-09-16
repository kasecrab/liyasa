//! One test binary for the crate. Every former `tests/*.rs` is a module here;
//! add a `mod` line for a new file. One binary links the dependency tree once
//! instead of once per file, which is most of a test build's cost and disk.

mod mig_01_mintlify;
mod mig_02_docusaurus;
mod mig_03_mdx;
mod mig_05_carry;
mod mig_06_report;
mod support;
