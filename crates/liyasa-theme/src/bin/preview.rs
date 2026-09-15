//! Writes the preset's review page (THM-01, THM-05).
//!
//! ```sh
//! cargo run -p liyasa-theme --bin preview            # ../../preview/aurora.html
//! cargo run -p liyasa-theme --bin preview -- out.html
//! ```

use std::path::PathBuf;

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let out = std::env::args()
        .nth(1)
        .map_or_else(default_path, PathBuf::from);
    let html = liyasa_theme::preview::page()?;
    if let Some(parent) = out.parent() {
        std::fs::create_dir_all(parent)?;
    }
    std::fs::write(&out, &html)?;
    let shown = std::fs::canonicalize(&out).unwrap_or(out);
    println!("{} ({} KB)", shown.display(), html.len() / 1024);
    Ok(())
}

/// `preview/aurora.html` beside the worktree, which is where the packet's
/// review copy lives; it is never part of the repository.
fn default_path() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../../../preview/aurora.html")
}
