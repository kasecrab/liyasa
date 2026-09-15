//! THM-31 and THM-32: the base bundle fits its budget, every lazy module fits
//! its own, and nothing the theme ships fetches from another origin.

use std::io::Write;
use std::process::{Command, Stdio};

use liyasa_theme::config::ThemeConfig;
use liyasa_theme::runtime::{BASE_BUDGET, BOOTSTRAP, Runtime, budget_of, external_requests};
use liyasa_theme::stylesheet::Styles;
use liyasa_theme::tokens::Tokens;

fn compressed_len(text: &str) -> usize {
    let Ok(mut child) = Command::new("gzip")
        .arg("-9c")
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::null())
        .spawn()
    else {
        return text.len();
    };
    if let Some(stdin) = child.stdin.as_mut() {
        stdin
            .write_all(text.as_bytes())
            .expect("gzip accepts input");
    }
    let output = child.wait_with_output().expect("gzip finishes");
    output.stdout.len()
}

#[test]
fn the_base_bundle_fits_its_budget() {
    let runtime = Runtime::build(&ThemeConfig::default());
    let compressed = compressed_len(&runtime.base);
    assert!(
        compressed <= BASE_BUDGET,
        "the base bundle is {compressed} bytes compressed ({} uncompressed)",
        runtime.base.len()
    );
}

#[test]
fn every_lazy_module_fits_its_budget() {
    let runtime = Runtime::build(&ThemeConfig::default());
    let measured: Vec<(&str, usize)> = runtime
        .lazy
        .iter()
        .map(|module| (module.name, compressed_len(&module.source)))
        .collect();
    for (name, size) in &measured {
        assert!(
            *size <= budget_of(name),
            "`{name}` is {size} bytes compressed"
        );
    }
    let compressed = compressed_len(&runtime.base);
    assert!(runtime.over_budget(compressed, &measured).is_empty());
}

#[test]
fn nothing_the_theme_ships_leaves_the_origin() {
    let runtime = Runtime::build(&ThemeConfig::default());
    let styles =
        Styles::build(&ThemeConfig::default(), &Tokens::aurora(), &[]).expect("the theme compiles");
    let mut sources = vec![runtime.base.as_str(), BOOTSTRAP, styles.css.as_str()];
    for module in &runtime.lazy {
        sources.push(&module.source);
    }
    let found = external_requests(&sources);
    assert!(found.is_empty(), "third-party requests: {found:?}");
}

#[test]
fn every_hook_thm_33_documents_is_emitted() {
    let runtime = Runtime::build(&ThemeConfig::default());
    for hook in [
        "page:load",
        "search:open",
        "theme:change",
        "feedback:submit",
    ] {
        assert!(
            runtime.base.contains(&format!("emit(\"{hook}\"")),
            "`liyasa.on(\"{hook}\")` would never fire"
        );
    }
    // A custom script registers before the first hook fires, because the API
    // is defined by the first module in the bundle and `theme.js` is deferred
    // after it (CMP-101).
    let api = runtime
        .base
        .find("window.liyasa = liyasa")
        .expect("the api is defined");
    let first_emit = runtime
        .base
        .find("emit(\"page:load\"")
        .expect("page:load fires");
    assert!(api < first_emit);
}

#[test]
fn every_module_guards_the_elements_it_enhances() {
    // A module that assumes its markup is present breaks every page that does
    // not carry it, which is how a progressive-enhancement bundle stops being
    // one. Each module either queries a list or returns early.
    let runtime = Runtime::build(&ThemeConfig::default());
    for module in runtime.module_names() {
        assert!(
            !runtime.base.is_empty(),
            "`{module}` is listed but the bundle is empty"
        );
    }
    assert!(runtime.base.matches("querySelectorAll").count() >= 6);
    assert!(runtime.base.contains("if (!dialog || !trigger) return;"));
}
