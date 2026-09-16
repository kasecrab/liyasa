//! CLI-05: `liyasa format`. The canonical form is
//! `liyasa_markdown::source::format`'s; what is here is deciding which files to
//! offer it, and what `--check` and `--dry-run` mean.

use std::path::{Path, PathBuf};

use liyasa_markdown::source::{FormatOptions, format_with};

use crate::Exit;
use crate::cli::{Format_, Global};
use crate::ctx;

/// Directories a project never wants formatted, whatever the ignore file says.
const NEVER: &[&str] = &[".git", ".liyasa", "node_modules", "target"];

pub fn run(global: &Global, args: &Format_) -> Exit {
    let format = global.resolve(crate::cli::Format::Text);
    let cwd = ctx::cwd();
    let project = match ctx::locate(global, &cwd) {
        Ok(project) => project,
        Err(diagnostic) => {
            ctx::report(global, format, diagnostic);
            return Exit::Errors;
        }
    };

    let options = FormatOptions {
        directives: args.directives,
    };

    let files = if args.paths.is_empty() {
        walk(&project.root, &output_dir(&project.root))
    } else {
        args.paths
            .iter()
            .flat_map(|path| {
                let path = crate::commands::build::absolute(path, &cwd);
                if path.is_dir() {
                    walk(&path, &output_dir(&project.root))
                } else {
                    vec![path]
                }
            })
            .collect()
    };

    // `--check` reports and changes nothing; `--dry-run` does the same but is
    // not a failure, because it is asking what would happen rather than
    // asserting that nothing needs to.
    let read_only = args.check || global.dry_run;

    let mut changed = Vec::new();
    let mut failed = Vec::new();
    let mut diagnostics = liyasa_core::Diagnostics::new();

    for file in &files {
        let Ok(source) = std::fs::read_to_string(file) else {
            failed.push(file.clone());
            continue;
        };
        match format_with(&source, &options) {
            Ok(formatted) if formatted != source => {
                changed.push(file.clone());
                if !read_only && std::fs::write(file, &formatted).is_err() {
                    failed.push(file.clone());
                }
            }
            Ok(_) => {}
            Err(found) => diagnostics.extend(found),
        }
    }

    // The configuration's canonical form is pretty-printed JSON with a final
    // newline, which is what every other writer of `liyasa.json` produces.
    if let Some(rewritten) = canonical_config(&project.config) {
        changed.push(project.config.clone());
        if !read_only && std::fs::write(&project.config, &rewritten).is_err() {
            failed.push(project.config.clone());
        }
    }

    if !diagnostics.is_empty() {
        let printer = crate::diag::Printer::new(format, ctx::use_color(global));
        printer.emit(&diagnostics, &liyasa_core::source_map::SourceMap::new());
    }

    for file in &failed {
        eprintln!("could not write {}", ctx::display_relative(file, &cwd));
    }

    if !global.quiet {
        report(&changed, &files, &cwd, read_only, args.check);
    }

    if !failed.is_empty() || diagnostics.has_errors() {
        return Exit::Errors;
    }
    if args.check && !changed.is_empty() {
        return Exit::Errors;
    }
    Exit::Success
}

fn report(changed: &[PathBuf], files: &[PathBuf], cwd: &Path, read_only: bool, check: bool) {
    if changed.is_empty() {
        println!(
            "{} file{} already formatted",
            files.len(),
            plural(files.len())
        );
        return;
    }
    for file in changed {
        println!(
            "{} {}",
            if read_only {
                "would rewrite"
            } else {
                "rewrote"
            },
            ctx::display_relative(file, cwd)
        );
    }
    if check {
        println!(
            "{} file{} need formatting",
            changed.len(),
            plural(changed.len())
        );
    }
}

const fn plural(n: usize) -> &'static str {
    if n == 1 { "" } else { "s" }
}

/// The canonical text of `liyasa.json`, or `None` when it is already canonical
/// or cannot be read as JSON at all — an unparseable config is `validate`'s
/// problem to report, not something to rewrite.
fn canonical_config(path: &Path) -> Option<String> {
    let text = std::fs::read_to_string(path).ok()?;
    let value: serde_json::Value = serde_json::from_str(&text).ok()?;
    let mut canonical = serde_json::to_string_pretty(&value).ok()?;
    canonical.push('\n');
    (canonical != text).then_some(canonical)
}

fn output_dir(root: &Path) -> PathBuf {
    let text =
        std::fs::read_to_string(root.join(liyasa_config::load::CONFIG_FILE)).unwrap_or_default();
    let configured = serde_json::from_str::<serde_json::Value>(&text)
        .ok()
        .and_then(|value| {
            value
                .pointer("/build/output")
                .and_then(serde_json::Value::as_str)
                .map(str::to_owned)
        })
        .unwrap_or_else(|| "dist".to_owned());
    root.join(configured)
}

/// Every `.md` under `root`, skipping the output directory, the never-format
/// list, and anything `.liyasaignore` matches.
fn walk(root: &Path, output: &Path) -> Vec<PathBuf> {
    let ignore = std::fs::read_to_string(root.join(liyasa_build::tree::IGNORE_FILE))
        .map(|text| liyasa_markdown::source::route::Ignore::parse(&text))
        .unwrap_or_else(|_| liyasa_markdown::source::route::Ignore::parse(""));

    let mut out = Vec::new();
    let mut stack = vec![root.to_path_buf()];
    while let Some(directory) = stack.pop() {
        let Ok(entries) = std::fs::read_dir(&directory) else {
            continue;
        };
        for entry in entries.flatten() {
            let path = entry.path();
            let name = entry.file_name().to_string_lossy().into_owned();
            if path.is_dir() {
                if NEVER.contains(&name.as_str()) || path == output {
                    continue;
                }
                stack.push(path);
            } else if path.extension().is_some_and(|extension| extension == "md") {
                let relative = path.strip_prefix(root).unwrap_or(&path);
                let vfs = liyasa_core::vfs::VfsPath::new(relative.to_string_lossy());
                if !ignore.matches(&vfs) {
                    out.push(path);
                }
            }
        }
    }
    out.sort();
    out
}
