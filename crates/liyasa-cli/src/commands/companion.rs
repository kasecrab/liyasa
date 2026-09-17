//! §6.12: `liyasa companion install|status|remove`.
//!
//! The runtime is a pinned headless browser. The client to fetch one exists
//! now (`crate::net`); what is missing is unpacking it — every published build
//! is a zip, and the PRD's dependency table has no compression crate, the same
//! gap that makes a release artifact a bare binary rather than an archive. So
//! `install` still takes a local unpacked directory, which is what HOST-08's
//! air-gapped installation needs anyway. TODO(rfc-0900).

use liyasa_core::diagnostics::{Diagnostic, code};

use crate::Exit;
use crate::cli::{Companion as Which, CompanionInstall, Global};
use crate::{ctx, home};

pub fn run(global: &Global, which: &Which) -> Exit {
    match which {
        Which::Status => status(global),
        Which::Install(args) => install(global, args),
        Which::Remove => remove(global),
    }
}

fn status(global: &Global) -> Exit {
    let directory = home::companion_dir();

    // What would actually be used, which may be a browser this command did not
    // install.
    if home::companion_version().is_none()
        && let Some(browser) = crate::browser::find()
    {
        if global.json {
            println!(
                "{}",
                serde_json::json!({
                    "installed": false,
                    "usable": true,
                    "version": browser.version,
                    "source": browser.source.name(),
                    "path": browser.path.display().to_string(),
                })
            );
        } else {
            println!(
                "no companion runtime installed, but the {} can be used: {} ({})",
                browser.source.name(),
                browser.version,
                browser.path.display()
            );
            println!("`liyasa.lock` cannot pin it, so a render is not reproducible elsewhere.");
        }
        return Exit::Success;
    }

    match home::companion_version() {
        Some(version) => {
            if global.json {
                println!(
                    "{}",
                    serde_json::json!({
                        "installed": true,
                        "version": version,
                        "path": directory.display().to_string(),
                    })
                );
            } else {
                println!("companion runtime {version} in {}", directory.display());
            }
            Exit::Success
        }
        None => {
            if global.json {
                println!(
                    "{}",
                    serde_json::json!({
                        "installed": false,
                        "path": directory.display().to_string(),
                    })
                );
            } else {
                println!("no companion runtime installed");
                println!("Without it: PDF export, Lighthouse budgets, axe checks, screenshot");
                println!("sources, and pre-rendered Mermaid are unavailable.");
            }
            Exit::Success
        }
    }
}

fn install(global: &Global, args: &CompanionInstall) -> Exit {
    let format = global.resolve(crate::cli::Format::Text);
    let Some(source) = &args.source else {
        ctx::report(
            global,
            format,
            Diagnostic::new(
                code::E0006,
                "this build cannot unpack a downloaded companion runtime".to_owned(),
            )
            .help("Pass `--source <directory>` holding an unpacked runtime, or set `LIYASA_COMPANION_SOURCE`."),
        );
        return Exit::Network;
    };

    let Some(path) = crate::update::source_path(source) else {
        ctx::report(
            global,
            format,
            Diagnostic::new(
                code::E0006,
                format!("`{source}` is a published archive, and this build cannot unpack one"),
            )
            .help("Unpack it yourself and point `--source` at the directory."),
        );
        return Exit::Network;
    };

    if !path.is_dir() {
        ctx::report(
            global,
            format,
            Diagnostic::new(
                code::E0002,
                format!("`{}` is not a directory", path.display()),
            )
            .help("Point `--source` at an unpacked runtime directory."),
        );
        return Exit::Errors;
    }

    let target = home::companion_dir();
    if global.dry_run {
        println!("companion install plan (--dry-run; nothing was written)");
        println!("  from  {}", path.display());
        println!("  to    {}", target.display());
        return Exit::Success;
    }

    if let Err(error) = copy_tree(&path, &target) {
        ctx::report(
            global,
            format,
            Diagnostic::new(
                code::E0002,
                format!("could not install into `{}`: {error}", target.display()),
            ),
        );
        return Exit::Errors;
    }

    let version = home::companion_version().unwrap_or_else(|| "unknown".to_owned());
    if !global.quiet {
        println!(
            "installed companion runtime {version} in {}",
            target.display()
        );
        if version == "unknown" {
            println!("The source has no `version` file, so `liyasa.lock` cannot pin it.");
        }
    }
    Exit::Success
}

fn remove(global: &Global) -> Exit {
    let directory = home::companion_dir();
    if !directory.exists() {
        println!("no companion runtime installed");
        return Exit::Success;
    }
    if global.dry_run {
        println!("would remove {}", directory.display());
        return Exit::Success;
    }
    match std::fs::remove_dir_all(&directory) {
        Ok(()) => {
            if !global.quiet {
                println!("removed {}", directory.display());
            }
            Exit::Success
        }
        Err(error) => {
            eprintln!("could not remove {}: {error}", directory.display());
            Exit::Errors
        }
    }
}

fn copy_tree(from: &std::path::Path, to: &std::path::Path) -> std::io::Result<()> {
    std::fs::create_dir_all(to)?;
    for entry in std::fs::read_dir(from)?.flatten() {
        let source = entry.path();
        let target = to.join(entry.file_name());
        if source.is_dir() {
            copy_tree(&source, &target)?;
        } else {
            std::fs::copy(&source, &target)?;
        }
    }
    Ok(())
}
