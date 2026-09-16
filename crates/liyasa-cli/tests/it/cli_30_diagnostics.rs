//! CLI-30: a diagnostic printed to a terminal has a code frame, and the same
//! diagnostic in `--json` has a shape that does not move between releases.
//!
//! The JSON is asserted against `schemas/diagnostic.json`, the frozen contract,
//! rather than against a snapshot of what the renderer happens to emit.

use crate::support::{Dir, Run};

const DIAGNOSTIC_SCHEMA: &str = include_str!("../../../../schemas/diagnostic.json");

/// A project whose config has a type error, which is a diagnostic with a span
/// and therefore one a code frame can be drawn for.
fn broken(name: &str) -> Dir {
    let project = Dir::new(name);
    project
        .write(
            "liyasa.json",
            "{\n  \"name\": 123,\n  \"seo\": { \"canonicalOrigin\": \"https://docs.acme.com\" }\n}\n",
        )
        .write("index.md", "---\ntitle: Home\n---\n\n# Home\n");
    project
}

fn validator() -> jsonschema::Validator {
    let schema: serde_json::Value =
        serde_json::from_str(DIAGNOSTIC_SCHEMA).expect("the diagnostic schema is valid JSON");
    jsonschema::options()
        .with_draft(jsonschema::Draft::Draft202012)
        .build(&schema)
        .expect("the diagnostic schema compiles")
}

fn diagnostics_of(output: &str) -> Vec<serde_json::Value> {
    let document: serde_json::Value =
        serde_json::from_str(output).unwrap_or_else(|error| panic!("not JSON: {error}\n{output}"));
    document
        .get("diagnostics")
        .and_then(serde_json::Value::as_array)
        .cloned()
        .unwrap_or_default()
}

#[test]
fn the_human_form_draws_a_code_frame() {
    let project = broken("cli30-frame");
    let outcome = Run::new(["validate"]).cwd(project.path()).output();
    let text = outcome.all();

    assert!(text.contains("E0102"), "no code in: {text}");
    assert!(text.contains("liyasa.json"), "no file name in: {text}");
    // The frame itself: miette's gutter, the source line, and the underline.
    assert!(text.contains('╭'), "no frame in: {text}");
    assert!(text.contains('│'), "no gutter in: {text}");
    assert!(
        text.contains("\"name\": 123"),
        "the offending line is not shown: {text}"
    );
}

/// Without a terminal there is no colour and no hyperlink escape, so a
/// redirected log is readable.
#[test]
fn a_pipe_gets_no_escape_sequences() {
    let project = broken("cli30-plain");
    let outcome = Run::new(["validate"]).cwd(project.path()).output();
    assert!(
        !outcome.all().contains('\u{1b}'),
        "an escape sequence reached a pipe: {:?}",
        outcome.all()
    );
}

/// `--color always` overrides the absence of a terminal, which is how a CI
/// system that renders ANSI asks for it.
#[test]
fn colour_can_be_forced_on_and_off() {
    let project = broken("cli30-colour");

    let forced = Run::new(["validate", "--color", "always"])
        .cwd(project.path())
        .output();
    assert!(
        forced.all().contains('\u{1b}'),
        "--color always produced no colour: {:?}",
        forced.all()
    );

    let refused = Run::new(["validate", "--color", "never"])
        .cwd(project.path())
        .output();
    assert!(!refused.all().contains('\u{1b}'));
}

#[test]
fn every_json_diagnostic_matches_the_frozen_schema() {
    let project = broken("cli30-schema");
    let outcome = Run::new(["validate", "--format", "json"])
        .cwd(project.path())
        .output();

    let diagnostics = diagnostics_of(&outcome.stdout);
    assert!(
        !diagnostics.is_empty(),
        "nothing to check: {}",
        outcome.all()
    );

    let validator = validator();
    for diagnostic in &diagnostics {
        assert!(
            validator.is_valid(diagnostic),
            "does not match schemas/diagnostic.json: {}\n{:#?}",
            serde_json::to_string_pretty(diagnostic).unwrap_or_default(),
            validator.validate(diagnostic).err().map(|e| e.to_string()),
        );
    }
}

/// CLI-30 names the keys a consumer may rely on. `file` and the line and
/// column inside `span` are the two the core serialization cannot supply,
/// because a `SourceId` means nothing outside the process that interned it.
#[test]
fn the_json_carries_the_keys_the_requirement_names() {
    let project = broken("cli30-keys");
    let outcome = Run::new(["validate", "--format", "json"])
        .cwd(project.path())
        .output();

    let diagnostics = diagnostics_of(&outcome.stdout);
    let located = diagnostics
        .iter()
        .find(|d| d.get("code").and_then(serde_json::Value::as_str) == Some("E0102"))
        .unwrap_or_else(|| panic!("no E0102 in {}", outcome.stdout));

    for key in ["code", "severity", "message", "url", "file", "span"] {
        assert!(located.get(key).is_some(), "no `{key}` in {located}");
    }
    assert_eq!(
        located.get("file").and_then(serde_json::Value::as_str),
        Some("liyasa.json")
    );

    let span = located.get("span").expect("a span");
    for key in [
        "source",
        "start",
        "end",
        "line",
        "column",
        "endLine",
        "endColumn",
    ] {
        assert!(span.get(key).is_some(), "no `{key}` in span {span}");
    }
    // The type error is on the second line of the file written above.
    assert_eq!(
        span.get("line").and_then(serde_json::Value::as_u64),
        Some(2)
    );
}

/// `--json` is the global spelling of `--format json`, and the two agree.
#[test]
fn the_global_json_flag_and_the_format_flag_agree() {
    let project = broken("cli30-json-alias");
    let with_format = Run::new(["validate", "--format", "json"])
        .cwd(project.path())
        .output();
    let with_global = Run::new(["validate", "--json"])
        .cwd(project.path())
        .output();

    assert_eq!(
        diagnostics_of(&with_format.stdout).len(),
        diagnostics_of(&with_global.stdout).len(),
        "{}\n----\n{}",
        with_format.stdout,
        with_global.stdout
    );
}

/// Machine output goes to stdout and the human form to stderr, so a pipeline
/// reading JSON never has a progress line spliced into it.
#[test]
fn machine_output_is_on_stdout_alone() {
    let project = broken("cli30-streams");
    let outcome = Run::new(["validate", "--format", "json"])
        .cwd(project.path())
        .output();
    assert!(
        serde_json::from_str::<serde_json::Value>(outcome.stdout.trim()).is_ok(),
        "stdout is not one JSON document: {}",
        outcome.stdout
    );
}

/// SARIF is what turns a diagnostic into a GitHub code-scanning annotation.
#[test]
fn sarif_carries_the_rule_and_the_region() {
    let project = broken("cli30-sarif");
    let outcome = Run::new(["validate", "--format", "sarif"])
        .cwd(project.path())
        .output();

    let document: serde_json::Value = serde_json::from_str(&outcome.stdout)
        .unwrap_or_else(|error| panic!("not JSON: {error}\n{}", outcome.stdout));
    assert_eq!(
        document.get("version").and_then(serde_json::Value::as_str),
        Some("2.1.0")
    );

    let results = document
        .pointer("/runs/0/results")
        .and_then(serde_json::Value::as_array)
        .unwrap_or_else(|| panic!("no results in {}", outcome.stdout));
    let found = results.iter().any(|result| {
        result.get("ruleId").and_then(serde_json::Value::as_str) == Some("E0102")
            && result
                .pointer("/locations/0/physicalLocation/region/startLine")
                .is_some()
    });
    assert!(found, "no located E0102 result in {}", outcome.stdout);

    let rules = document
        .pointer("/runs/0/tool/driver/rules")
        .and_then(serde_json::Value::as_array)
        .unwrap_or_else(|| panic!("no rules in {}", outcome.stdout));
    assert!(
        rules
            .iter()
            .any(|rule| rule.get("id").and_then(serde_json::Value::as_str) == Some("E0102")),
        "E0102 has no rule entry"
    );
}
