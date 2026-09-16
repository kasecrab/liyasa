//! MIG-22: the Liyasa docs are built with Liyasa, and the build is clean.
//!
//! The acceptance criterion names the release pipeline: build, verify, deploy,
//! spec checks, and Lighthouse. The build and the spec checks run here; deploy
//! and Lighthouse need a host and a browser, and are the pipeline's own steps.

use liyasa_tests::docs::Docs;

#[test]
fn the_docs_build_without_an_error() {
    let docs = Docs::build("clean");
    assert_eq!(docs.errors(), Vec::<String>::new());
}

#[test]
fn the_home_page_is_served_as_html_and_as_markdown() {
    let docs = Docs::build("home");
    let html = docs.read("index.html");
    assert!(
        html.contains("<!DOCTYPE html>") || html.contains("<!doctype html>"),
        "{html}"
    );
    assert!(html.contains("Liyasa"), "{html}");
    assert!(docs.read("index.md").contains("# Liyasa"));
}
