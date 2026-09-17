//! CLI-11: `liyasa serve`.
//!
//! The server of §18 is `liyasa-server`: routing by `Host`, reader
//! authentication, TLS with automatic certificates, a database, deployments,
//! and the analytics collector.
//!
//! Every piece of that is on main, and so is the assembly: `liyasa-server`'s
//! own `src/main.rs` opens the store, reads the bundle, builds the router,
//! terminates TLS and drains on a signal. It is a **binary**, and all of it is
//! private, so no library call reaches it — the crate's public surface is
//! `routes`, `deploy` and `auth`, the parts and not the whole. Rebuilding that
//! assembly here would be a second copy of one program, drifting from the
//! first. RFC 0912 asks WP-14 for the one `pub async fn` its own `main` calls.
//!
//! So the command still cannot run, for a narrower reason again, and a command
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
        .help("`liyasa dev` serves a built site locally. `liyasa serve` needs `liyasa-server` to export the entry point its own binary uses (RFC 0912); run `liyasa-server` directly until it does."),
    );
    Exit::Errors
}
