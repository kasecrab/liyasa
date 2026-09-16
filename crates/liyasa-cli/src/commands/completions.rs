//! CLI-26: `liyasa completions <shell>`.
//!
//! Generated from the same [`Cli`](crate::cli::Cli) the binary parses with, so
//! a flag cannot exist in one and not the other.

use clap::CommandFactory;

use crate::Exit;
use crate::cli::{Cli, Completions, Global};

pub fn run(_global: &Global, args: &Completions) -> Exit {
    let mut command = Cli::command();
    clap_complete::generate(
        args.shell,
        &mut command,
        crate::commands::version::NAME,
        &mut std::io::stdout(),
    );
    Exit::Success
}
