//! CLI-04: `liyasa validate` over a fixture with a fault of each kind.
//!
//! The acceptance criterion asks for config, content, link, and spec errors in
//! one project, and for the JSON to validate against the diagnostic schema and
//! to list every expected code.

use std::collections::BTreeSet;

use liyasa_cli::Exit;

use crate::support::{Dir, Run};

const DIAGNOSTIC_SCHEMA: &str = include_str!("../../../../schemas/diagnostic.json");

/// One project, four kinds of fault.
fn fixture(name: &str) -> Dir {
    let project = Dir::new(name);
    project
        // Config: `name` must be a string, and the navigation names a page
        // that is not in the project.
        .write(
            "liyasa.json",
            concat!(
                "{\n",
                "  \"name\": 123,\n",
                "  \"seo\": { \"canonicalOrigin\": \"https://docs.acme.com\" },\n",
                "  \"navigation\": [{ \"group\": \"Guides\", \"pages\": [\"guides/missing\"] }],\n",
                "  \"openapi\": [{ \"id\": \"api\", \"source\": \"openapi/api.yaml\" }]\n",
                "}\n"
            ),
        )
        // Content: a code fence that is never closed. Link: a route that does
        // not exist, and an image that is not a file.
        .write(
            "index.md",
            concat!(
                "---\ntitle: Home\n---\n\n",
                "# Home\n\n",
                "[Install](/guides/install)\n\n",
                "![A diagram](assets/missing.png)\n\n",
                "```rust\n",
                "let unclosed = 1;\n"
            ),
        )
        // Spec: valid YAML, but not a document the loader accepts.
        .write(
            "openapi/api.yaml",
            "openapi: 9.9.9\ninfo:\n  title: Acme\n  version: \"1\"\npaths: {}\n",
        );
    project
}

fn codes(output: &str) -> BTreeSet<String> {
    let document: serde_json::Value =
        serde_json::from_str(output).unwrap_or_else(|error| panic!("not JSON: {error}\n{output}"));
    document
        .get("diagnostics")
        .and_then(serde_json::Value::as_array)
        .map(|list| {
            list.iter()
                .filter_map(|d| d.get("code").and_then(serde_json::Value::as_str))
                .map(str::to_owned)
                .collect()
        })
        .unwrap_or_default()
}

#[test]
fn every_expected_code_is_reported() {
    let project = fixture("cli04-all");
    let outcome = Run::new(["validate", "--format", "json"])
        .cwd(project.path())
        .output();

    assert_eq!(outcome.code, Exit::Errors.code(), "{}", outcome.all());

    let found = codes(&outcome.stdout);
    for (code, what) in [
        ("E0102", "the config type error"),
        ("E0104", "navigation naming a page that does not exist"),
        ("E0301", "the unclosed code fence"),
        ("E0401", "the broken internal link"),
        ("E0403", "the missing image"),
        ("E0504", "the unsupported OpenAPI version"),
    ] {
        assert!(
            found.contains(code),
            "{code} is missing ({what}); found {found:?}\n{}",
            outcome.stdout
        );
    }
}

#[test]
fn the_output_validates_against_the_diagnostic_schema() {
    let project = fixture("cli04-schema");
    let outcome = Run::new(["validate", "--format", "json"])
        .cwd(project.path())
        .output();

    let schema: serde_json::Value =
        serde_json::from_str(DIAGNOSTIC_SCHEMA).expect("the diagnostic schema is valid JSON");
    let validator = jsonschema::options()
        .with_draft(jsonschema::Draft::Draft202012)
        .build(&schema)
        .expect("the diagnostic schema compiles");

    let document: serde_json::Value = serde_json::from_str(&outcome.stdout)
        .unwrap_or_else(|error| panic!("not JSON: {error}\n{}", outcome.stdout));
    let list = document
        .get("diagnostics")
        .and_then(serde_json::Value::as_array)
        .unwrap_or_else(|| panic!("no diagnostics in {}", outcome.stdout));

    assert!(!list.is_empty());
    for diagnostic in list {
        assert!(
            validator.is_valid(diagnostic),
            "does not validate: {diagnostic}"
        );
    }
}

/// The defect RFC 0904 records: a warm build drops the diagnostics its cached
/// pages produced, so `validate` must not depend on whether the cache is warm.
/// If this fails, the `clean: true` in `validate.rs` was removed before the
/// engine was fixed.
#[test]
fn two_runs_in_a_row_report_the_same_thing() {
    let project = fixture("cli04-stable");
    let first = Run::new(["validate", "--format", "json"])
        .cwd(project.path())
        .output();
    let second = Run::new(["validate", "--format", "json"])
        .cwd(project.path())
        .output();

    assert_eq!(
        codes(&first.stdout),
        codes(&second.stdout),
        "the second run disagreed with the first"
    );
    assert_eq!(first.code, second.code);
}

/// A subset keeps its own codes and drops the other subsets', while a code no
/// subset claims is always reported (RFC 0903).
#[test]
fn a_subset_narrows_the_report() {
    let project = fixture("cli04-subset");
    let outcome = Run::new(["validate", "--format", "json", "--only", "config"])
        .cwd(project.path())
        .output();

    let found = codes(&outcome.stdout);
    assert!(found.contains("E0102"), "{found:?}");
    assert!(
        !found.contains("E0301"),
        "content codes survived --only config: {found:?}"
    );
    assert!(
        !found.contains("E0504"),
        "spec codes survived --only config: {found:?}"
    );
}

#[test]
fn the_shorthand_flags_are_the_subsets_they_name() {
    let project = fixture("cli04-shorthand");
    let by_flag = Run::new(["validate", "--format", "json", "--links"])
        .cwd(project.path())
        .output();
    let by_only = Run::new(["validate", "--format", "json", "--only", "links"])
        .cwd(project.path())
        .output();

    assert_eq!(codes(&by_flag.stdout), codes(&by_only.stdout));
    assert!(codes(&by_flag.stdout).contains("E0401"));
}

/// Several subsets union rather than intersect.
#[test]
fn subsets_compose() {
    let project = fixture("cli04-compose");
    let outcome = Run::new(["validate", "--format", "json", "--only", "config,content"])
        .cwd(project.path())
        .output();

    let found = codes(&outcome.stdout);
    assert!(found.contains("E0102"), "{found:?}");
    assert!(found.contains("E0301"), "{found:?}");
    assert!(!found.contains("E0401"), "{found:?}");
}

/// A sound project exits 0 and says so.
#[test]
fn a_sound_project_passes() {
    let project = Dir::new("cli04-sound");
    project
        .write(
            "liyasa.json",
            r#"{"name":"Acme docs","seo":{"canonicalOrigin":"https://docs.acme.com"}}"#,
        )
        .write("index.md", "---\ntitle: Home\n---\n\n# Home\n\nWelcome.\n");

    let outcome = Run::new(["validate"]).cwd(project.path()).output();
    assert_eq!(outcome.code, Exit::Success.code(), "{}", outcome.all());
}

/// Validation never writes the site's output directory, whatever the project
/// configures it to be.
#[test]
fn validation_does_not_write_the_output_directory() {
    let project = fixture("cli04-no-output");
    let _ = Run::new(["validate"]).cwd(project.path()).output();
    assert!(
        !project.path().join("dist").exists(),
        "validate wrote dist/"
    );
}

/// A remote spec cannot be fetched by this build, and saying nothing would
/// claim it was checked.
#[test]
fn a_remote_spec_is_reported_as_unchecked() {
    let project = Dir::new("cli04-remote-spec");
    project
        .write(
            "liyasa.json",
            concat!(
                "{\n",
                "  \"name\": \"Acme docs\",\n",
                "  \"seo\": { \"canonicalOrigin\": \"https://docs.acme.com\" },\n",
                "  \"openapi\": [{ \"id\": \"api\", \"source\": \"https://example.com/api.yaml\" }]\n",
                "}\n"
            ),
        )
        .write("index.md", "---\ntitle: Home\n---\n\n# Home\n");

    let outcome = Run::new(["validate", "--format", "json"])
        .cwd(project.path())
        .output();
    assert!(
        codes(&outcome.stdout).contains("W0017"),
        "{}",
        outcome.stdout
    );
    // A warning, so the project still validates.
    assert_eq!(outcome.code, Exit::Success.code(), "{}", outcome.all());
}

/// §6.6.4: `--personalization` lists the pages rendered per request, so an
/// author can keep them few. The W0715 warnings say why each one is dynamic;
/// this is the roll-up, because a warning per page is not a list.
#[test]
fn personalization_lists_the_on_demand_pages() {
    let project = Dir::new("cli04-personalization");
    project
        .write(
            "liyasa.json",
            r#"{"name":"Acme docs","seo":{"canonicalOrigin":"https://docs.acme.com"}}"#,
        )
        .write("index.md", "---\ntitle: Home\n---\n\n# Home\n\nWelcome.\n")
        .write(
            "account.md",
            "---\ntitle: Account\npersonalized: true\n---\n\n# Account\n\nHello {{ reader.name }}.\n",
        );

    let outcome = Run::new(["validate", "--personalization"])
        .cwd(project.path())
        .output();

    assert!(
        outcome.stdout.contains("/account"),
        "the on-demand page is not listed: {}",
        outcome.all()
    );
    assert!(outcome.stdout.contains("on demand"), "{}", outcome.stdout);
}

/// A site with no personalized page says so rather than printing an empty
/// heading, because "none" is the answer an author wants to see.
#[test]
fn personalization_says_when_there_are_none() {
    let project = Dir::new("cli04-personalization-none");
    project
        .write(
            "liyasa.json",
            r#"{"name":"Acme docs","seo":{"canonicalOrigin":"https://docs.acme.com"}}"#,
        )
        .write("index.md", "---\ntitle: Home\n---\n\n# Home\n\nWelcome.\n");

    let outcome = Run::new(["validate", "--personalization"])
        .cwd(project.path())
        .output();

    assert_eq!(outcome.code, Exit::Success.code(), "{}", outcome.all());
    assert!(
        outcome.stdout.contains("no page is rendered on demand"),
        "{}",
        outcome.stdout
    );
}

/// The listing is machine-readable too, since the reason to track on-demand
/// pages is usually a budget somewhere.
#[test]
fn personalization_is_readable_as_json() {
    let project = Dir::new("cli04-personalization-json");
    project
        .write(
            "liyasa.json",
            r#"{"name":"Acme docs","seo":{"canonicalOrigin":"https://docs.acme.com"}}"#,
        )
        .write("index.md", "---\ntitle: Home\n---\n\n# Home\n\nWelcome.\n");

    let outcome = Run::new(["validate", "--personalization", "--format", "json"])
        .cwd(project.path())
        .output();

    // One document, not two: the listing goes inside the envelope the
    // diagnostics already come in.
    let document: serde_json::Value =
        serde_json::from_str(outcome.stdout.trim()).unwrap_or_else(|error| {
            panic!(
                "stdout is not one JSON document ({error}): {}",
                outcome.stdout
            )
        });
    assert!(
        document["personalization"]["onDemand"].is_array(),
        "{document}"
    );
    assert!(document["diagnostics"].is_array(), "{document}");
}

/// Without the flag nothing is listed, so the default output is unchanged.
#[test]
fn personalization_is_off_unless_asked_for() {
    let project = Dir::new("cli04-personalization-off");
    project
        .write(
            "liyasa.json",
            r#"{"name":"Acme docs","seo":{"canonicalOrigin":"https://docs.acme.com"}}"#,
        )
        .write("index.md", "---\ntitle: Home\n---\n\n# Home\n\nWelcome.\n");

    let outcome = Run::new(["validate"]).cwd(project.path()).output();
    assert!(!outcome.stdout.contains("on demand"), "{}", outcome.stdout);
}

/// SARIF is one document and stdout carries it whole. The listing is still
/// shown, on stderr, where it cannot corrupt what a parser reads.
#[test]
fn personalization_never_corrupts_a_machine_document() {
    let project = Dir::new("cli04-personalization-sarif");
    project
        .write(
            "liyasa.json",
            r#"{"name":"Acme docs","seo":{"canonicalOrigin":"https://docs.acme.com"}}"#,
        )
        .write("index.md", "---\ntitle: Home\n---\n\n# Home\n\nWelcome.\n");

    let outcome = Run::new(["validate", "--personalization", "--format", "sarif"])
        .cwd(project.path())
        .output();

    let document: serde_json::Value = serde_json::from_str(outcome.stdout.trim())
        .unwrap_or_else(|error| panic!("stdout is not SARIF ({error}): {}", outcome.stdout));
    assert_eq!(document["version"].as_str(), Some("2.1.0"), "{document}");
    assert!(
        outcome.stderr.contains("on demand"),
        "the listing was dropped rather than moved: {}",
        outcome.stderr
    );
}
