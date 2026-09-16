//! One test binary for the crate. Every former `tests/*.rs` is a module here;
//! add a `mod` line for a new file. One binary links the dependency tree once
//! instead of once per file, which is most of a test build's cost and disk.

mod cfg_71_errors;
mod cmp_100;
mod cmp_102;
mod render;
mod rx_01_rendering;
mod rx_22;
mod rx_23;
mod rx_40_dark;
mod rx_90_a11y;
mod thm_02_contrast;
mod thm_06_print;
mod thm_10;
mod thm_20_partials;
mod thm_22_context;
mod thm_23_eject;
mod thm_30;
mod thm_31;
mod thm_40_brand;
