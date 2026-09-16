//! API-30: every generator is a template over the same request, run over the
//! whole corpus and compared with a golden file.
//!
//! Run with `UPDATE_GOLDEN=1` to rewrite the golden files after changing a
//! template; the diff is the review.

use std::path::PathBuf;

use liyasa_openapi::codegen::{GENERATORS, Registry, corpus};
use liyasa_openapi::sample::{Options, Request};
use liyasa_openapi::{Spec, load};

fn spec() -> Spec {
    let loaded =
        load::from_bytes("api", "corpus.yaml", corpus::SPEC.as_bytes()).expect("the corpus loads");
    assert!(
        !loaded.diagnostics.has_errors(),
        "{:?}",
        loaded.diagnostics.as_slice()
    );
    loaded.spec
}

fn golden(language: &str, case: &str) -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("tests/golden/codegen")
        .join(language)
        .join(format!("{case}.txt"))
}

fn check(language: &str) {
    let spec = spec();
    let registry = Registry::new();
    let mut mismatched = Vec::new();

    for case in corpus::CASES {
        let operation = spec
            .by_operation_id(case)
            .unwrap_or_else(|| panic!("the corpus has no operation `{case}`"));
        let request = Request::build(&spec, &operation, &Options::default());
        let sample = registry
            .render(language, &request)
            .unwrap_or_else(|error| panic!("`{language}` on `{case}`: {error}"));

        let path = golden(language, case);
        if std::env::var_os("UPDATE_GOLDEN").is_some() {
            if let Some(parent) = path.parent() {
                std::fs::create_dir_all(parent).expect("the golden directory is writable");
            }
            std::fs::write(&path, format!("{}\n", sample.source)).expect("the golden is writable");
            continue;
        }
        let expected = std::fs::read_to_string(&path).unwrap_or_else(|_| {
            panic!("no golden at {}; run with UPDATE_GOLDEN=1", path.display())
        });
        if expected.trim_end() != sample.source.trim_end() {
            mismatched.push(format!(
                "--- {language}/{case} ---\nexpected:\n{}\n\ngot:\n{}",
                expected.trim_end(),
                sample.source
            ));
        }
    }
    assert!(mismatched.is_empty(), "{}", mismatched.join("\n\n"));
}

#[test]
fn curl() {
    check("curl");
}

#[test]
fn javascript() {
    check("javascript");
}

#[test]
fn python() {
    check("python");
}

#[test]
fn go() {
    check("go");
}

#[test]
fn every_other_generator_renders_the_whole_corpus() {
    for generator in GENERATORS {
        if matches!(generator.id, "curl" | "javascript" | "python" | "go") {
            continue;
        }
        check(generator.id);
    }
}
