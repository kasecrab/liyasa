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
            ctx::report(global, format, diagnostic);
            return Exit::Errors;
        }
    };

    let vfs = OsVfs::new(&project.root);
    let mut sources = SourceMap::new();
    let load = liyasa_config::load(
        &vfs,
        &mut sources,
        &liyasa_config::Options {
            root: VfsPath::new(""),
            env: None,
        },
    );
    let printer = Printer::new(format, ctx::use_color(global));
    if load.diagnostics.has_errors() {
        printer.emit(&load.diagnostics, &sources);
        return Exit::Errors;
    }

    let migrated = liyasa_config::migrate::migrate(&load.value, &load.spans);
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
