//! CLI-01 and MIG-10: `liyasa new`.
//!
//! Interactive when there is a terminal to be interactive with, and silent
//! under `--yes` or when stdin is not a terminal — a scaffold that blocks on a
//! prompt inside a CI job is a scaffold nobody can script.

use std::io::{IsTerminal, Write};
use std::path::{Path, PathBuf};

use liyasa_core::diagnostics::{Diagnostic, code};

use crate::Exit;
use crate::cli::{Global, New};
use crate::{ctx, git, scaffold};

pub fn run(global: &Global, args: &New) -> Exit {
    let format = global.resolve(crate::cli::Format::Text);
    let cwd = ctx::cwd();
    let directory = args.directory.as_ref().map_or_else(
        || cwd.clone(),
        |given| crate::commands::build::absolute(given, &cwd),
    );

    if let Some(template) = &args.template {
        // TODO(rfc-0905): `--template <name|url>` needs a registry and a git
        // clone; neither exists, and inventing a name for a starter nobody
        // has published would be worse than saying so.
        ctx::report(
            global,
            format,
            Diagnostic::new(
                code::E0013,
                format!("no starter template named `{template}`"),
            )
            .help("This release ships one starter. Run `liyasa new` without `--template`."),
        );
        return Exit::Errors;
    }

    if let Some(existing) = occupied(&directory) {
        ctx::report(
            global,
            format,
            Diagnostic::new(
                code::E0012,
                format!(
                    "`{}` already has files in it, starting with `{existing}`",
                    ctx::display_relative(&directory, &cwd)
                ),
            )
            .help("Pick an empty directory, or pass one that does not exist yet."),
        );
        return Exit::Errors;
    }

    let interactive =
        !args.yes && std::io::stdin().is_terminal() && std::io::stdout().is_terminal();
    let options = match answers(args, &directory, interactive) {
        Ok(options) => options,
        Err(diagnostic) => {
            ctx::report(global, format, diagnostic);
            return Exit::Errors;
        }
    };

    let files = scaffold::files(&options);
    // `--git` and `--no-git` override each other; the default is to initialise
    // a repository, because a scaffold nobody has committed is one nobody can
    // undo a change to.
    let wants_git = !args.no_git;

    if global.dry_run {
        println!("new project plan (--dry-run; nothing was written)");
        println!("  directory    {}", ctx::display_relative(&directory, &cwd));
        println!("  name         {}", options.name);
        println!("  preset       {}", options.preset);
        println!("  openapi      {}", yes_no(options.openapi));
        println!("  ci workflow  {}", yes_no(options.ci));
        println!(
            "  git init     {}",
            yes_no(wants_git && git::SystemGit::is_available())
        );
        println!("  files        {}", files.len());
        for (path, _) in &files {
            println!("    {path}");
        }
        return Exit::Success;
    }

    if let Err(error) = std::fs::create_dir_all(&directory) {
        ctx::report(
            global,
            format,
            Diagnostic::new(
                code::E0002,
                format!("could not create `{}`: {error}", directory.display()),
            ),
        );
        return Exit::Errors;
    }

    for (path, body) in &files {
        let full = directory.join(path);
        if let Some(parent) = full.parent() {
            if let Err(error) = std::fs::create_dir_all(parent) {
                ctx::report(
                    global,
                    format,
                    Diagnostic::new(
                        code::E0002,
                        format!("could not create `{}`: {error}", parent.display()),
                    ),
                );
                return Exit::Errors;
            }
        }
        if let Err(error) = std::fs::write(&full, body) {
            ctx::report(
                global,
                format,
                Diagnostic::new(
                    code::E0002,
                    format!("could not write `{}`: {error}", full.display()),
                ),
            );
            return Exit::Errors;
        }
    }

    let repository = wants_git && git::SystemGit::is_available() && git::init(&directory);

    if !global.quiet {
        let here = directory == cwd;
        println!(
            "created {} in {}",
            options.name,
            ctx::display_relative(&directory, &cwd)
        );
        println!("  {} files written", files.len());
        if repository {
            println!("  git repository initialised");
        } else if wants_git {
            println!("  git is not on PATH, so no repository was initialised");
        }
        println!();
        println!("next:");
        if !here {
            println!("  cd {}", ctx::display_relative(&directory, &cwd));
        }
        println!("  liyasa dev");
        println!();
        println!("`checklist.md` lists what to change before the site is yours.");
    }

    Exit::Success
}

/// The answers, from flags, from the person, or from the defaults.
fn answers(
    args: &New,
    directory: &Path,
    interactive: bool,
) -> Result<scaffold::Options, Diagnostic> {
    let mut options = scaffold::Options {
        name: args
            .name
            .clone()
            .unwrap_or_else(|| scaffold::name_from_directory(directory)),
        preset: args
            .preset
            .clone()
            .unwrap_or_else(|| scaffold::DEFAULT_PRESET.to_owned()),
        openapi: !args.no_openapi,
        ci: !args.no_ci,
    };

    if let Some(preset) = &args.preset {
        if !scaffold::PRESETS.contains(&preset.as_str()) {
            return Err(
                Diagnostic::new(code::E0013, format!("`{preset}` is not a theme preset"))
                    .help(format!("Pick one of: {}", scaffold::PRESETS.join(", "))),
            );
        }
    }

    if !interactive {
        return Ok(options);
    }

    options.name = ask("Site name", &options.name);
    options.preset = loop {
        let answer = ask(
            &format!("Theme preset ({})", scaffold::PRESETS.join(", ")),
            &options.preset,
        );
        if scaffold::PRESETS.contains(&answer.as_str()) {
            break answer;
        }
        println!(
            "  not a preset; pick one of: {}",
            scaffold::PRESETS.join(", ")
        );
    };
    if !args.openapi && !args.no_openapi {
        options.openapi = confirm("Include a sample OpenAPI specification", true);
    }
    if !args.ci && !args.no_ci {
        options.ci = confirm("Write a GitHub Actions workflow", true);
    }
    Ok(options)
}

/// One question with a default. An unreadable stdin takes the default rather
/// than looping forever.
fn ask(question: &str, default: &str) -> String {
    print!("{question} [{default}]: ");
    let _ = std::io::stdout().flush();
    let mut answer = String::new();
    if std::io::stdin().read_line(&mut answer).is_err() {
        return default.to_owned();
    }
    let answer = answer.trim();
    if answer.is_empty() {
        default.to_owned()
    } else {
        answer.to_owned()
    }
}

fn confirm(question: &str, default: bool) -> bool {
    let hint = if default { "Y/n" } else { "y/N" };
    let answer = ask(
        &format!("{question} ({hint})"),
        if default { "y" } else { "n" },
    );
    matches!(answer.trim().to_ascii_lowercase().as_str(), "y" | "yes")
}

/// The first entry in `directory` that is not something a scaffold may sit
/// beside. A `.git` a person has already created is fine; a `liyasa.json` is
/// not.
fn occupied(directory: &Path) -> Option<String> {
    let entries = std::fs::read_dir(directory).ok()?;
    for entry in entries.flatten() {
        let name = entry.file_name().to_string_lossy().into_owned();
        if matches!(
            name.as_str(),
            ".git" | ".gitignore" | "LICENSE" | "README.md"
        ) {
            continue;
        }
        return Some(name);
    }
    None
}

const fn yes_no(value: bool) -> &'static str {
    if value { "yes" } else { "no" }
}
