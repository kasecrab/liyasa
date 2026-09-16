//! The `liyasa` command-line interface (PRD §16).
//!
//! The binary is a thin shell over the engine crates: it parses arguments,
//! finds the project, calls one library function, renders whatever diagnostics
//! come back (CLI-30), and returns the documented exit code (CLI-31). Behaviour
//! that belongs to the content lives in the crate that owns it, never here.

pub mod cli;
pub mod commands;
pub mod ctx;
pub mod diag;
pub mod exit;
pub mod git;

pub use exit::Exit;

/// Parses `argv` and runs the command it names.
///
/// A parse failure is CLI-31's exit 2 and is printed by clap itself; `--help`
/// and `--version` are parse "failures" in clap's model that exit 0.
pub fn run<I, T>(argv: I) -> Exit
where
    I: IntoIterator<Item = T>,
    T: Into<std::ffi::OsString> + Clone,
{
    use clap::Parser;

    match cli::Cli::try_parse_from(argv) {
        Ok(parsed) => commands::dispatch(&parsed.global, parsed.command),
        Err(error) => {
            let is_usage_error = error.use_stderr();
            let _ = error.print();
            if is_usage_error {
                Exit::Usage
            } else {
                Exit::Success
            }
        }
    }
}
