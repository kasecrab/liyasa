//! The runners that execute user code, and the sandboxes they execute it in
//! (VER-01, VER-02.1, VER-02.6 to VER-02.9, VER-02.17, VER-03 to VER-06).
//!
//! `core::runners` holds the half of §14 that only reads code. This half runs
//! it, and VER-03 is the reason the two are separate modules: nothing here
//! executes anything outside a `Sandbox`, so a machine with no container
//! runtime still gets every in-process check.
//!
//! `Diagnostic` is 160 bytes and is the workspace's one error type; a runner
//! that could not build its job has a diagnostic and nothing smaller to
//! return. Boxing it at every call site to satisfy `result_large_err` would
//! cost every reader of this module and buy one allocation's worth of moves on
//! the path that is already failing.
#![allow(clippy::result_large_err)]

use std::sync::Arc;

use liyasa_core::diagnostics::Diagnostics;

pub mod attrs;
pub mod cache;
pub mod chain;
pub mod code;
pub mod custom;
pub mod hidden;
pub mod image;
pub mod lang;
pub mod sandbox;
pub mod staging;

pub use attrs::{BlockVerify, DEFAULT_TIMEOUT, Mode, Site, check_id};
pub use cache::ResultCache;
pub use code::{Binding, Bindings, SandboxRunner};
pub use custom::Custom;
pub use hidden::{DEFAULT_PREFIX, Split};
pub use image::{ImagePin, Images};
pub use lang::{Go, Language, Node, Python, Rust, Shell};
pub use sandbox::{Builder, Host};

use crate::core::config::VerifyConfig;
use crate::core::runners::Registry;

/// The sandboxed runners, in the order a language is offered to them.
///
/// The declared ones come first: `verify.runners.custom` exists so an operator
/// can override what Liyasa would do with a language, and a registry that
/// offered the built-in first would make that impossible.
///
/// A declaration Liyasa cannot use is `E0613` and the rest of the registry is
/// still built, for the reason `core::config` reads one key at a time.
pub fn sandboxed(config: &VerifyConfig) -> (Registry, Diagnostics) {
    let images = Images::new(&config.runners);
    let mut problems = Diagnostics::new();
    let mut runners: Vec<Arc<dyn liyasa_core::verify::Runner>> = Vec::new();

    for declared in &config.runners.custom {
        match Custom::new(declared) {
            Ok(custom) => runners.push(Arc::new(runner(Arc::new(custom), &images, config))),
            Err(problem) => problems.push(problem),
        }
    }
    for language in built_in() {
        runners.push(Arc::new(runner(language, &images, config)));
    }
    (Registry::new(runners), problems)
}

/// Every language VER-02.1 and VER-02.6 to VER-02.9 name.
pub fn built_in() -> Vec<Arc<dyn Language>> {
    vec![
        Arc::new(Shell),
        Arc::new(Python),
        Arc::new(Node),
        Arc::new(Go),
        Arc::new(Rust),
    ]
}

fn runner(language: Arc<dyn Language>, images: &Images, config: &VerifyConfig) -> SandboxRunner {
    SandboxRunner::new(language, images.clone()).with_hide_prefix(config.hide_prefix.clone())
}

/// A registry assembled with bindings the orchestrator already holds.
pub fn sandboxed_with(config: &VerifyConfig, bindings: Bindings) -> (Registry, Diagnostics) {
    let images = Images::new(&config.runners);
    let mut problems = Diagnostics::new();
    let mut runners: Vec<Arc<dyn liyasa_core::verify::Runner>> = Vec::new();
    for declared in &config.runners.custom {
        match Custom::new(declared) {
            Ok(custom) => runners.push(Arc::new(
                runner(Arc::new(custom), &images, config).with_bindings(bindings.clone()),
            )),
            Err(problem) => problems.push(problem),
        }
    }
    for language in built_in() {
        runners.push(Arc::new(
            runner(language, &images, config).with_bindings(bindings.clone()),
        ));
    }
    (Registry::new(runners), problems)
}

#[cfg(test)]
mod tests;
