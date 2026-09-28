//! CLI-06 and VER-01 to VER-03: `liyasa verify --only code` runs the fences.
//!
//! Nothing here starts a container. Every case names a sandbox the machine
//! cannot provide, because the question these tests answer is what the CLI
//! does with the orchestrator and the registry, and a test that depended on
//! Podman being installed would answer it only on the machines that have it.

use liyasa_cli::Exit;

use crate::support::{Dir, Run};

/// A page with one verified fence, and a sandbox setting to go with it.
fn site(name: &str, sandbox: &str) -> Dir {
    let project = Dir::new(name);
    project.write(
        "liyasa.json",
        &format!(r#"{{"name":"Acme docs","verify":{{"runners":{{"sandbox":"{sandbox}"}}}}}}"#),
    );
    project.write(
        "index.md",
        "---\ntitle: Home\ndescription: The home page.\n---\n\n# Home\n\n```python verify\nprint(\"hello\")\n```\n",
    );
    project
}

/// `remote` with no service configured is `E0611`, and the class-level note
/// carries it rather than replacing it with a summary of its own.
#[test]
fn a_sandbox_that_cannot_be_assembled_says_which_one_and_why() {
    let project = site("ver-code-remote", "remote");

    let outcome = Run::new(["verify", "--only", "code", "--offline"])
        .cwd(project.path())
        .output();

    assert!(outcome.all().contains("W0019"), "{}", outcome.all());
    assert!(
        outcome.all().contains("no runner service is configured"),
        "the real reason is missing: {}",
        outcome.all()
    );
}

/// A class that could not run is a warning, not a failure. §14 says the other
/// classes still run on a machine with no sandbox, and an exit code that says
/// otherwise would make `liyasa verify` unusable there.
#[test]
fn a_missing_sandbox_does_not_fail_the_run() {
    let project = site("ver-code-exit", "remote");

    let outcome = Run::new(["verify", "--only", "code", "--offline"])
        .cwd(project.path())
        .output();

    assert_eq!(outcome.code, Exit::Success.code(), "{}", outcome.all());
}

/// The JSON report carries the same pair, so a consumer sees the code that
/// explains the note and not only the note.
#[test]
fn the_json_report_carries_the_reason_as_well_as_the_note() {
    let project = site("ver-code-json", "remote");

    let outcome = Run::new(["verify", "--only", "code", "--offline", "--format", "json"])
        .cwd(project.path())
        .output();

    let json: serde_json::Value = serde_json::from_str(&outcome.stdout)
        .unwrap_or_else(|error| panic!("{error}: {}", outcome.all()));
    let text = json.to_string();
    assert!(text.contains("W0019"), "{text}");
    assert!(text.contains("E0611"), "{text}");
}
