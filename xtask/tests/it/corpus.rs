//! The corpus file format and the harness's own rules (PRD §30.9).

use std::path::Path;

use xtask::conformance::{self, Verdict};
use xtask::corpus::{self, Case, CaseHeader, ExpectedDiagnostic};
use xtask::spike::engines;

const CASE: &str = "\
%%% case
{\"id\":\"cm-50/containers/example\",\"requirement\":\"CM-50\",\"tags\":[\"directive\"]}
%%% source
:::note
body
:::
%%% html
<div class=\"note\">
<p>body</p>
</div>
%%% diagnostics
[{\"code\":\"W0319\",\"line\":2}]
%%% end
";

fn parse(text: &str) -> Case {
    corpus::parse(Path::new("case.md"), text).expect("parses")
}

#[test]
fn a_case_parses_into_its_sections() {
    let case = parse(CASE);
    assert_eq!(case.header.id, "cm-50/containers/example");
    assert_eq!(case.header.requirement.as_deref(), Some("CM-50"));
    assert_eq!(case.source, ":::note\nbody\n:::\n");
    assert_eq!(
        case.html.as_deref(),
        Some("<div class=\"note\">\n<p>body</p>\n</div>\n")
    );
    assert_eq!(
        case.diagnostics,
        Some(vec![ExpectedDiagnostic {
            code: "W0319".to_owned(),
            line: Some(2),
            col: None,
            message: None,
        }])
    );
    assert_eq!(case.asserted(), ["html", "diagnostics"]);
}

#[test]
fn writing_a_case_round_trips() {
    let case = parse(CASE);
    let written = corpus::write(&case).expect("writes");
    assert_eq!(parse(&written), case);
}

#[test]
fn a_delimiter_is_only_a_known_section_name() {
    // `%%% note` is content, not a delimiter, so no case ever needs escaping.
    let case = parse("%%% case\n{\"id\":\"x\"}\n%%% source\n%%% note\nbody\n%%% end\n");
    assert_eq!(case.source, "%%% note\nbody\n");
}

#[test]
fn a_case_without_a_header_is_rejected() {
    let error =
        corpus::parse(Path::new("case.md"), "%%% source\nbody\n%%% end\n").expect_err("must fail");
    assert!(error.contains("no `%%% case` section"), "{error}");
}

#[test]
fn a_case_without_a_source_is_rejected() {
    let error = corpus::parse(Path::new("case.md"), "%%% case\n{\"id\":\"x\"}\n%%% end\n")
        .expect_err("must fail");
    assert!(error.contains("no `%%% source` section"), "{error}");
}

#[test]
fn content_before_the_first_section_is_rejected() {
    let error = corpus::parse(Path::new("case.md"), "stray\n%%% case\n{\"id\":\"x\"}\n")
        .expect_err("must fail");
    assert!(error.contains("content before"), "{error}");
}

#[test]
fn a_malformed_header_names_the_file() {
    let error = corpus::parse(Path::new("bad.md"), "%%% case\nnot json\n%%% source\nx\n")
        .expect_err("must fail");
    assert!(error.contains("bad.md"), "{error}");
    assert!(error.contains("not valid JSON"), "{error}");
}

fn run(case: Case) -> Verdict {
    let engine = engines::by_name("comrak-directive").expect("engine exists");
    let report = conformance::run(
        std::slice::from_ref(&case),
        &conformance::Options {
            engine: engine.as_ref(),
            filter: None,
            verbose: false,
        },
    );
    report.outcomes.first().expect("one outcome").verdict
}

#[test]
fn a_matching_expectation_passes() {
    let mut case = parse(CASE);
    case.diagnostics = None; // the engine does not produce diagnostics
    assert_eq!(run(case), Verdict::Pass);
}

#[test]
fn a_wrong_expectation_fails() {
    let mut case = parse(CASE);
    case.diagnostics = None;
    case.html = Some("<p>not this</p>\n".to_owned());
    assert_eq!(run(case), Verdict::Fail);
}

#[test]
fn an_expectation_the_engine_cannot_produce_is_skipped_not_passed() {
    let mut case = parse(CASE);
    case.html = None;
    case.ast = Some(serde_json::json!({"node": "block"}));
    assert_eq!(
        run(case),
        Verdict::Skip,
        "an unproduced expectation must never read as a pass"
    );
}

#[test]
fn a_pending_case_that_fails_is_pending() {
    let mut case = parse(CASE);
    case.diagnostics = None;
    case.html = Some("<p>not this</p>\n".to_owned());
    case.header.pending = Some("known".to_owned());
    assert_eq!(run(case), Verdict::Pending);
}

#[test]
fn a_pending_case_that_passes_fails_so_the_list_cannot_go_stale() {
    let mut case = parse(CASE);
    case.diagnostics = None;
    case.header.pending = Some("known".to_owned());
    assert_eq!(run(case), Verdict::Fail);
}

#[test]
fn a_case_that_asserts_nothing_fails_rather_than_passing_forever() {
    let mut case = parse(CASE);
    case.html = None;
    case.diagnostics = None;
    assert!(case.asserted().is_empty());
    assert_eq!(
        run(case),
        Verdict::Fail,
        "an expectation-free case must not read as a pass"
    );
}

#[test]
fn the_review_sample_is_deterministic_and_about_the_right_size() {
    let cases: Vec<Case> = (0..1000)
        .map(|n| Case {
            path: Path::new("case.md").to_path_buf(),
            header: CaseHeader {
                id: format!("case/{n:04}"),
                ..CaseHeader::default()
            },
            source: String::new(),
            html: None,
            markdown: None,
            ast: None,
            source_document: None,
            diagnostics: None,
        })
        .collect();
    let first = xtask::corpus_seed::review_sample(&cases, 20);
    let second = xtask::corpus_seed::review_sample(&cases, 20);
    let ids = |s: &[&Case]| s.iter().map(|c| c.header.id.clone()).collect::<Vec<_>>();
    assert_eq!(ids(&first), ids(&second), "the sample must be reproducible");
    assert!(
        (150..=250).contains(&first.len()),
        "20% of 1000 should be near 200, got {}",
        first.len()
    );
}

#[test]
fn every_engine_produces_something_it_claims_to() {
    let case = parse(CASE);
    for engine in engines::all() {
        let outputs = engine
            .run(&case)
            .unwrap_or_else(|e| panic!("{}: {e}", engine.name()));
        for kind in engine.produces() {
            let present = match *kind {
                "html" => outputs.html.is_some(),
                "markdown" => outputs.markdown.is_some(),
                "ast" => outputs.ast.is_some(),
                "source-document" => outputs.source_document.is_some(),
                "diagnostics" => outputs.diagnostics.is_some(),
                other => panic!("{}: unknown expectation `{other}`", engine.name()),
            };
            assert!(
                present,
                "{} claims to produce `{kind}` and did not",
                engine.name()
            );
        }
    }
}
