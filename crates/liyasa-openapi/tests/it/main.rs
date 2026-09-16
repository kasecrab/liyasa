//! One test binary for the crate. Every former `tests/*.rs` is a module here;
//! add a `mod` line for a new file. One binary links the dependency tree once
//! instead of once per file, which is most of a test build's cost and disk.

mod api_01_load;
mod api_02_refs;
mod api_03_nav;
mod api_04_augment;
mod api_05_ext;
mod api_10_layout;
mod api_11_bounds;
mod api_11_complex;
mod api_13_pills;
mod api_14_markdown;
mod api_30_codegen;
mod api_30_registry;
mod api_31_prefill;
mod api_32_verify;
mod api_44_prefill;
mod api_53_validate;
mod compat;
mod support;
