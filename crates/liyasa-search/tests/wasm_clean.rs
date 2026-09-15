//! `idx` must keep compiling for `wasm32-unknown-unknown`
//! (plan/rfcs/0700-idx-inside-search.md).
//!
//! The real gate is
//! `cargo check -p liyasa-search --no-default-features --target wasm32-unknown-unknown`,
//! which needs the target installed. This test is the part CI can always run:
//! it reads the module and asserts the separation a crate boundary would
//! otherwise enforce, so the split into `liyasa-idx` stays a directory move.

use std::path::{Path, PathBuf};

fn idx() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("src/idx")
}

fn sources(directory: &Path) -> Vec<(PathBuf, String)> {
    let mut out = Vec::new();
    let Ok(entries) = std::fs::read_dir(directory) else {
        return out;
    };
    for entry in entries.flatten() {
        let path = entry.path();
        if path.is_dir() {
            out.extend(sources(&path));
        } else if path.extension().is_some_and(|e| e == "rs")
            && let Ok(text) = std::fs::read_to_string(&path)
        {
            out.push((path, text));
        }
    }
    out
}

/// Text outside `//` comments, so a module's prose may name what its code may
/// not reach for.
fn code(text: &str) -> String {
    text.lines()
        .map(|line| match line.find("//") {
            Some(at) => &line[..at],
            None => line,
        })
        .collect::<Vec<_>>()
        .join("\n")
}

#[test]
fn the_browser_format_names_nothing_it_cannot_compile_against() {
    let files = sources(&idx());
    assert!(!files.is_empty(), "the module has sources to check");

    for (path, text) in files {
        let code = code(&text);
        let name = path.display();
        for forbidden in [
            // rayon unconditionally, and a synchronous `Directory` API.
            "tantivy",
            // Threads in a browser need cross-origin isolation, which GitHub
            // Pages cannot set (SRC-05).
            "rayon",
            "std::thread",
            // The worker hands the reader bytes; it never opens anything.
            "std::fs",
            "std::net",
            "std::process",
            "std::time::SystemTime",
        ] {
            assert!(
                !code.contains(forbidden),
                "{name} names `{forbidden}`, which `wasm32-unknown-unknown` has no answer for"
            );
        }
    }
}

#[test]
fn the_server_half_is_behind_its_feature() {
    let lib = std::fs::read_to_string(Path::new(env!("CARGO_MANIFEST_DIR")).join("src/lib.rs"))
        .expect("the crate root is readable");
    let at = lib
        .find("pub mod server;")
        .expect("the server module exists");
    assert!(
        lib[..at].ends_with("#[cfg(feature = \"server\")]\n"),
        "`server` must be feature-gated so `--no-default-features` builds for wasm32"
    );
}

#[test]
fn every_module_the_browser_needs_is_inside_idx() {
    // The split into `liyasa-idx` moves this directory and nothing else, so
    // nothing the reader needs may live outside it. `doc`, `error`, and
    // `glob` are shared and wasm-clean; they move with it or are duplicated.
    for module in [
        "docs.rs",
        "field.rs",
        "index.rs",
        "manifest.rs",
        "postings.rs",
        "query.rs",
        "reader.rs",
        "score.rs",
        "search.rs",
        "snippets.rs",
        "writer.rs",
    ] {
        assert!(
            idx().join(module).exists(),
            "`{module}` is not in `src/idx/`"
        );
    }
    assert!(idx().join("tokenize").is_dir());
}
