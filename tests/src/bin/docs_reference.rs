//! Writes the generated half of `docs/` (NFR-70).
//!
//! ```sh
//! cargo run -p liyasa-tests --bin docs-reference
//! ```
//!
//! `tests/docs/nfr_70.rs` fails when what is on disk differs from what this
//! writes, so the generated pages cannot drift from the code registry, the
//! schemas, the component registry, or the host matrix.
//!
//! `--pins` instead rewrites `tests/pins/undocumented-codes.txt`. It is a
//! separate mode on purpose: the pin is a ratchet, and a plain run rewriting it
//! would let a code arrive with no body and no failure, which is the whole
//! thing the pin exists to catch.

use std::fs;

use liyasa_tests::docs::generate;

fn main() -> Result<(), Box<dyn std::error::Error>> {
    if std::env::args().any(|arg| arg == "--pins") {
        return pins();
    }
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

    println!("{written} file(s) written");
    Ok(())
}

/// Rewrites the undocumented-codes pin from the notes on disk.
///
/// Deliberate rather than automatic. Deleting one line by hand is the normal
/// way an entry leaves, and it is what keeps two packages documenting different
/// codes out of each other's way; this is for the rarer case where the set
/// moves by more than a line or two.
fn pins() -> Result<(), Box<dyn std::error::Error>> {
    let path = generate::repository().join(generate::UNDOCUMENTED_PINS);
    let current = fs::read_to_string(&path)?;
    let header: String = current
        .lines()
        .take_while(|line| line.starts_with('#'))
        .map(|line| format!("{line}\n"))
        .collect();
    let mut body = header;
    for code in generate::undocumented_codes() {
        body.push_str(&code);
        body.push('\n');
    }
    if body == current {
        println!("pins: already current");
        return Ok(());
    }
    fs::write(&path, &body)?;
    println!(
        "pins: rewrote {} — commit the diff",
        generate::UNDOCUMENTED_PINS
    );
    Ok(())
}
