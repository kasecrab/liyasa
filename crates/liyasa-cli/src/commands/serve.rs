//! CLI-11: `liyasa serve`.
//!
//! The server of §18 is `liyasa-server`: routing by `Host`, reader
//! authentication, TLS with automatic certificates, a database, deployments,
//! and the analytics collector. That crate does not exist yet, and a command
//! that quietly served static files instead would be claiming to be it.
//!
//! `liyasa dev` is the one that serves a directory, and it says so.

use liyasa_core::diagnostics::{Diagnostic, code};

use crate::Exit;
use crate::cli::{Global, Serve};
use crate::ctx;

pub fn run(global: &Global, args: &Serve) -> Exit {
    let format = global.resolve(crate::cli::Format::Text);

    let what = if args.init {
        "first-time setup"
    } else if args.collector_only {
        "the analytics collector"
    } else {
        "the server"
    };

    ctx::report(
        global,
        format,
        Diagnostic::new(
            code::E0006,
            format!("{what} is not in this build"),
        )
        .help("`liyasa dev` serves a built site locally. `liyasa serve` arrives with the server crate."),
    );
    Exit::Errors
}
