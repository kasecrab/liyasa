use std::time::Duration;

use liyasa_core::diagnostics::{Severity, code};
use liyasa_core::ids::BlockId;

use super::*;
use crate::core::scrub::REDACTED;

const OUTPUT: &str = r#"{
  "docs/index.md": [
    {
      "Check": "Google.Passive",
      "Description": "",
      "Line": 2,
      "Link": "https://developers.google.com/style/voice",
      "Match": "was written",
      "Message": "'was written' may be passive voice.",
      "Severity": "warning",
      "Span": [6, 16]
    },
    {
      "Check": "Microsoft.Contractions",
      "Description": "",
      "Line": 1,
      "Link": "",
      "Match": "do not",
      "Message": "Use 'don't' instead of 'do not'.",
      "Severity": "error",
      "Span": [1, 6]
    }
  ]
}"#;

const TEXT: &str = "do not stop\nThis was written by a person.\n";

fn companion() -> Companion {
    Companion::new("ghcr.io/liyasa/vale", "sha256:abc")
}

fn request() -> Request {
    Request {
        config: "StylesPath = styles\n[*.md]\nBasedOnStyles = Google\n".to_owned(),
        styles: vec![(
            VfsPath::new("Google/Passive.yml"),
            Bytes::from_static(b"extends: existence\n"),
        )],
        path: "docs/index.md".to_owned(),
        text: TEXT.to_owned(),
    }
}

fn output(exit: i32, stdout: &str, stderr: &str) -> SandboxOutput {
    SandboxOutput {
        exit,
        stdout: Bytes::copy_from_slice(stdout.as_bytes()),
        stderr: Bytes::copy_from_slice(stderr.as_bytes()),
        duration: Duration::from_millis(12),
    }
}

fn block() -> BlockId {
    BlockId::explicit("b")
}

#[test]
fn a_job_carries_the_config_the_rules_and_the_document() {
    let job = companion().job(&request());

    let files: Vec<&str> = job.files.iter().map(|(path, _)| path.as_str()).collect();
    assert_eq!(
        files,
        [".vale.ini", "styles/Google/Passive.yml", "docs/index.md"]
    );
    assert!(
        job.cmd.contains(&"--output=JSON".to_owned()),
        "{:?}",
        job.cmd
    );
    assert!(
        job.cmd.last().is_some_and(|last| last == "docs/index.md"),
        "the document is the argument: {:?}",
        job.cmd
    );
}

#[test]
fn a_job_never_gets_the_network_or_a_secret() {
    let job = companion().job(&request());
    assert!(!job.network, "a prose linter has nothing to fetch");
    assert!(job.env.is_empty());
    assert_eq!(job.image, "ghcr.io/liyasa/vale");
    assert_eq!(job.digest, "sha256:abc");
}

#[test]
fn the_output_becomes_findings_in_the_order_vale_listed_them() {
    let found = findings(
        &output(1, OUTPUT, ""),
        TEXT,
        block(),
        None,
        &Scrubber::new(),
    )
    .expect("valid output");

    assert_eq!(found.len(), 2);
    assert_eq!(found[0].rule, "Google.Passive");
    assert_eq!(found[0].severity, Severity::Warning);
    assert_eq!(found[0].message, "'was written' may be passive voice.");
    assert_eq!(found[0].found, "was written");
    assert_eq!(
        found[0].link.as_deref(),
        Some("https://developers.google.com/style/voice")
    );
    assert_eq!(found[1].rule, "Microsoft.Contractions");
    assert_eq!(found[1].severity, Severity::Error);
    assert_eq!(found[1].link, None, "an empty link is no link");
}

#[test]
fn a_finding_points_at_the_byte_vale_pointed_at() {
    let found = findings(
        &output(1, OUTPUT, ""),
        TEXT,
        block(),
        None,
        &Scrubber::new(),
    )
    .expect("valid");

    // Line 2, column 9: the `w` of `was written`.
    assert_eq!(&TEXT[found[0].at..found[0].at + 11], "was written");
    assert_eq!(found[1].at, 0);
}

#[test]
fn a_column_is_counted_in_characters_not_bytes() {
    let text = "Es war schön — und dann wurde geschrieben.\n";
    let out = r#"{"a.md":[{"Check":"X.Y","Line":1,"Link":"","Match":"geschrieben","Message":"m","Severity":"warning","Span":[31,41]}]}"#;

    let found =
        findings(&output(1, out, ""), text, block(), None, &Scrubber::new()).expect("valid output");

    assert_eq!(&text[found[0].at..found[0].at + 11], "geschrieben");
}

#[test]
fn a_position_outside_the_text_lands_at_the_start_rather_than_panicking() {
    let out = r#"{"a.md":[{"Check":"X.Y","Line":99,"Link":"","Match":"","Message":"m","Severity":"warning","Span":[400,401]}]}"#;

    let found =
        findings(&output(1, out, ""), TEXT, block(), None, &Scrubber::new()).expect("valid output");

    assert_eq!(found[0].at, 0);
}

#[test]
fn an_empty_result_is_no_findings_and_no_complaint() {
    let found = findings(&output(0, "{}", ""), TEXT, block(), None, &Scrubber::new())
        .expect("valid output");
    assert!(found.is_empty());
}

#[test]
fn an_exit_the_binary_did_not_lint_with_is_reported_not_swallowed() {
    let problem = findings(
        &output(2, "", "E100 [.vale.ini] StylesPath does not exist\n"),
        TEXT,
        block(),
        None,
        &Scrubber::new(),
    )
    .expect_err("exit 2 is an error, not an empty result");

    assert_eq!(problem.code, code::W0636);
    assert!(
        problem.message.contains("StylesPath does not exist"),
        "{}",
        problem.message
    );
}

#[test]
fn output_that_is_not_json_is_reported() {
    let problem = findings(
        &output(1, "not json at all", ""),
        TEXT,
        block(),
        None,
        &Scrubber::new(),
    )
    .expect_err("unreadable output is not an empty result");
    assert_eq!(problem.code, code::W0636);
}

#[test]
fn what_the_binary_printed_is_scrubbed_before_it_reaches_a_diagnostic() {
    let scrubber = Scrubber::with_secrets(["s3cret-build-token-9f2a"]);
    let problem = findings(
        &output(
            2,
            "",
            "cannot reach registry with token s3cret-build-token-9f2a\n",
        ),
        TEXT,
        block(),
        None,
        &scrubber,
    )
    .expect_err("exit 2 is an error");

    assert!(
        !problem.message.contains("s3cret-build-token-9f2a"),
        "{}",
        problem.message
    );
    assert!(problem.message.contains(REDACTED), "{}", problem.message);
}

#[test]
fn with_no_companion_runtime_the_rules_that_did_not_run_are_named() {
    let problem = not_run(
        &[
            Delegated {
                rule: "Google.Passive".to_owned(),
                extends: "sequence".to_owned(),
            },
            Delegated {
                rule: "Microsoft.Readability".to_owned(),
                extends: "readability".to_owned(),
            },
        ],
        "no sandbox is configured",
    )
    .expect("two rules did not run");

    assert_eq!(problem.code, code::W0636);
    assert!(
        problem.message.contains("Google.Passive"),
        "{}",
        problem.message
    );
    assert!(
        problem.message.contains("Microsoft.Readability"),
        "{}",
        problem.message
    );
    assert!(
        problem
            .help
            .is_some_and(|help| help.contains("no sandbox is configured")),
        "the reason is the help"
    );
}

#[test]
fn nothing_delegated_is_nothing_to_report() {
    assert!(not_run(&[], "no sandbox is configured").is_none());
}
