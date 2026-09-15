use liyasa_core::conformance;
use liyasa_core::verify::{CheckInput, CheckOutcome, Expectation, Isolation, Runner};

use super::testing::{NoSandbox, Secrets, code, expect_text, spec};
use super::*;

fn run(runner: &dyn Runner, spec: &CheckSpec) -> CheckResult {
    conformance::block_on(runner.run(spec, &NoSandbox, &Secrets::default()))
}

// ---- the registry ----

#[test]
fn the_in_process_set_claims_the_languages_ver_02_names() {
    let registry = in_process();
    for lang in ["json", "yaml", "yml", "mermaid", "regex"] {
        assert!(
            registry.for_language(lang).is_some(),
            "no runner for {lang}"
        );
    }
    assert!(
        registry.for_language("rust").is_none(),
        "rust needs a sandbox"
    );
}

#[test]
fn a_language_is_matched_without_regard_to_case_or_padding() {
    let registry = in_process();
    assert!(registry.for_language("  JSON ").is_some());
}

#[test]
fn every_in_process_runner_says_so() {
    for runner in [
        &schema::SchemaRunner as &dyn Runner,
        &mermaid::MermaidRunner,
        &regex::RegexRunner,
    ] {
        assert_eq!(runner.isolation(), Isolation::InProcess, "{}", runner.id());
    }
}

#[test]
fn a_runner_is_reachable_by_its_id() {
    let registry = in_process();
    assert_eq!(registry.by_id("mermaid").map(Runner::id), Some("mermaid"));
    assert!(registry.by_id("cargo").is_none());
}

#[test]
fn an_unclaimed_language_names_itself_in_e0602() {
    let diagnostic = no_runner("brainfuck");
    assert_eq!(diagnostic.code, code::E0602);
    assert!(diagnostic.message.contains("brainfuck"));
}

// ---- the conformance kit, once per runner ----

#[test]
fn the_regex_runner_is_a_runner() {
    conformance::runner::check(
        &regex::RegexRunner,
        &NoSandbox,
        &Secrets::default(),
        &conformance::runner::Fixture {
            passing: code("regex", r"^\d{3}-\d{4}$"),
            failing: code("regex", r"^[a-z]+$"),
            expect: vec![expect_text("555-0199")],
        },
    );
}

#[test]
fn the_mermaid_runner_is_a_runner() {
    conformance::runner::check(
        &mermaid::MermaidRunner,
        &NoSandbox,
        &Secrets::default(),
        &conformance::runner::Fixture {
            passing: code("mermaid", "flowchart LR\n  A[Start] --> B[End]"),
            failing: code("mermaid", "flowchart LR\n  A[Start --> B[End]"),
            expect: Vec::new(),
        },
    );
}

#[test]
fn the_schema_runner_is_a_runner() {
    conformance::runner::check(
        &schema::SchemaRunner,
        &NoSandbox,
        &Secrets::default(),
        &conformance::runner::Fixture {
            passing: CheckInput::Schema {
                lang: "json".to_owned(),
                source: r#"{"port": 8080}"#.to_owned(),
                schema: r#"{"type":"object","properties":{"port":{"type":"integer"}}}"#.to_owned(),
            },
            failing: CheckInput::Schema {
                lang: "json".to_owned(),
                source: r#"{"port": "eighty-eighty"}"#.to_owned(),
                schema: r#"{"type":"object","properties":{"port":{"type":"integer"}}}"#.to_owned(),
            },
            expect: Vec::new(),
        },
    );
}

// ---- results ----

#[test]
fn a_result_carries_the_id_of_the_spec_that_ran() {
    let check = spec("/p#b#0", code("regex", "^a$"), vec![expect_text("a")]);
    assert_eq!(run(&regex::RegexRunner, &check).id, check.id);
}

#[test]
fn the_digest_ignores_how_long_the_check_took() {
    let check = spec("/p#b#0", code("regex", "^a$"), vec![expect_text("a")]);
    let first = run(&regex::RegexRunner, &check);
    let second = run(&regex::RegexRunner, &check);
    assert_eq!(first.digest, second.digest);
}

#[test]
fn the_digest_changes_when_the_outcome_does() {
    let passing = spec("/p#b#0", code("regex", "^a$"), vec![expect_text("a")]);
    let failing = spec("/p#b#0", code("regex", "^a$"), vec![expect_text("b")]);
    assert_ne!(
        run(&regex::RegexRunner, &passing).digest,
        run(&regex::RegexRunner, &failing).digest
    );
}

#[test]
fn two_runners_digest_the_same_spec_differently() {
    let check = spec("/p#b#0", code("mermaid", "pie\n  \"a\" : 1"), Vec::new());
    let one = run(&mermaid::MermaidRunner, &check);
    let other = run(&regex::RegexRunner, &check);
    assert_ne!(one.digest, other.digest);
}

#[test]
fn a_secret_the_check_declared_never_reaches_the_excerpt() {
    let mut check = spec(
        "/p#b#0",
        code("regex", "^(swordfish-1234567890)$"),
        vec![expect_text("nothing like it")],
    );
    check.needs_secrets = vec!["token".to_owned()];
    let secrets = Secrets(vec![(
        "token".to_owned(),
        "swordfish-1234567890".to_owned(),
    )]);
    let result = conformance::block_on(regex::RegexRunner.run(&check, &NoSandbox, &secrets));
    match result.outcome {
        CheckOutcome::Fail { excerpt } => {
            assert!(!excerpt.contains("swordfish"), "{excerpt}");
        }
        other => panic!("expected a failure, got {other:?}"),
    }
}

// ---- the regex runner (VER-02.5) ----

#[test]
fn a_pattern_that_matches_every_expect_passes() {
    let check = spec(
        "/p#b#0",
        code("regex", r"^\d{3}-\d{4}$"),
        vec![expect_text("555-0199"), expect_text("212-5309")],
    );
    assert_eq!(run(&regex::RegexRunner, &check).outcome, CheckOutcome::Pass);
}

#[test]
fn a_pattern_that_misses_one_expect_fails_and_names_it() {
    let check = spec(
        "/p#b#0",
        code("regex", r"^\d{3}-\d{4}$"),
        vec![expect_text("555-0199"), expect_text("not a number")],
    );
    match run(&regex::RegexRunner, &check).outcome {
        CheckOutcome::Fail { excerpt } => assert!(excerpt.contains("not a number"), "{excerpt}"),
        other => panic!("{other:?}"),
    }
}

#[test]
fn a_pattern_that_does_not_compile_fails_with_the_compiler_message() {
    let check = spec("/p#b#0", code("regex", "(unclosed"), Vec::new());
    match run(&regex::RegexRunner, &check).outcome {
        CheckOutcome::Fail { excerpt } => assert!(excerpt.contains("compile"), "{excerpt}"),
        other => panic!("{other:?}"),
    }
}

#[test]
fn a_pattern_with_no_expect_asserts_only_that_it_compiles() {
    let check = spec("/p#b#0", code("regex", r"^\w+$"), Vec::new());
    assert_eq!(run(&regex::RegexRunner, &check).outcome, CheckOutcome::Pass);
}

#[test]
fn an_empty_regex_block_is_an_authoring_error() {
    let check = spec("/p#b#0", code("regex", "  \n "), Vec::new());
    match run(&regex::RegexRunner, &check).outcome {
        CheckOutcome::Error(diagnostic) => assert_eq!(diagnostic.code, code::E0601),
        other => panic!("{other:?}"),
    }
}

// ---- the schema runner (VER-02.3) ----

#[test]
fn yaml_is_validated_against_a_json_schema() {
    let check = spec(
        "/p#b#0",
        CheckInput::Schema {
            lang: "yaml".to_owned(),
            source: "port: 8080\nhost: localhost\n".to_owned(),
            schema: r#"{"type":"object","required":["port","host"]}"#.to_owned(),
        },
        Vec::new(),
    );
    assert_eq!(
        run(&schema::SchemaRunner, &check).outcome,
        CheckOutcome::Pass
    );
}

#[test]
fn a_schema_may_itself_be_written_in_yaml() {
    let check = spec(
        "/p#b#0",
        CheckInput::Schema {
            lang: "json".to_owned(),
            source: r#"{"port": 8080}"#.to_owned(),
            schema: "type: object\nrequired: [port]\n".to_owned(),
        },
        Vec::new(),
    );
    assert_eq!(
        run(&schema::SchemaRunner, &check).outcome,
        CheckOutcome::Pass
    );
}

#[test]
fn a_failing_document_reports_the_pointer_to_the_bad_value() {
    let check = spec(
        "/p#b#0",
        CheckInput::Schema {
            lang: "json".to_owned(),
            source: r#"{"port": "eighty"}"#.to_owned(),
            schema: r#"{"type":"object","properties":{"port":{"type":"integer"}}}"#.to_owned(),
        },
        Vec::new(),
    );
    match run(&schema::SchemaRunner, &check).outcome {
        CheckOutcome::Fail { excerpt } => assert!(excerpt.contains("port"), "{excerpt}"),
        other => panic!("{other:?}"),
    }
}

#[test]
fn a_block_that_is_not_its_language_fails_before_the_schema_runs() {
    let check = spec(
        "/p#b#0",
        CheckInput::Schema {
            lang: "json".to_owned(),
            source: "{not json".to_owned(),
            schema: r#"{"type":"object"}"#.to_owned(),
        },
        Vec::new(),
    );
    match run(&schema::SchemaRunner, &check).outcome {
        CheckOutcome::Fail { excerpt } => assert!(excerpt.contains("not JSON"), "{excerpt}"),
        other => panic!("{other:?}"),
    }
}

#[test]
fn a_block_without_a_schema_asserts_only_that_it_parses() {
    let check = spec("/p#b#0", code("yaml", "a: 1\nb: [2, 3]\n"), Vec::new());
    assert_eq!(
        run(&schema::SchemaRunner, &check).outcome,
        CheckOutcome::Pass
    );

    let broken = spec("/p#b#0", code("yaml", "a: [1, 2\n"), Vec::new());
    assert!(matches!(
        run(&schema::SchemaRunner, &broken).outcome,
        CheckOutcome::Fail { .. }
    ));
}

#[test]
fn an_unreadable_schema_is_the_authors_error_not_the_documents() {
    let check = spec(
        "/p#b#0",
        CheckInput::Schema {
            lang: "json".to_owned(),
            source: "{}".to_owned(),
            schema: r#"{"type": 12}"#.to_owned(),
        },
        Vec::new(),
    );
    match run(&schema::SchemaRunner, &check).outcome {
        CheckOutcome::Error(diagnostic) => assert_eq!(diagnostic.code, code::E0601),
        other => panic!("{other:?}"),
    }
}

// ---- the mermaid runner (VER-02.4) ----

#[test]
fn every_documented_diagram_type_parses() {
    let diagrams = [
        "flowchart LR\n  A --> B",
        "graph TD\n  A --> B",
        "sequenceDiagram\n  Alice->>Bob: Hello",
        "classDiagram\n  Animal <|-- Duck",
        "stateDiagram-v2\n  [*] --> Still",
        "erDiagram\n  CUSTOMER ||--o{ ORDER : places",
        "journey\n  title My day\n  Wake: 5: Me",
        "gantt\n  title A\n  section S\n  Task :a1, 2026-01-01, 30d",
        "pie\n  \"Dogs\" : 386",
        "mindmap\n  root((liyasa))",
        "timeline\n  title History\n  2026 : Liyasa",
        "gitGraph\n  commit",
    ];
    for source in diagrams {
        let check = spec("/p#b#0", code("mermaid", source), Vec::new());
        assert_eq!(
            run(&mermaid::MermaidRunner, &check).outcome,
            CheckOutcome::Pass,
            "{source}"
        );
    }
}

#[test]
fn an_unknown_diagram_type_fails() {
    let check = spec(
        "/p#b#0",
        code("mermaid", "flowhcart LR\n  A --> B"),
        Vec::new(),
    );
    match run(&mermaid::MermaidRunner, &check).outcome {
        CheckOutcome::Fail { excerpt } => assert!(excerpt.contains("flowhcart"), "{excerpt}"),
        other => panic!("{other:?}"),
    }
}

#[test]
fn a_flowchart_without_a_direction_fails() {
    let check = spec(
        "/p#b#0",
        code("mermaid", "flowchart\n  A --> B"),
        Vec::new(),
    );
    assert!(matches!(
        run(&mermaid::MermaidRunner, &check).outcome,
        CheckOutcome::Fail { .. }
    ));
}

#[test]
fn an_unclosed_bracket_fails_and_names_its_line() {
    let check = spec(
        "/p#b#0",
        code("mermaid", "flowchart LR\n  A[Start --> B[End]"),
        Vec::new(),
    );
    match run(&mermaid::MermaidRunner, &check).outcome {
        CheckOutcome::Fail { excerpt } => assert!(excerpt.contains("line 2"), "{excerpt}"),
        other => panic!("{other:?}"),
    }
}

#[test]
fn a_bracket_inside_a_quoted_label_is_not_a_problem() {
    let check = spec(
        "/p#b#0",
        code("mermaid", "flowchart LR\n  A[\"a [b] c\"] --> B"),
        Vec::new(),
    );
    assert_eq!(
        run(&mermaid::MermaidRunner, &check).outcome,
        CheckOutcome::Pass
    );
}

#[test]
fn an_unclosed_quote_fails() {
    let check = spec(
        "/p#b#0",
        code("mermaid", "flowchart LR\n  A[\"never closed] --> B"),
        Vec::new(),
    );
    assert!(matches!(
        run(&mermaid::MermaidRunner, &check).outcome,
        CheckOutcome::Fail { .. }
    ));
}

#[test]
fn a_sequence_arrow_that_mermaid_does_not_have_fails() {
    let check = spec(
        "/p#b#0",
        code("mermaid", "sequenceDiagram\n  Alice=>>Bob: Hello"),
        Vec::new(),
    );
    match run(&mermaid::MermaidRunner, &check).outcome {
        CheckOutcome::Fail { excerpt } => assert!(excerpt.contains("arrow"), "{excerpt}"),
        other => panic!("{other:?}"),
    }
}

#[test]
fn every_sequence_arrow_mermaid_does_have_passes() {
    for arrow in ["->", "-->", "->>", "-->>", "-x", "--x", "-)", "--)"] {
        let source = format!("sequenceDiagram\n  Alice{arrow}Bob: Hello");
        let check = spec("/p#b#0", code("mermaid", &source), Vec::new());
        assert_eq!(
            run(&mermaid::MermaidRunner, &check).outcome,
            CheckOutcome::Pass,
            "{arrow}"
        );
    }
}

#[test]
fn a_sequence_keyword_line_is_not_read_as_a_message() {
    let source =
        "sequenceDiagram\n  participant Alice\n  Note over Alice: thinking\n  Alice->>Bob: Hi";
    let check = spec("/p#b#0", code("mermaid", source), Vec::new());
    assert_eq!(
        run(&mermaid::MermaidRunner, &check).outcome,
        CheckOutcome::Pass
    );
}

#[test]
fn comments_and_directives_are_not_statements() {
    let source = "%%{init: {'theme':'dark'}}%%\nflowchart LR\n%% a comment\n  A --> B";
    let check = spec("/p#b#0", code("mermaid", source), Vec::new());
    assert_eq!(
        run(&mermaid::MermaidRunner, &check).outcome,
        CheckOutcome::Pass
    );
}

#[test]
fn a_header_with_no_statements_fails() {
    let check = spec("/p#b#0", code("mermaid", "flowchart LR\n"), Vec::new());
    match run(&mermaid::MermaidRunner, &check).outcome {
        CheckOutcome::Fail { excerpt } => assert!(excerpt.contains("no statements"), "{excerpt}"),
        other => panic!("{other:?}"),
    }
}

#[test]
fn an_empty_mermaid_block_is_an_authoring_error() {
    let check = spec("/p#b#0", code("mermaid", "\n\n"), Vec::new());
    match run(&mermaid::MermaidRunner, &check).outcome {
        CheckOutcome::Error(diagnostic) => assert_eq!(diagnostic.code, code::E0601),
        other => panic!("{other:?}"),
    }
}

// ---- shared helpers ----

#[test]
fn expect_collects_only_the_stdout_expectations() {
    let check = spec(
        "/p#b#0",
        code("regex", "x"),
        vec![expect_text("a"), Expectation::Exit(0), expect_text("b")],
    );
    assert_eq!(expected_text(&check), ["a", "b"]);
}

#[test]
fn a_timeout_is_e0603() {
    match timed_out(std::time::Duration::from_millis(250)) {
        CheckOutcome::Error(diagnostic) => {
            assert_eq!(diagnostic.code, code::E0603);
            assert!(diagnostic.message.contains("250"));
        }
        other => panic!("{other:?}"),
    }
}
