//! One module per command, and the table that routes to them.

pub mod build;
pub mod completions;
pub mod doctor;
pub mod format;
pub mod migrate;
pub mod new;
pub mod schema;
pub mod telemetry;
pub mod update;
pub mod validate;
pub mod version;

use crate::Exit;
use crate::cli::{Command, Global};

pub fn dispatch(global: &Global, command: Command) -> Exit {
    match command {
        Command::Version(args) => version::run(global, &args),
        Command::Completions(args) => completions::run(global, &args),

        // Written in the order of §16.1; each becomes a `mod` line above as it
        // lands.
        Command::New(args) => new::run(global, &args),
        Command::Dev(_) => pending("dev"),
        Command::Build(args) => build::run(global, &args),
        Command::Validate(args) => validate::run(global, &args),
        Command::Format(args) => format::run(global, &args),
        Command::Verify(_) => pending("verify"),
        Command::BrokenLinks(_) => pending("broken-links"),
        Command::Test(_) => pending("test"),
        Command::Score(_) => pending("score"),
        Command::Export(_) => pending("export"),
        Command::Serve(_) => pending("serve"),
        Command::Search(_) => pending("search"),
        Command::Schema(args) => schema::run(global, &args),
        Command::MigrateConfig(args) => migrate::run(global, &args),
        Command::Theme(_) => pending("theme"),
        Command::Update(args) => update::run(global, &args),
        Command::Telemetry(command) => telemetry::run(global, &command),
        Command::Doctor(args) => doctor::run(global, &args),
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
