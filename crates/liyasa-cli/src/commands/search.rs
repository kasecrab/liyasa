//! CLI-12: `liyasa search "<query>"` against the index in the built site.

use std::collections::BTreeMap;
use std::path::Path;

use liyasa_core::diagnostics::{Diagnostic, code};
use liyasa_search::cli::Options;
use liyasa_search::idx::Index;

use crate::Exit;
use crate::cli::{Global, Search};
use crate::ctx;

/// Where the build writes the browser index (§12.2).
pub const INDEX_DIR: &str = "search-index";

pub fn run(global: &Global, args: &Search) -> Exit {
    let format = global.resolve(crate::cli::Format::Text);
    let cwd = ctx::cwd();

    let directory = match &args.index {
        Some(given) => crate::commands::build::absolute(given, &cwd),
        None => match ctx::locate(global, &cwd) {
            Ok(project) => crate::commands::output_dir(&project).join(INDEX_DIR),
            Err(diagnostic) => {
                ctx::report(global, format, diagnostic);
                return Exit::Errors;
            }
        },
    };

    let files = match read_index(&directory) {
        Ok(files) if !files.is_empty() => files,
        _ => {
            ctx::report(
                global,
                format,
                Diagnostic::new(
                    code::E0016,
                    format!("no search index in `{}`", directory.display()),
                )
                .help("Run `liyasa build` first, or pass `--index <directory>`."),
            );
            return Exit::Errors;
        }
    };

    let index = match Index::open(files) {
        Ok(index) => index,
        Err(error) => {
            ctx::report(
                global,
                format,
                Diagnostic::new(
                    code::E0016,
                    format!("the search index could not be read: {error:?}"),
                ),
            );
            return Exit::Errors;
        }
    };

    let options = Options {
        locale: args.locale.clone(),
        version: args.version_name.clone(),
        tab: args.tab.clone(),
        limit: Some(args.limit),
        json: global.json,
        expect: None,
    };

    match liyasa_search::cli::run(&index, &args.query, &options) {
        Ok(outcome) => {
            print!("{}", outcome.output);
            if outcome.code == 0 {
                Exit::Success
            } else {
                Exit::Errors
            }
        }
        Err(error) => {
            ctx::report(
                global,
                format,
                Diagnostic::new(code::E1004, format!("{error:?}")),
            );
            Exit::Errors
        }
    }
}

/// Every file in the index directory, keyed by name, which is the shape
/// `Index::open` takes.
fn read_index(directory: &Path) -> std::io::Result<BTreeMap<String, Vec<u8>>> {
    let mut files = BTreeMap::new();
    let mut stack = vec![directory.to_path_buf()];
    while let Some(at) = stack.pop() {
        for entry in std::fs::read_dir(&at)?.flatten() {
            let path = entry.path();
            if path.is_dir() {
                stack.push(path);
            } else if let (Ok(bytes), Some(name)) =
                (std::fs::read(&path), relative(directory, &path))
            {
                files.insert(name, bytes);
            }
        }
    }
    Ok(files)
}

fn relative(root: &Path, path: &Path) -> Option<String> {
    path.strip_prefix(root)
        .ok()
        .map(|rest| rest.to_string_lossy().replace('\\', "/"))
}
