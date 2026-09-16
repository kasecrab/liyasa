//! One module per command, and the table that routes to them.

pub mod completions;
pub mod version;

use crate::Exit;
use crate::cli::{Command, Global};

pub fn dispatch(global: &Global, command: Command) -> Exit {
    match command {
        Command::Version(args) => version::run(global, &args),
        Command::Completions(args) => completions::run(global, &args),

        // Written in the order of §16.1; each becomes a `mod` line above as it
        // lands.
        Command::New(_) => pending("new"),
        Command::Dev(_) => pending("dev"),
        Command::Build(_) => pending("build"),
        Command::Validate(_) => pending("validate"),
        Command::Format(_) => pending("format"),
        Command::Verify(_) => pending("verify"),
        Command::BrokenLinks(_) => pending("broken-links"),
        Command::Test(_) => pending("test"),
        Command::Score(_) => pending("score"),
        Command::Export(_) => pending("export"),
        Command::Serve(_) => pending("serve"),
        Command::Search(_) => pending("search"),
        Command::Schema(_) => pending("schema"),
        Command::MigrateConfig(_) => pending("migrate-config"),
        Command::Theme(_) => pending("theme"),
        Command::Update(_) => pending("update"),
        Command::Telemetry(_) => pending("telemetry"),
        Command::Doctor(_) => pending("doctor"),
        Command::Companion(_) => pending("companion"),
        Command::Lock(_) => pending("lock"),
    }
}

/// A command whose arguments parse but whose body is not written yet. It is a
/// project error rather than a usage error: the command line was correct.
fn pending(name: &str) -> Exit {
    eprintln!("liyasa {name}: not implemented in this build");
    Exit::Errors
}
