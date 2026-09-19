//! CLI-06, VER-22 and VER-25: the three `liyasa verify` flags that nothing
//! read — `--refresh`, `--no-cache`, `--changed` — and `--allow-commands`.
//!
//! Nothing here leaves the machine: the one declared source points at a
//! loopback address, which `liyasa-net` refuses at connect time (§30.2.3).

use liyasa_cli::Exit;

use crate::support::{Dir, Run};

/// A site with one page whose link goes nowhere, so every run has something
/// to report and a run that reports nothing is visibly wrong.
fn site(name: &str) -> Dir {
    let project = Dir::new(name);
    project.write("liyasa.json", r#"{"name":"Acme docs"}"#);
    project.write(
        "index.md",
        "---\ntitle: Home\ndescription: The home page.\n---\n\n# Home\n\nA [missing page](/nope).\n",
    );
    project
}

fn git(project: &Dir, arguments: &[&str]) -> std::process::Output {
    std::process::Command::new("git")
        .arg("-C")
        .arg(project.path())
        .args(arguments)
        .env("GIT_AUTHOR_NAME", "test")
        .env("GIT_AUTHOR_EMAIL", "test@example.invalid")
        .env("GIT_COMMITTER_NAME", "test")
        .env("GIT_COMMITTER_EMAIL", "test@example.invalid")
        .output()
        .expect("git runs")
}

// --- --no-cache ------------------------------------------------------------

/// The promise is "ignore cached check results". Every run is cold (RFC 0904),
/// so the promise is kept; this pins it, because the day that build becomes
/// warm the flag is what has to keep it.
#[test]
fn a_second_run_reports_what_the_first_did() {
    let project = site("ver-nocache");
    let first = Run::new(["verify", "--only", "links", "--offline"])
        .cwd(project.path())
        .output();
    let second = Run::new(["verify", "--only", "links", "--offline", "--no-cache"])
        .cwd(project.path())
        .output();

    assert_eq!(
        first.code,
        second.code,
        "{}\n---\n{}",
        first.all(),
        second.all()
    );
    assert!(first.all().contains("E0401"), "{}", first.all());
    assert!(second.all().contains("E0401"), "{}", second.all());
}

/// A page fixed between runs is reported as fixed, which is the failure a
/// stale cache would produce.
#[test]
fn a_fixed_page_is_not_reported_from_a_cache() {
    let project = site("ver-nocache-fixed");
    let before = Run::new(["verify", "--only", "links", "--offline"])
        .cwd(project.path())
        .output();
    assert!(before.all().contains("E0401"), "{}", before.all());

    project.write(
        "index.md",
        "---\ntitle: Home\ndescription: The home page.\n---\n\n# Home\n\nNo link at all.\n",
    );
    let after = Run::new(["verify", "--only", "links", "--offline"])
        .cwd(project.path())
        .output();
    assert!(!after.all().contains("E0401"), "{}", after.all());
}

// --- --changed -------------------------------------------------------------

#[test]
fn a_reference_this_repository_does_not_have_is_refused() {
    let project = site("ver-changed-badref");
    git(&project, &["init", "--quiet"]);
    let outcome = Run::new(["verify", "--offline", "--changed", "no-such-ref"])
        .cwd(project.path())
        .output();

    assert_eq!(outcome.code, Exit::Errors.code(), "{}", outcome.all());
    assert!(outcome.all().contains("E0022"), "{}", outcome.all());
}

/// The point of the flag: a page that did not change is not reported.
#[test]
fn only_the_changed_page_is_reported() {
    let project = site("ver-changed");
    project.write(
        "other.md",
        "---\ntitle: Other\ndescription: Another page.\n---\n\n# Other\n\nFine.\n",
    );
    git(&project, &["init", "--quiet"]);
    git(&project, &["add", "-A"]);
    git(&project, &["commit", "--quiet", "-m", "start"]);

    // index.md already has the broken link and is now unchanged; other.md
    // gains one.
    project.write(
        "other.md",
        "---\ntitle: Other\ndescription: Another page.\n---\n\n# Other\n\nA [gone](/also-nope).\n",
    );

    let everything = Run::new(["verify", "--only", "links", "--offline"])
        .cwd(project.path())
        .output();
    assert!(everything.all().contains("/nope"), "{}", everything.all());
    assert!(
        everything.all().contains("/also-nope"),
        "{}",
        everything.all()
    );

    let narrowed = Run::new([
        "verify",
        "--only",
        "links",
        "--offline",
        "--changed",
        "HEAD",
    ])
    .cwd(project.path())
    .output();
    assert!(
        narrowed.all().contains("/also-nope"),
        "the changed page is missing: {}",
        narrowed.all()
    );
    // W0406 carries a span and belongs to `index.md`, which did not change.
    assert!(
        !narrowed.all().contains("index.md"),
        "an unchanged page was located and reported: {}",
        narrowed.all()
    );
    // The link errors carry no span at all, so they are shown and counted.
    assert!(narrowed.all().contains("W0023"), "{}", narrowed.all());
}

/// Without `--changed`, nothing is narrowed and nothing is counted.
#[test]
fn an_unnarrowed_run_does_not_warn_about_narrowing() {
    let project = site("ver-unnarrowed");
    let outcome = Run::new(["verify", "--only", "links", "--offline"])
        .cwd(project.path())
        .output();
    assert!(!outcome.all().contains("W0023"), "{}", outcome.all());
}

#[test]
fn the_flags_are_in_the_help() {
    let outcome = Run::new(["verify", "--help"]).output();
    assert_eq!(outcome.code, Exit::Success.code(), "{}", outcome.all());
    for flag in ["--refresh", "--no-cache", "--changed"] {
        assert!(outcome.stdout.contains(flag), "{}", outcome.stdout);
    }
}
