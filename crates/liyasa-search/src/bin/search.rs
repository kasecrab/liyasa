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

use liyasa_search::cli;
use liyasa_search::idx::Index;

fn main() -> ExitCode {
    let invocation = match cli::parse_arguments(std::env::args().skip(1)) {
        Ok(invocation) => invocation,
        Err(message) => {
            eprintln!("{message}");
            return ExitCode::from(2);
        }
    };
    let (directory, query, options) = (invocation.directory, invocation.query, invocation.options);

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
