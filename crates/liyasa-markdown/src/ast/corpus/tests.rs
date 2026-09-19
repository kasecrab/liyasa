use liyasa_core::markdown::{HtmlMode, ParseOptions};

use super::*;
use crate::directives::testing::{codes, corpus_registry, expanded};

fn options_for(case: &Case) -> ParseOptions {
    let mut options = ParseOptions {
        build_nonce: crate::directives::testing::NONCE,
        ..ParseOptions::default()
    };
    for (key, value) in &case.options {
        let on = value.as_bool().unwrap_or(true);
        match key.as_str() {
            "content.math" => options.math = on,
            "content.wikilinks" => options.wikilinks = on,
            "content.html" => {
                options.html = match value.as_str() {
                    Some("off") => HtmlMode::Off,
                    Some("allow") => HtmlMode::Allow,
                    _ => HtmlMode::Sanitize,
                };
            }
            _ => {}
        }
    }
    options
}

fn run(case: &Case) -> Vec<String> {
    let document = crate::ast::parse(
        &expanded(&case.source),
        &corpus_registry(),
        &options_for(case),
    );
    codes(&document).into_iter().map(str::to_owned).collect()
}

/// The corpus is regenerated rather than committed, so a checkout without it
/// says so once instead of failing every case.
///
/// Every other test here loops over what this returns, so when it returns
/// nothing they all loop over nothing and report `ok`. That is a pass which
/// asserts nothing, and it is indistinguishable in the output from a pass that
/// checked 977 cases — WP-00 hit it and only noticed because the run was
/// suspiciously fast. [`the_corpus_is_there_at_all`] is what makes the empty
/// case visible; it is deliberately the only test here that fails on it, so
/// one clear failure names the cause instead of three silent successes.
fn corpus() -> Vec<Case> {
    match load() {
        Some(cases) if !cases.is_empty() => cases,
        _ => {
            eprintln!(
                "spec/markdown is not checked out; set LIYASA_CORPUS or see its README to rebuild it"
            );
            Vec::new()
        }
    }
}

/// The precondition every other test in this module rests on.
#[test]
fn the_corpus_is_there_at_all() {
    let cases = load().unwrap_or_default();
    assert!(
        !cases.is_empty(),
        "no corpus was found, so every case-driven test in this module passed \
         without asserting anything. Point LIYASA_CORPUS at spec/markdown, or \
         see spec/markdown/README.md to rebuild it."
    );
}

/// Every case, whatever it asserts, must come back as a document.
#[test]
fn no_case_in_the_corpus_panics() {
    for case in corpus() {
        let _ = run(&case);
    }
}

/// The `diagnostics` sections the spike engines report as skipped.
#[test]
fn every_expected_diagnostic_is_raised() {
    let mut failures = Vec::new();
    for case in corpus() {
        let Some(expected) = &case.diagnostics else {
            continue;
        };
        if case.pending.is_some() {
            continue;
        }
        let raised = run(&case);
        for want in expected
            .iter()
            .filter(|code| RAISED_HERE.contains(&code.as_str()))
        {
            if !raised.contains(want) {
                failures.push(format!(
                    "{}: expected {want}, got [{}]",
                    case.id,
                    raised.join(", ")
                ));
            }
        }
    }
    assert!(failures.is_empty(), "\n{}", failures.join("\n"));
}

/// A diagnostic nobody claimed is a rule that started firing unnoticed.
///
/// A case that writes a `diagnostics` section, even an empty one, has been
/// looked at, so it is held to every code the parser raises. A case without one
/// is held only to the codes that do not depend on a registry, because the
/// corpus names components for illustration and ships no schema for them.
#[test]
fn no_case_raises_an_unclaimed_error() {
    let mut surprises = Vec::new();
    for case in corpus() {
        // The imported suites assert CommonMark and GFM under their own
        // options, which the spike engines check. Liyasa's parser runs with
        // directives and the sanitizer on, so it is expected to say more about
        // them than the reference implementations do.
        if case.pending.is_some()
            || case
                .tags
                .iter()
                .any(|tag| tag == "commonmark" || tag == "gfm")
        {
            continue;
        }
        let held_to = if case.diagnostics.is_some() {
            RAISED_HERE
        } else {
            REGISTRY_FREE
        };
        let mut expected = case.diagnostics.clone().unwrap_or_default();
        for raised in run(&case) {
            if !held_to.contains(&raised.as_str()) {
                continue;
            }
            match expected.iter().position(|want| *want == raised) {
                Some(at) => {
                    expected.remove(at);
                }
                None => surprises.push(format!("{}: unexpected {raised}", case.id)),
            }
        }
    }
    assert!(surprises.is_empty(), "\n{}", surprises.join("\n"));
}

/// Prints every case whose diagnostics differ from what it asserts, so the
/// corpus can be corrected by hand rather than by capture.
#[test]
#[ignore = "a reporting aid, not an assertion"]
fn report_diagnostics() {
    for case in corpus() {
        let raised = run(&case);
        let expected = case.diagnostics.clone().unwrap_or_default();
        if raised.is_empty() && expected.is_empty() {
            continue;
        }
        println!("{}\t{:?}\t{:?}", case.id, expected, raised);
    }
}
