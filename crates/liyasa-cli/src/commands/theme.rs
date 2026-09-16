//! CLI-17: `liyasa theme eject|diff|tokens` (THM-23).

use std::path::Path;

use liyasa_core::diagnostics::{Diagnostic, code};
use liyasa_theme::theme::{Change, Theme};
use liyasa_theme::tokens::{Scheme, Tokens};

use crate::Exit;
use crate::cli::{Global, Theme as Which, ThemeDiff, ThemeEject, ThemeTokens};
use crate::ctx;

pub fn run(global: &Global, which: &Which) -> Exit {
    let format = global.resolve(crate::cli::Format::Text);
    let cwd = ctx::cwd();
    let project = match ctx::locate(global, &cwd) {
        Ok(project) => project,
        Err(diagnostic) => {
            ctx::report(global, format, diagnostic);
            return Exit::Errors;
        }
    };

    match which {
        Which::Eject(args) => eject(global, &project, args),
        Which::Diff(args) => diff(global, &project, args),
        Which::Tokens(args) => tokens(global, &project, args),
    }
}

/// THM-23: copy a default partial into `theme/partials/` so it can be edited.
fn eject(global: &Global, project: &ctx::Project, args: &ThemeEject) -> Exit {
    let format = global.resolve(crate::cli::Format::Text);

    let Some(name) = &args.partial else {
        list(global);
        return Exit::Success;
    };

    let Some((relative, source)) = Theme::eject(name) else {
        ctx::report(
            global,
            format,
            Diagnostic::new(code::E0013, format!("`{name}` is not a partial"))
                .help("Run `liyasa theme eject` with no argument to see the list."),
        );
        return Exit::Errors;
    };

    let target = project.root.join(&relative);
    if target.exists() {
        ctx::report(
            global,
            format,
            Diagnostic::new(code::E0012, format!("`{relative}` already exists"))
                .help("Delete it first, or run `liyasa theme diff` to see what it changed."),
        );
        return Exit::Errors;
    }

    if global.dry_run {
        println!("would write {relative} ({} bytes)", source.len());
        return Exit::Success;
    }

    if let Some(parent) = target.parent() {
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
    match std::fs::write(&target, source) {
        Ok(()) => {
            if !global.quiet {
                println!("wrote {relative}");
                println!("It overrides the default until you delete it.");
            }
            Exit::Success
        }
        Err(error) => {
            ctx::report(
                global,
                format,
                Diagnostic::new(
                    code::E0002,
                    format!("could not write `{relative}`: {error}"),
                ),
            );
            Exit::Errors
        }
    }
}

fn list(global: &Global) {
    let names = Theme::partials();
    if global.json {
        println!("{}", serde_json::json!({ "partials": names }));
    } else {
        println!("partials this release can eject:");
        for name in names {
            println!("  {name}");
        }
    }
}

/// THM-23: what an override changed relative to the current default, which is
/// the question after an upgrade.
fn diff(global: &Global, project: &ctx::Project, args: &ThemeDiff) -> Exit {
    let wanted: Vec<&'static str> = match &args.partial {
        Some(name) => Theme::partials()
            .into_iter()
            .filter(|partial| partial == name)
            .collect(),
        None => Theme::partials(),
    };

    let mut any = false;
    let mut report = Vec::new();
    for name in wanted {
        let Some((relative, source)) = Theme::eject(name) else {
            continue;
        };
        let path = project.root.join(&relative);
        let Ok(overridden) = std::fs::read_to_string(&path) else {
            continue;
        };
        let changes = liyasa_theme::theme::diff(source, &overridden);
        if !liyasa_theme::theme::is_changed(&changes) {
            continue;
        }
        any = true;
        report.push((name, relative, changes));
    }

    if global.json {
        let rows: Vec<serde_json::Value> = report
            .iter()
            .map(|(name, relative, changes)| {
                serde_json::json!({
                    "partial": name,
                    "path": relative,
                    "added": count(changes, |c| matches!(c, Change::Added(_))),
                    "removed": count(changes, |c| matches!(c, Change::Removed(_))),
                })
            })
            .collect();
        println!("{}", serde_json::json!({ "overrides": rows }));
        return Exit::Success;
    }

    if !any {
        println!("no partial in this project differs from its default");
        return Exit::Success;
    }

    for (name, relative, changes) in &report {
        println!("--- {name} (default)");
        println!("+++ {relative}");
        for change in changes {
            match change {
                Change::Kept(line) => println!("  {line}"),
                Change::Added(line) => println!("+ {line}"),
                Change::Removed(line) => println!("- {line}"),
            }
        }
        println!();
    }
    Exit::Success
}

fn count(changes: &[Change], predicate: impl Fn(&Change) -> bool) -> usize {
    changes.iter().filter(|change| predicate(change)).count()
}

/// The resolved token set: what the theme actually emits for this project,
/// after the preset, the configured colours, and `theme/tokens.css`.
fn tokens(global: &Global, project: &ctx::Project, _args: &ThemeTokens) -> Exit {
    let format = global.resolve(crate::cli::Format::Text);
    let config = read_theme_config(&project.config);
    let (mut resolved, diagnostics) = Tokens::from_config(&config);

    if let Ok(css) = std::fs::read_to_string(project.root.join("theme/tokens.css")) {
        let more = resolved.with_overrides(&css);
        if more.has_errors() {
            let printer = crate::diag::Printer::new(format, ctx::use_color(global));
            printer.emit(&more, &liyasa_core::source_map::SourceMap::new());
            return Exit::Errors;
        }
    }

    if diagnostics.has_errors() {
        let printer = crate::diag::Printer::new(format, ctx::use_color(global));
        printer.emit(&diagnostics, &liyasa_core::source_map::SourceMap::new());
        return Exit::Errors;
    }

    if global.json {
        let mut rows = serde_json::Map::new();
        for name in resolved.names() {
            rows.insert(
                name.to_owned(),
                serde_json::json!({
                    "light": resolved.get(name, Scheme::Light),
                    "dark": resolved.get(name, Scheme::Dark),
                }),
            );
        }
        println!(
            "{}",
            serde_json::to_string_pretty(&serde_json::Value::Object(rows))
                .unwrap_or_else(|_| "{}".to_owned())
        );
    } else {
        print!("{}", resolved.to_css());
    }
    Exit::Success
}

fn read_theme_config(path: &Path) -> liyasa_theme::config::ThemeConfig {
    std::fs::read_to_string(path)
        .ok()
        .and_then(|text| serde_json::from_str::<serde_json::Value>(&text).ok())
        .and_then(|value| value.get("theme").cloned())
        .and_then(|theme| serde_json::from_value(theme).ok())
        .unwrap_or_default()
}
