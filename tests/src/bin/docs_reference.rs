//! Writes the generated half of `docs/` (NFR-70).
//!
//! ```sh
//! cargo run -p liyasa-tests --bin docs-reference
//! ```
//!
//! `tests/docs/nfr_70.rs` fails when what is on disk differs from what this
//! writes, so the generated pages cannot drift from the code registry, the
//! schemas, the component registry, or the host matrix.

use std::fs;

use liyasa_tests::docs::generate;

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let docs = generate::repository().join("docs");
    let mut written = 0usize;

    for file in generate::files() {
        let path = docs.join(&file.path);
        if let Some(parent) = path.parent() {
            fs::create_dir_all(parent)?;
        }
        if fs::read_to_string(&path).is_ok_and(|current| current == file.text) {
            continue;
        }
        fs::write(&path, &file.text)?;
        written += 1;
        println!("{}", file.path);
    }

    let hosting = generate::hosting_page();
    let page = fs::read_to_string(&hosting)?;
    let spliced = generate::splice_matrix(&page);
    if spliced != page {
        fs::write(&hosting, &spliced)?;
        written += 1;
        println!("guides/hosting.md");
    }

    println!("{written} file(s) written");
    Ok(())
}
