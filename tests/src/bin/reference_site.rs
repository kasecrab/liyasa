//! Writes the reference site to a directory for the e2e suite.
//!
//! ```sh
//! cargo run -p liyasa-tests --bin reference-site            # target/reference-site
//! cargo run -p liyasa-tests --bin reference-site -- out/
//! ```

use std::fs;
use std::path::{Path, PathBuf};

use liyasa_tests::site;

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let out = PathBuf::from(
        std::env::args()
            .nth(1)
            .unwrap_or_else(|| "target/reference-site".to_owned()),
    );
    let built = site::build()?;

    write(&out.join("_liyasa/theme.css"), &built.stylesheet)?;
    write(&out.join("_liyasa/base.js"), &built.script)?;
    write(&out.join("_liyasa/reader.js"), &built.reader)?;
    write(&out.join("_liyasa/measure.js"), site::MEASURE)?;

    for page in &built.pages {
        let route = page.route.trim_matches('/');
        let directory = if route.is_empty() {
            out.clone()
        } else {
            out.join(route)
        };
        write(&directory.join("index.html"), &page.html)?;
        // The Markdown twin every page carries (RX-60), which is also what the
        // conversion ratio counts.
        let markdown = if route.is_empty() {
            out.join("index.md")
        } else {
            out.join(format!("{route}.md"))
        };
        write(&markdown, &page.markdown)?;
        println!("{} ({} bytes)", page.route, page.html.len());
    }
    println!("written to {}", out.display());
    Ok(())
}

fn write(path: &Path, contents: &str) -> std::io::Result<()> {
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent)?;
    }
    fs::write(path, contents)
}
