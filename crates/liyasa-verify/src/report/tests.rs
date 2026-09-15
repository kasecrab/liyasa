use std::time::Duration;

use liyasa_core::diagnostics::{Diagnostic, code};
use liyasa_core::ids::{BlockId, CheckId, Route};
use liyasa_core::verify::CheckOutcome;
use serde_json::{Value, json};

use super::*;
use crate::core::config::DriftSeverity;

fn check(id: &str, runner: &str, outcome: CheckOutcome, severity: Option<Severity>) -> CheckReport {
    CheckReport {
        id: CheckId::new(id),
        block: BlockId::explicit(id),
        runner: runner.to_owned(),
        class: CheckClass::Code,
        outcome,
        duration: Duration::from_millis(120),
        severity,
    }
}

fn fail(excerpt: &str) -> CheckOutcome {
    CheckOutcome::Fail {
        excerpt: excerpt.to_owned(),
    }
}

/// A run with one of everything: a pass, a failure, a skip, a structural
/// warning, and an open drift.
fn mixed_run() -> Report {
    let mut install = PageReport::new(Route::new("/guide/install"));
    install.source_path = Some("guide/install.md".to_owned());
    install.checks = vec![
        check(
            "/guide/install#a#0",
            "shell",
            CheckOutcome::Pass,
            Some(Severity::Error),
        ),
        check(
            "/guide/install#b#0",
            "http",
            fail("status is 500, not 200"),
            Some(Severity::Error),
        ),
        check(
            "/guide/install#c#0",
            "shell",
            CheckOutcome::Skip {
                reason: "needs a windows runner".to_owned(),
            },
            Some(Severity::Error),
        ),
    ];
    install.diagnostics = vec![Diagnostic::new(
        code::W0630,
        "`/guide/install` has no `description`",
    )];

    let mut pricing = PageReport::new(Route::new("/pricing"));
    pricing.source_path = Some("pricing.md".to_owned());
    pricing.checks = vec![check(
        "/pricing#a#0",
        "regex",
        CheckOutcome::Error(Diagnostic::new(code::E0603, "the check did not finish")),
        Some(Severity::Error),
    )];
    pricing.drift = vec![DriftEntry {
        id: "drift_01".to_owned(),
        summary: "facts.pricing.pro changed from 49 to 59".to_owned(),
        severity: DriftSeverity::High,
        age: Duration::from_secs(3 * 24 * 60 * 60),
    }];

    Report::new(vec![install, pricing])
}

fn plain() -> Scrubber {
    Scrubber::new()
}

// ---- VER-70's acceptance criterion ----

#[test]
fn ver_70_sarif_carries_what_a_consumer_requires() {
    let document = sarif::document(&mixed_run(), &plain());
    let problems = validate_sarif(&document);
    assert!(problems.is_empty(), "{problems:#?}\n{document:#}");
}

#[test]
fn ver_70_junit_carries_what_a_runner_requires() {
    let xml = mixed_run().render(Format::Junit, &plain());
    let problems = validate_junit(&xml);
    assert!(problems.is_empty(), "{problems:#?}\n{xml}");
}

#[test]
fn ver_70_the_exit_code_is_three_on_failures() {
    let report = mixed_run();
    assert!(report.has_failures());
    assert_eq!(report.exit_code(), ExitCode::Verification);
    assert_eq!(report.exit_code().code(), 3);
}

// ---- exit codes (CLI-31) ----

#[test]
fn a_clean_run_exits_zero() {
    let mut page = PageReport::new(Route::new("/p"));
    page.checks = vec![check(
        "/p#a#0",
        "shell",
        CheckOutcome::Pass,
        Some(Severity::Error),
    )];
    let report = Report::new(vec![page]);
    assert_eq!(report.exit_code(), ExitCode::Success);
}

#[test]
fn a_failure_in_a_class_policy_only_warns_about_does_not_fail_the_run() {
    let mut page = PageReport::new(Route::new("/p"));
    page.checks = vec![check(
        "/p#a#0",
        "http",
        fail("status is 500"),
        Some(Severity::Warning),
    )];
    let report = Report::new(vec![page]);
    assert!(!report.has_failures());
    assert_eq!(report.exit_code(), ExitCode::Success);
}

#[test]
fn a_failure_in_a_class_that_is_off_is_recorded_and_costs_nothing() {
    let mut page = PageReport::new(Route::new("/p"));
    page.checks = vec![check("/p#a#0", "http", fail("status is 500"), None)];
    let report = Report::new(vec![page]);
    assert_eq!(report.summary().fail, 1, "the failure is still counted");
    assert_eq!(report.exit_code(), ExitCode::Success);
    assert!(
        findings(&report, &plain()).is_empty(),
        "and it annotates nothing"
    );
}

#[test]
fn a_structural_error_with_no_failing_check_exits_one() {
    let report = Report {
        diagnostics: vec![Diagnostic::new(code::E0105, "two pages resolve to `/p`")],
        ..Report::default()
    };
    assert_eq!(report.exit_code(), ExitCode::Errors);
}

#[test]
fn a_run_the_network_stopped_exits_four() {
    let report = Report {
        network_failed: true,
        ..Report::default()
    };
    assert_eq!(report.exit_code(), ExitCode::Network);
}

#[test]
fn a_verification_failure_outranks_a_structural_one() {
    let mut page = PageReport::new(Route::new("/p"));
    page.checks = vec![check("/p#a#0", "http", fail("nope"), Some(Severity::Error))];
    let report = Report {
        pages: vec![page],
        diagnostics: vec![Diagnostic::new(code::E0105, "duplicate")],
        network_failed: true,
    };
    assert_eq!(report.exit_code(), ExitCode::Verification);
}

// ---- the summary ----

#[test]
fn the_summary_counts_every_outcome() {
    let summary = mixed_run().summary();
    assert_eq!(summary.pass, 1);
    assert_eq!(summary.fail, 2, "a Fail and an Error are both failures");
    assert_eq!(summary.skip, 1);
    assert_eq!(summary.drift, 1);
    assert_eq!(summary.total(), 4);
}

// ---- the text report ----

#[test]
fn the_text_report_groups_by_page_and_ends_with_the_counts() {
    let out = mixed_run().render(Format::Text, &plain());
    assert!(out.contains("/guide/install"), "{out}");
    assert!(out.contains("/pricing"), "{out}");
    assert!(out.contains("status is 500, not 200"), "{out}");
    assert!(out.contains("needs a windows runner"), "{out}");
    assert!(out.contains("drift_01"), "{out}");
    assert!(
        out.trim_end()
            .ends_with("1 passed, 2 failed, 1 skipped, 1 drift"),
        "{out}"
    );
}

#[test]
fn a_passing_check_prints_no_detail() {
    let mut page = PageReport::new(Route::new("/p"));
    page.checks = vec![check(
        "/p#a#0",
        "shell",
        CheckOutcome::Pass,
        Some(Severity::Error),
    )];
    let out = Report::new(vec![page]).render(Format::Text, &plain());
    assert_eq!(out.lines().filter(|l| l.starts_with("        ")).count(), 0);
}

#[test]
fn a_drift_age_reads_in_the_largest_whole_unit() {
    let out = mixed_run().render(Format::Text, &plain());
    assert!(out.contains("3d old"), "{out}");
}

// ---- json ----

#[test]
fn the_json_report_round_trips() {
    let report = mixed_run();
    let text = report.render(Format::Json, &plain());
    let back: Report = serde_json::from_str(&text).expect("deserialize");
    assert_eq!(back, report);
}

// ---- scrubbing ----

#[test]
fn every_format_scrubs_before_it_prints() {
    let mut page = PageReport::new(Route::new("/p"));
    page.checks = vec![check(
        "/p#a#0",
        "http",
        fail("Authorization: Bearer swordfish-1234567890"),
        Some(Severity::Error),
    )];
    let report = Report::new(vec![page]);
    let scrubber = Scrubber::with_secrets(["swordfish-1234567890"]);
    for format in [Format::Text, Format::Json, Format::Sarif, Format::Junit] {
        let out = report.render(format, &scrubber);
        assert!(!out.contains("swordfish"), "{}:\n{out}", format.as_str());
    }
}

// ---- formats ----

#[test]
fn every_documented_format_name_parses() {
    for (text, want) in [
        ("text", Format::Text),
        ("json", Format::Json),
        ("sarif", Format::Sarif),
        ("junit", Format::Junit),
        ("SARIF", Format::Sarif),
    ] {
        assert_eq!(Format::parse(text), Some(want), "{text}");
    }
    assert_eq!(Format::parse("tap"), None);
}

// ---- sarif detail ----

#[test]
fn a_sarif_rule_is_declared_once_per_code_and_links_to_its_page() {
    let document = sarif::document(&mixed_run(), &plain());
    let rules = document["runs"][0]["tool"]["driver"]["rules"]
        .as_array()
        .expect("rules");
    let ids: Vec<&str> = rules.iter().filter_map(|r| r["id"].as_str()).collect();
    assert!(ids.contains(&"E0601"), "{ids:?}");
    assert!(ids.contains(&"E0603"), "{ids:?}");
    assert!(ids.contains(&"W0630"), "{ids:?}");
    assert_eq!(
        ids.len(),
        ids.iter().collect::<std::collections::BTreeSet<_>>().len(),
        "no rule is declared twice"
    );
    for rule in rules {
        let help = rule["helpUri"].as_str().unwrap_or_default();
        assert!(help.starts_with("https://"), "{help}");
    }
}

#[test]
fn a_skip_is_not_a_sarif_result() {
    let document = sarif::document(&mixed_run(), &plain());
    let results = document["runs"][0]["results"].as_array().expect("results");
    let messages: Vec<&str> = results
        .iter()
        .filter_map(|r| r["message"]["text"].as_str())
        .collect();
    assert!(
        !messages.iter().any(|m| m.contains("windows runner")),
        "{messages:?}"
    );
}

#[test]
fn a_sarif_location_is_the_source_file_when_there_is_one() {
    let document = sarif::document(&mixed_run(), &plain());
    let results = document["runs"][0]["results"].as_array().expect("results");
    let uris: Vec<&str> = results
        .iter()
        .filter_map(|r| r["locations"][0]["physicalLocation"]["artifactLocation"]["uri"].as_str())
        .collect();
    assert!(uris.contains(&"guide/install.md"), "{uris:?}");
}

#[test]
fn a_page_without_a_source_file_falls_back_to_its_route() {
    let mut page = PageReport::new(Route::new("/generated/api"));
    page.diagnostics = vec![Diagnostic::new(code::W0630, "no description")];
    let document = sarif::document(&Report::new(vec![page]), &plain());
    assert_eq!(
        document["runs"][0]["results"][0]["locations"][0]["physicalLocation"]["artifactLocation"]["uri"],
        json!("generated/api")
    );
}

// ---- junit detail ----

#[test]
fn a_junit_error_and_a_junit_failure_are_counted_apart() {
    let xml = mixed_run().render(Format::Junit, &plain());
    assert!(xml.contains("failures=\"1\" errors=\"1\""), "{xml}");
    assert!(xml.contains("<failure "), "{xml}");
    assert!(xml.contains("<error "), "{xml}");
    assert!(xml.contains("<skipped "), "{xml}");
}

#[test]
fn junit_escapes_every_character_xml_reserves() {
    let mut page = PageReport::new(Route::new("/p"));
    page.checks = vec![check(
        "/p#a#0",
        "shell",
        fail("expected <a href=\"x\"> & got 'b'"),
        Some(Severity::Error),
    )];
    let xml = Report::new(vec![page]).render(Format::Junit, &plain());
    assert!(
        xml.contains("&lt;a href=&quot;x&quot;&gt; &amp; got &apos;b&apos;"),
        "{xml}"
    );
    assert!(!xml.contains("<a href"), "{xml}");
}

#[test]
fn a_control_character_never_reaches_the_xml() {
    let mut page = PageReport::new(Route::new("/p"));
    page.checks = vec![check(
        "/p#a#0",
        "shell",
        fail("bell\u{7}and\u{1}nul"),
        Some(Severity::Error),
    )];
    let xml = Report::new(vec![page]).render(Format::Junit, &plain());
    assert!(!xml.contains('\u{7}'), "{xml}");
    assert!(!xml.contains('\u{1}'), "{xml}");
}

#[test]
fn a_junit_failure_message_is_the_first_line_and_the_body_is_all_of_it() {
    let mut page = PageReport::new(Route::new("/p"));
    page.checks = vec![check(
        "/p#a#0",
        "http",
        fail("status is 500, not 200\nheader `x` is absent"),
        Some(Severity::Error),
    )];
    let xml = Report::new(vec![page]).render(Format::Junit, &plain());
    assert!(xml.contains("message=\"status is 500, not 200\""), "{xml}");
    assert!(xml.contains("header `x` is absent</failure>"), "{xml}");
}

#[test]
fn an_empty_report_is_still_well_formed() {
    let report = Report::default();
    let xml = report.render(Format::Junit, &plain());
    assert!(validate_junit(&xml).is_empty(), "{xml}");
    let sarif = sarif::document(&report, &plain());
    assert!(validate_sarif(&sarif).is_empty(), "{sarif:#}");
    assert_eq!(report.exit_code(), ExitCode::Success);
}

// ---- the two schemas ----

/// SARIF 2.1.0's required structure, from §3 of the specification. The full
/// schema is a 200 KB document that belongs under `schemas/`, which is not
/// this package's path; see `NEEDS-INPUT.md`.
fn validate_sarif(document: &Value) -> Vec<String> {
    let mut problems = Vec::new();

    fn require(problems: &mut Vec<String>, condition: bool, what: &str) {
        if !condition {
            problems.push(what.to_owned());
        }
    }

    require(
        &mut problems,
        document["version"] == json!("2.1.0"),
        "version must be 2.1.0",
    );
    require(
        &mut problems,
        document["$schema"].is_string(),
        "$schema must be a string",
    );
    let Some(runs) = document["runs"].as_array() else {
        problems.push("runs must be an array".to_owned());
        return problems;
    };
    require(
        &mut problems,
        !runs.is_empty(),
        "runs must hold at least one run",
    );

    for run in runs {
        let driver = &run["tool"]["driver"];
        require(
            &mut problems,
            driver["name"].is_string(),
            "tool.driver.name is required",
        );
        let rules = driver["rules"].as_array().cloned().unwrap_or_default();
        let declared: std::collections::BTreeSet<&str> =
            rules.iter().filter_map(|r| r["id"].as_str()).collect();
        for rule in &rules {
            require(
                &mut problems,
                rule["id"].is_string(),
                "every rule needs an id",
            );
            require(
                &mut problems,
                rule["shortDescription"]["text"].is_string(),
                "every rule needs a shortDescription.text",
            );
        }
        let Some(results) = run["results"].as_array() else {
            problems.push("run.results must be an array".to_owned());
            continue;
        };
        for result in results {
            require(
                &mut problems,
                result["message"]["text"].is_string(),
                "every result needs a message.text",
            );
            let Some(rule_id) = result["ruleId"].as_str() else {
                problems.push("every result needs a ruleId".to_owned());
                continue;
            };
            require(
                &mut problems,
                declared.contains(rule_id),
                &format!("result points at undeclared rule {rule_id}"),
            );
            require(
                &mut problems,
                matches!(
                    result["level"].as_str(),
                    Some("none" | "note" | "warning" | "error")
                ),
                "level must be one of none, note, warning, error",
            );
            require(
                &mut problems,
                result["locations"][0]["physicalLocation"]["artifactLocation"]["uri"].is_string(),
                "every result needs a physical location",
            );
        }
    }
    problems
}

/// The JUnit shape every CI runner reads: the counts on `testsuites` agree
/// with the elements below it, and the document is well-formed XML.
fn validate_junit(xml: &str) -> Vec<String> {
    let mut problems = Vec::new();
    if !xml.starts_with("<?xml version=\"1.0\" encoding=\"UTF-8\"?>") {
        problems.push("missing the XML declaration".to_owned());
    }
    if !xml.contains("<testsuites ") || !xml.trim_end().ends_with("</testsuites>") {
        problems.push("the root element must be testsuites".to_owned());
    }
    for attribute in ["tests=", "failures=", "errors=", "skipped=", "time="] {
        if !xml.contains(attribute) {
            problems.push(format!("testsuites needs {attribute}"));
        }
    }
    if let Some(problem) = well_formed(xml) {
        problems.push(problem);
    }
    let declared = attribute_of(xml, "<testsuites ", "tests").unwrap_or_default();
    let cases = xml.matches("<testcase ").count();
    if declared != cases {
        problems.push(format!(
            "testsuites says {declared} tests and there are {cases}"
        ));
    }
    problems
}

fn attribute_of(xml: &str, element: &str, name: &str) -> Option<usize> {
    let start = xml.find(element)? + element.len();
    let tail = &xml[start..];
    let end = tail.find('>')?;
    let attributes = &tail[..end];
    let at = attributes.find(&format!("{name}=\""))? + name.len() + 2;
    let rest = &attributes[at..];
    rest[..rest.find('"')?].parse().ok()
}

/// Tags open and close in order, and no raw `<` survives in text.
fn well_formed(xml: &str) -> Option<String> {
    let mut stack: Vec<&str> = Vec::new();
    let mut rest = xml;
    while let Some(at) = rest.find('<') {
        let tail = &rest[at + 1..];
        let end = tail.find('>')?;
        let tag = &tail[..end];
        rest = &tail[end + 1..];
        if tag.starts_with('?') || tag.starts_with('!') {
            continue;
        }
        if let Some(name) = tag.strip_prefix('/') {
            match stack.pop() {
                Some(open) if open == name => {}
                Some(open) => return Some(format!("<{open}> is closed by </{name}>")),
                None => return Some(format!("</{name}> closes nothing")),
            }
        } else if !tag.ends_with('/') {
            stack.push(tag.split_whitespace().next().unwrap_or(tag));
        }
    }
    stack.pop().map(|open| format!("<{open}> is never closed"))
}
