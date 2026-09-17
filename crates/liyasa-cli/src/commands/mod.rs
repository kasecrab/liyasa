//! One module per command, and the table that routes to them.

pub mod build;
pub mod companion;
pub mod completions;
pub mod dev;
pub mod doctor;
pub mod export;
pub mod format;
pub mod lock;
pub mod migrate;
pub mod new;
pub mod schema;
pub mod score;
pub mod search;
pub mod serve;
pub mod telemetry;
pub mod test;
pub mod theme;
pub mod update;
pub mod validate;
pub mod verify;
pub mod version;

use crate::Exit;
use crate::cli::{Command, Global};

pub fn dispatch(global: &Global, command: Command) -> Exit {
    match command {
        Command::Version(args) => version::run(global, &args),
        Command::Completions(args) => completions::run(global, &args),

        Command::New(args) => new::run(global, &args),
        Command::Dev(args) => dev::run(global, &args),
        Command::Build(args) => build::run(global, &args),
        Command::Validate(args) => validate::run(global, &args),
        Command::Format(args) => format::run(global, &args),
        Command::Verify(args) => verify::run(global, &args),
        Command::BrokenLinks(args) => verify::links(global, &args),
        Command::Test(args) => test::run(global, &args),
        Command::Score(args) => score::run(global, &args),
        Command::Export(args) => export::run(global, &args),
        Command::Serve(args) => serve::run(global, &args),
        Command::Search(args) => search::run(global, &args),
        Command::Schema(args) => schema::run(global, &args),
        Command::MigrateConfig(args) => migrate::run(global, &args),
        Command::Theme(which) => theme::run(global, &which),
        Command::Update(args) => update::run(global, &args),
        Command::Telemetry(command) => telemetry::run(global, &command),
        Command::Doctor(args) => doctor::run(global, &args),
        Command::Companion(which) => companion::run(global, &which),
        Command::Lock(which) => lock::run(global, &which),
        Command::Lsp => lsp(global),
        Command::Budgets => budgets(global),
    }
}

/// CLI-25: the language server, speaking the protocol over stdin and stdout.
///
/// A failure is reported on stderr rather than discarded. Stdout carries the
/// protocol and nothing else, so a diagnostic there would corrupt the stream;
/// stderr is where an editor's log looks, and it is the only place a user of a
/// stdio language server can see why it stopped. A bare non-zero exit tells
/// them nothing.
fn lsp(global: &Global) -> Exit {
    match liyasa_lsp::serve_stdio() {
        Ok(()) => Exit::Success,
        Err(error) => {
            crate::ctx::report(
                global,
                global.resolve(crate::cli::Format::Text),
                liyasa_core::Diagnostic::new(
                    liyasa_core::diagnostics::code::E0002,
                    format!("the language server stopped: {error}"),
                ),
            );
            Exit::Errors
        }
    }
}

/// CLI-35's table, for the release job. Always JSON: its only reader is a
/// script.
fn budgets(_global: &Global) -> Exit {
    println!(
        "{}",
        serde_json::to_string_pretty(&crate::budget::as_json()).unwrap_or_else(|_| "[]".to_owned())
    );
    Exit::Success
}

/// The output directory a project's configuration names, resolved against its
/// root. `LIYASA_OUTPUT` and `--output` override it per command.
pub fn output_dir(project: &crate::ctx::Project) -> std::path::PathBuf {
    let configured = std::fs::read_to_string(&project.config)
        .ok()
        .and_then(|text| serde_json::from_str::<serde_json::Value>(&text).ok())
        .and_then(|value| {
            value
                .pointer("/build/output")
                .and_then(serde_json::Value::as_str)
                .map(str::to_owned)
        })
        .unwrap_or_else(|| "dist".to_owned());
    project.root.join(configured)
}
