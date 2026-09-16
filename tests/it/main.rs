//! One test binary for the crate. Every former `tests/*.rs` is a module here;
//! add a `mod` line for a new file. One binary links the dependency tree once
//! instead of once per file, which is most of a test build's cost and disk.

#[path = "../config/cfg_01.rs"]
mod cfg_01;
#[path = "../config/cfg_03_presets.rs"]
mod cfg_03_presets;
#[path = "../config/cfg_04.rs"]
mod cfg_04;
#[path = "../config/cfg_30_navigation.rs"]
mod cfg_30_navigation;
#[path = "../build/cfg_64.rs"]
mod cfg_64;
#[path = "../build/cfg_65.rs"]
mod cfg_65;
#[path = "../config/cfg_90_diagnostics.rs"]
mod cfg_90_diagnostics;
#[path = "../build/cfg_90_rules.rs"]
mod cfg_90_rules;
#[path = "../config/cfg_91.rs"]
mod cfg_91;
#[path = "../config/cfg_94_schema.rs"]
mod cfg_94_schema;
#[path = "../cli/cli_02_dev.rs"]
mod cli_02_dev;
#[path = "../cli/cli_03_build.rs"]
mod cli_03_build;
#[path = "../cli/cli_05_format.rs"]
mod cli_05_format;
#[path = "../build/cm_120_changelog.rs"]
mod cm_120_changelog;
#[path = "../build/cm_15_functions.rs"]
mod cm_15_functions;
#[path = "../build/cm_36_links.rs"]
mod cm_36_links;
#[path = "../build/cm_80_hidden.rs"]
mod cm_80_hidden;
#[path = "../build/cm_82_redirects.rs"]
mod cm_82_redirects;
#[path = "../build/cm_84_files.rs"]
mod cm_84_files;
#[path = "../web/nojs.rs"]
mod nojs;
#[path = "../build/rx_02_assets.rs"]
mod rx_02_assets;
#[path = "../budget/rx_12.rs"]
mod rx_12;
#[path = "../budget/rx_14_transfer.rs"]
mod rx_14_transfer;
#[path = "../budget/thm_31.rs"]
mod thm_31;
#[path = "../web/reader.rs"]
mod web_reader;
