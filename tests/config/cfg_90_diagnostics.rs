//! CFG-90: every semantic rule reports a registered code, at a span, in the
//! shape `schemas/diagnostic.json` describes — the same format content errors
//! use. `liyasa validate --format json` itself is the CLI's (WP-09); this is
//! what the CLI would print.

use liyasa_core::diagnostics::Diagnostic;
use liyasa_tests::config::project;
use serde_json::Value;

/// A config that triggers one rule, and the code it has to raise.
const CASES: &[(&str, &str, &str)] = &[
    (
        "E0101",
        r#"{ "name": "Acme", }"#,
        "`liyasa.json` is not valid JSON",
    ),
    (
        "E0102",
        r#"{ "description": "no name" }"#,
        "a key the schema requires is missing",
    ),
    (
        "E0103",
        r#"{ "name": "Acme", "nvaigation": [] }"#,
        "an unknown key",
    ),
    (
        "E0104",
        r#"{ "name": "Acme", "navigation": ["index", "gone"] }"#,
        "navigation names a page that is not there",
    ),
    (
        "E0105",
        r#"{ "name": "Acme", "navigation": ["index", "index"] }"#,
        "the same route twice",
    ),
    (
        "E0106",
        r#"{ "name": "Acme", "navigation": ["index"],
             "redirects": { "rules": [{ "source": "/a", "destination": "/b" },
                                      { "source": "/a", "destination": "/c" }] } }"#,
        "two redirects from one source",
    ),
    (
        "E0107",
        r##"{ "name": "Acme", "navigation": ["index"],
             "theme": { "colors": { "primary": "#818CF8" } } }"##,
        "no label on the fill clears AA",
    ),
    (
        "E0108",
        r#"{ "name": "Acme", "navigation": ["index"],
             "versions": [{ "name": "v2" }, { "name": "v1" }] }"#,
        "no version is the default",
    ),
    (
        "E0109",
        r#"{ "name": "Acme", "navigation": ["index"],
             "redirects": { "rules": [{ "source": "/a", "destination": "https://elsewhere.example/x" }] } }"#,
        "an absolute destination no host allows",
    ),
    (
        "E0120",
        r#"{ "name": "Acme", "navigation": ["index"], "public": false }"#,
        "a private site before the server ships",
    ),
    (
        "E0121",
        r#"{ "$schema": "https://liyasa.dev/schema/v2/liyasa.json", "name": "Acme",
             "navigation": ["index"] }"#,
        "a schema version this build does not know",
    ),
    (
        "W0130",
        r#"{ "name": "Acme", "navigation": [] }"#,
        "a page no navigation reaches",
    ),
    (
        "W0131",
        r#"{ "name": "Acme", "navigation": ["index"] }"#,
        "no `seo.canonicalOrigin`",
    ),
    (
        "E0132",
        r##"{ "name": "Acme", "navigation": ["index"],
             "theme": { "colors": { "primary": "#ggg" } } }"##,
        "a colour Liyasa cannot read",
    ),
    (
        "E0133",
        r#"{ "name": "Acme", "navigation": [{ "version": "v9", "pages": ["index"] }] }"#,
        "a subtree bound to a version nobody declared",
    ),
];

/// The origin every case but the `W0131` one carries, so the missing-origin
/// warning does not answer for another case's config.
const ORIGIN: &str = r#""seo": { "canonicalOrigin": "https://acme.dev" },"#;

fn config(case: &str, text: &str) -> String {
    if case == "W0131" || case == "E0101" || case == "E0102" {
        return text.to_owned();
    }
    text.replacen('{', &format!("{{ {ORIGIN}"), 1)
}

fn validator() -> jsonschema::Validator {
    let schema: Value = serde_json::from_str(include_str!("../../schemas/diagnostic.json"))
        .expect("the diagnostic schema is valid JSON");
    jsonschema::options()
        .with_draft(jsonschema::Draft::Draft202012)
        .build(&schema)
        .expect("it compiles")
}

#[test]
fn every_rule_reports_its_code() {
    for (code, text, what) in CASES {
        let text = config(code, text);
        let checked = project(&[("liyasa.json", &text), ("index.md", "# Home")]);
        let codes: Vec<&str> = checked
            .diagnostics
            .iter()
            .map(|diagnostic| diagnostic.code.as_str())
            .collect();
        assert!(
            codes.contains(code),
            "{what}: {code} is not among {codes:?}"
        );
    }
}

#[test]
fn every_diagnostic_points_into_the_file_it_came_from() {
    for (code, text, what) in CASES {
        let text = config(code, text);
        let checked = project(&[("liyasa.json", &text), ("index.md", "# Home")]);
        let diagnostic = checked
            .diagnostics
            .iter()
            .find(|diagnostic| diagnostic.code.as_str() == *code)
            .unwrap_or_else(|| panic!("{code} was not reported"));
        let Some(span) = diagnostic.span else {
            // W0130 is about a page the config never mentions, so there is
            // nowhere in `liyasa.json` to point at.
            assert_eq!(*code, "W0130", "{what}: {code} carries no span");
            continue;
        };
        assert!(
            span.end as usize <= text.len() && span.start <= span.end,
            "{code}: {span:?} is outside a {} byte file",
            text.len()
        );
    }
}

#[test]
fn what_the_cli_would_print_matches_the_published_shape() {
    let validator = validator();
    for (code, text, _) in CASES {
        let text = config(code, text);
        let checked = project(&[("liyasa.json", &text), ("index.md", "# Home")]);
        for diagnostic in checked.diagnostics.iter() {
            let value = serde_json::to_value(diagnostic).expect("a diagnostic serializes");
            let errors: Vec<String> = validator
                .iter_errors(&value)
                .map(|e| e.to_string())
                .collect();
            assert_eq!(errors, Vec::<String>::new(), "{code}: {value}");
        }
    }
}

#[test]
fn a_diagnostic_carries_the_url_its_code_generates() {
    let checked = project(&[
        ("liyasa.json", r#"{ "name": "Acme" }"#),
        ("index.md", "# Home"),
    ]);
    let diagnostic: &Diagnostic = checked.diagnostics.iter().next().expect("one diagnostic");
    assert!(
        diagnostic.url.ends_with(diagnostic.code.as_str()),
        "{}",
        diagnostic.url
    );
}
