//! `liyasa search` before `liyasa-cli` exists (SRC-09).
//!
//! Reads a `search-index/` directory and runs one query against it:
//!
//! ```text
//! cargo run -p liyasa-search --bin search -- ./dist/search-index "rate limits"
//! ```

use std::collections::BTreeMap;
use std::path::Path;
use std::process::ExitCode;

use liyasa_search::cli::{self, Options};
use liyasa_search::idx::Index;

fn main() -> ExitCode {
    let mut arguments = std::env::args().skip(1);
    let (Some(directory), Some(query)) = (arguments.next(), arguments.next()) else {
        eprintln!("usage: search <search-index directory> <query> [--json] [--expect <url>]");
        return ExitCode::from(2);
    };

    let mut options = Options::default();
    let rest: Vec<String> = arguments.collect();
    let mut at = 0;
    while at < rest.len() {
        match rest[at].as_str() {
            "--json" => options.json = true,
            "--expect" => {
                at += 1;
                options.expect = rest.get(at).cloned();
            }
            "--locale" => {
                at += 1;
                options.locale = rest.get(at).cloned();
            }
            "--limit" => {
                at += 1;
                options.limit = rest.get(at).and_then(|value| value.parse().ok());
            }
            other => {
                eprintln!("unknown option `{other}`");
                return ExitCode::from(2);
            }
        }
        at += 1;
    }

    let files = match read(Path::new(&directory)) {
        Ok(files) => files,
        Err(error) => {
            eprintln!("{directory}: {error}");
            return ExitCode::from(2);
        }
    };
    let index = match Index::open(files) {
        Ok(index) => index,
        Err(error) => {
            let diagnostic = error.diagnostic();
            eprintln!("{}: {}", diagnostic.code, diagnostic.message);
            return ExitCode::from(2);
        }
    };
    match cli::run(&index, &query, &options) {
        Ok(outcome) => {
            print!("{}", outcome.output);
            ExitCode::from(outcome.code as u8)
        }
        Err(error) => {
            let diagnostic = error.diagnostic();
            eprintln!("{}: {}", diagnostic.code, diagnostic.message);
            ExitCode::from(2)
        }
    }
}

fn read(directory: &Path) -> std::io::Result<BTreeMap<String, Vec<u8>>> {
    let mut files = BTreeMap::new();
    for entry in std::fs::read_dir(directory)? {
        let entry = entry?;
        if entry.file_type()?.is_file()
            && let Some(name) = entry.file_name().to_str()
        {
            files.insert(name.to_owned(), std::fs::read(entry.path())?);
        }
    }
    Ok(files)
}
