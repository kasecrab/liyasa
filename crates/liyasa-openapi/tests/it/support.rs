//! Shared fixtures for the endpoint-page tests.
//!
//! One module of the crate's single test binary; not every test uses every
//! helper, so unused ones are allowed.
#![allow(dead_code)]

use std::path::PathBuf;

use liyasa_openapi::codegen::Registry;
use liyasa_openapi::page::{BuildOptions, Page};
use liyasa_openapi::{Spec, load};

pub fn spec(source: &str) -> Spec {
    let loaded = load::from_bytes("api", "api.yaml", source.as_bytes()).expect("the spec loads");
    assert!(
        !loaded.diagnostics.has_errors(),
        "{:?}",
        loaded.diagnostics.as_slice()
    );
    loaded.spec
}

pub fn page(spec: &Spec, operation_id: &str) -> Page {
    let registry = Registry::new();
    let operation = spec
        .by_operation_id(operation_id)
        .unwrap_or_else(|| panic!("the spec has no operation `{operation_id}`"));
    Page::build(
        spec,
        &operation,
        &registry,
        &BuildOptions {
            route: format!("/api-reference/{operation_id}"),
            ..BuildOptions::default()
        },
    )
}

/// Compares against a golden file, or writes it when `UPDATE_GOLDEN` is set.
pub fn golden(name: &str, got: &str) {
    let path = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("tests/golden/pages")
        .join(format!("{name}.md"));
    if std::env::var_os("UPDATE_GOLDEN").is_some() {
        if let Some(parent) = path.parent() {
            std::fs::create_dir_all(parent).expect("the golden directory is writable");
        }
        std::fs::write(&path, format!("{got}\n")).expect("the golden is writable");
        return;
    }
    let expected = std::fs::read_to_string(&path)
        .unwrap_or_else(|_| panic!("no golden at {}; run with UPDATE_GOLDEN=1", path.display()));
    assert_eq!(
        expected.trim_end(),
        got.trim_end(),
        "{} is out of date",
        path.display()
    );
}
