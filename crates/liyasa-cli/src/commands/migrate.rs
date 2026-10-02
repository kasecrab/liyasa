//! CLI-14: `liyasa migrate-config`.

use liyasa_config::vfs::OsVfs;
use liyasa_core::source_map::SourceMap;
use liyasa_core::vfs::VfsPath;

use crate::Exit;
use crate::cli::{Global, MigrateConfig};
use crate::ctx;
use crate::diag::Printer;

pub fn run(global: &Global, args: &MigrateConfig) -> Exit {
    let format = global.resolve(crate::cli::Format::Text);
    let cwd = ctx::cwd();
    let project = match ctx::locate(global, &cwd) {
        Ok(project) => project,
        Err(diagnostic) => {
            ctx::report(global, format, *diagnostic);
            return Exit::Errors;
        }
    };

    let vfs = OsVfs::new(&project.root);
    let mut sources = SourceMap::new();
    let load = liyasa_config::load(
        &vfs,
        &mut sources,
        &liyasa_config::Options {
            env: None,
            ..Default::default()
        },
    );
    let printer = Printer::new(format, ctx::use_color(global));

    // RFC 0110: a v0 config always fails to load, because a v0 config is not a
    // v1 config — `E0102` for the schema itself and one more for every key
    // whose shape changed, which is the list this command exists to fix. The
    // gate is right for `build` and `validate` and wrong only here.
    //
    // `E0101` still refuses: a file that is not JSON leaves nothing to
    // migrate.
    let mut ignored = liyasa_core::Diagnostics::new();
    let declared = liyasa_config::schema::declared_version(&load.value, &load.spans, &mut ignored);
    let older =
        declared.is_some_and(|version| version < liyasa_config::schema::CONFIG_SCHEMA_VERSION);
    let unparseable = load
        .diagnostics
        .iter()
        .any(|diagnostic| diagnostic.code == liyasa_core::diagnostics::code::E0101);
    if unparseable || (load.diagnostics.has_errors() && !older) {
        printer.emit(&load.diagnostics, &sources);
        return Exit::Errors;
    }

    // The migration reads the FILE, not the loaded view. `load` strips every
    // key the v1 schema does not know (`schema::without`), which on a v0
    // config is every key the migration exists to move: migrating `load.value`
    // rewrote three keys of the twelve the library's own golden has, and
    // turned `analytics.plausible` into an empty `integrations` object.
    // Reading the file also keeps `--env` overlays out of the base config,
    // which is what a caller means by migrating `liyasa.json` (RFC 0915).
    let (raw, spans) = match raw_config(&project.config, &mut sources) {
        Ok(parsed) => parsed,
        Err(diagnostic) => {
            printer.emit(&std::iter::once(*diagnostic).collect(), &sources);
            return Exit::Errors;
        }
    };
    let migrated = liyasa_config::migrate::migrate(&raw, &spans);
    if migrated.diagnostics.has_errors() {
        printer.emit(&migrated.diagnostics, &sources);
        return Exit::Errors;
    }

    let write = args.write && !global.dry_run;
    if global.json {
        let changes: Vec<serde_json::Value> = migrated
            .changes
            .iter()
            .map(|change| {
                serde_json::json!({
                    "from": change.from,
                    "to": change.to,
                    "note": change.note,
                })
            })
            .collect();
        println!(
            "{}",
            serde_json::json!({
                "summary": liyasa_config::migrate::summary(&migrated),
                "changes": changes,
                "written": write,
                "config": migrated.value,
            })
        );
    } else {
        println!("{}", liyasa_config::migrate::summary(&migrated));
        for change in &migrated.changes {
            if change.to.is_empty() {
                println!("  dropped {} ({})", change.from, change.note);
            } else {
                println!("  {} -> {} ({})", change.from, change.to, change.note);
            }
        }
        if !write {
            println!();
            print!("{}", migrated.json);
        }
    }

    if write {
        if let Err(error) = std::fs::write(&project.config, &migrated.json) {
            eprintln!("could not write {}: {error}", project.config.display());
            return Exit::Errors;
        }
        if !global.quiet {
            println!("wrote {}", ctx::display_relative(&project.config, &cwd));
        }
    } else if args.write && global.dry_run && !global.quiet {
        println!(
            "(--dry-run: {} was not written)",
            ctx::display_relative(&project.config, &cwd)
        );
    }

    Exit::Success
}

/// The config file as written, with its spans: the migration's input.
fn raw_config(
    path: &std::path::Path,
    sources: &mut SourceMap,
) -> Result<(serde_json::Value, liyasa_config::json::SpanIndex), Box<liyasa_core::Diagnostic>> {
    let text = std::fs::read_to_string(path).map_err(|error| {
        Box::new(liyasa_core::Diagnostic::new(
            liyasa_core::diagnostics::code::E0002,
            format!("`{}` could not be read: {error}", path.display()),
        ))
    })?;
    let value = serde_json::from_str(&text).map_err(|error| {
        Box::new(liyasa_core::Diagnostic::new(
            liyasa_core::diagnostics::code::E0101,
            format!("`{}` is not valid JSON: {error}", path.display()),
        ))
    })?;
    let source = sources.intern(
        VfsPath::new(path.file_name().map_or_else(
            || liyasa_config::load::CONFIG_FILE.to_owned(),
            |name| name.to_string_lossy().into_owned(),
        )),
        std::sync::Arc::from(text.as_str()),
    );
    let spans = liyasa_config::json::SpanIndex::scan(source, &text);
    Ok((value, spans))
}
