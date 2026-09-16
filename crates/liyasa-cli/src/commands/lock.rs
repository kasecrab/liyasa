//! CLI-33: `liyasa lock update|check`, and the `--locked` predicate that
//! `liyasa build` uses.

use std::path::Path;

use liyasa_core::diagnostics::{Diagnostic, code};

use crate::Exit;
use crate::cli::{Global, Lock as Which};
use crate::{ctx, lock};

pub fn run(global: &Global, which: &Which) -> Exit {
    let format = global.resolve(crate::cli::Format::Text);
    let cwd = ctx::cwd();
    let project = match ctx::locate(global, &cwd) {
        Ok(project) => project,
        Err(diagnostic) => {
            ctx::report(global, format, *diagnostic);
            return Exit::Errors;
        }
    };

    let path = project.root.join(lock::LOCK_FILE);
    let config = read_config(&project.config);
    let fresh = lock::compute(&config);

    let current = match lock::read(&path) {
        Ok(current) => current,
        Err(failure) => {
            ctx::report(
                global,
                format,
                Diagnostic::new(failure.code(), failure.message()),
            );
            return Exit::Errors;
        }
    };

    let changes = lock::changes(current.as_ref(), &fresh);

    match which {
        Which::Check => {
            if global.json {
                println!(
                    "{}",
                    serde_json::json!({ "locked": changes.is_empty(), "changes": changes })
                );
            } else if changes.is_empty() {
                println!("`{}` is up to date", lock::LOCK_FILE);
            } else {
                println!("`{}` would change:", lock::LOCK_FILE);
                for change in &changes {
                    println!("  {change}");
                }
            }
            if changes.is_empty() {
                Exit::Success
            } else {
                Exit::Errors
            }
        }
        Which::Update => {
            if changes.is_empty() {
                if !global.quiet {
                    println!("`{}` is already up to date", lock::LOCK_FILE);
                }
                return Exit::Success;
            }
            if global.dry_run {
                println!("lock plan (--dry-run; nothing was written)");
                for change in &changes {
                    println!("  {change}");
                }
                return Exit::Success;
            }
            match lock::write(&path, &fresh) {
                Ok(()) => {
                    if !global.quiet {
                        println!("wrote {}", ctx::display_relative(&path, &cwd));
                        for change in &changes {
                            println!("  {change}");
                        }
                    }
                    Exit::Success
                }
                Err(error) => {
                    ctx::report(
                        global,
                        format,
                        Diagnostic::new(
                            code::E0002,
                            format!("could not write `{}`: {error}", path.display()),
                        ),
                    );
                    Exit::Errors
                }
            }
        }
    }
}

/// `build --locked`: refuse to run when the lock would have to change, and
/// write it when there is none yet and the build is allowed to.
///
/// Returns the diagnostic that stops the build, or `None` to carry on.
pub fn enforce(root: &Path, config_path: &Path, locked: bool) -> Option<Diagnostic> {
    let path = root.join(lock::LOCK_FILE);
    let config = read_config(config_path);
    let fresh = lock::compute(&config);

    let current = match lock::read(&path) {
        Ok(current) => current,
        Err(failure) => return Some(Diagnostic::new(failure.code(), failure.message())),
    };
    let changes = lock::changes(current.as_ref(), &fresh);
    if changes.is_empty() {
        return None;
    }

    if locked {
        return Some(
            Diagnostic::new(
                code::E0010,
                format!(
                    "`--locked` was given and `{}` would change: {}",
                    lock::LOCK_FILE,
                    changes.join("; ")
                ),
            )
            .help("Run `liyasa lock update` and commit the result, or drop `--locked`."),
        );
    }

    // §34.12: a missing lock is generated on first build. An existing one that
    // has drifted is rewritten, because the build is the thing that knows what
    // the project resolves to.
    let _ = lock::write(&path, &fresh);
    None
}

fn read_config(path: &Path) -> serde_json::Value {
    std::fs::read_to_string(path)
        .ok()
        .and_then(|text| serde_json::from_str(&text).ok())
        .unwrap_or(serde_json::Value::Null)
}
