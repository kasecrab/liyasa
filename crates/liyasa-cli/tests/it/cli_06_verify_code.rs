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

/// The one case that runs a check for real, end to end: a `local` sandbox,
/// which VER-03 allows the CLI and forbids the server, and a shell fence that
/// only echoes. It is here because every other case in this file stops before
/// a runner starts, and a wiring that never executes anything would pass them
/// all.
///
/// The pin comes from `liyasa.lock`, which is also the only test that a lock
/// entry reaches `Images` at all: `SandboxRunner` refuses an unpinned image
/// with `E0610` whatever the sandbox, so without the lock neither fence runs.
#[test]
fn a_pinned_runner_runs_the_fence_and_reports_what_it_found() {
    let project = Dir::new("ver-code-local");
    project.write(
        "liyasa.json",
        r#"{"name":"Acme docs","verify":{"runners":{"sandbox":"local"}}}"#,
    );
    project.write(
        "index.md",
        "---\ntitle: Home\ndescription: The home page.\n---\n\n# Home\n\n```sh verify expect=\"hello\"\necho hello\n```\n\n```sh verify expect=\"never\"\necho goodbye\n```\n",
    );

    let unpinned = Run::new(["verify", "--only", "code", "--offline"])
        .cwd(project.path())
        .output();
    assert!(
        unpinned.all().contains("E0610"),
        "an unpinned image is refused before it runs: {}",
        unpinned.all()
    );

    project.write(
        "liyasa.lock",
        "version = 1\n\n[liyasa]\nversion = \"0.1.0\"\n\n[[runners]]\nid = \"shell\"\nimage = \"docker.io/library/alpine\"\ndigest = \"sha256:1111111111111111111111111111111111111111111111111111111111111111\"\n",
    );

    let outcome = Run::new(["verify", "--only", "code", "--offline"])
        .cwd(project.path())
        .output();

    assert!(
        outcome.all().contains("1 passed, 1 failed"),
        "the fences did not run: {}",
        outcome.all()
    );
    assert!(
        outcome.all().contains("does not contain `never`"),
        "the failing fence is not reported: {}",
        outcome.all()
    );
    // CLI-31: a failed check is `Verification`, not `Errors`.
    assert_eq!(outcome.code, Exit::Verification.code(), "{}", outcome.all());
}

/// The case CI hit and this machine could not: a sandbox that assembles, and
/// a page with nothing to check. The class runs, finds no fence, and says so
/// with a count — it must not fall back to "did not run", which was the note
/// for a CLI that could not call the orchestrator at all.
///
/// `local` is what makes this the same on both: it needs no container engine,
/// so the class runs here exactly as `container` makes it run on CI.
#[test]
fn a_page_with_no_fences_reports_a_count_rather_than_a_refusal() {
    let project = Dir::new("ver-code-empty");
    project.write(
        "liyasa.json",
        r#"{"name":"Acme docs","verify":{"runners":{"sandbox":"local"}}}"#,
    );
    project.write(
        "index.md",
        "---\ntitle: Home\ndescription: The home page.\n---\n\n# Home\n\nProse, and no fence.\n",
    );

    let outcome = Run::new(["verify", "--only", "code", "--offline"])
        .cwd(project.path())
        .output();

    assert!(
        outcome
            .all()
            .contains("code: 0 passed, 0 failed, 0 skipped"),
        "{}",
        outcome.all()
    );
    assert!(
        !outcome.all().contains("W0019"),
        "the class ran, so it must not report that it did not: {}",
        outcome.all()
    );
    assert_eq!(outcome.code, Exit::Success.code(), "{}", outcome.all());
}

/// And when the sandbox cannot be assembled, exactly one note comes back.
///
/// Two used to: the class-level one carrying `E0611`, and a second from the
/// loop that reports a class this command never attempted, still saying the
/// orchestrator was waiting for a caller — in the same report that called it.
#[test]
fn a_class_that_could_not_run_is_reported_once() {
    let project = site("ver-code-once", "remote");

    let outcome = Run::new(["verify", "--only", "code", "--offline"])
        .cwd(project.path())
        .output();

    // Counting the code would count twice: the header and the help URL both
    // carry it. The sentence appears once per note.
    assert_eq!(
        outcome.all().matches("`code` did not run").count(),
        1,
        "{}",
        outcome.all()
    );
    assert!(
        !outcome
            .all()
            .contains("to call the verification orchestrator"),
        "the note still says the orchestrator has no caller: {}",
        outcome.all()
    );
}
