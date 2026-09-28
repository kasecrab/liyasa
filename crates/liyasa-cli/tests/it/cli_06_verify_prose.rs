//! CLI-06 and VER-60/VER-61: `liyasa verify --only prose`.

use liyasa_cli::Exit;

use crate::support::{Dir, Run};

fn site(name: &str, body: &str) -> Dir {
    let project = Dir::new(name);
    project.write("liyasa.json", r#"{"name":"Acme docs"}"#);
    project.write(
        "index.md",
        &format!("---\ntitle: Home\ndescription: The home page.\n---\n\n# Home\n\n{body}\n"),
    );
    project
}

/// With no `.vale.ini` and no `styles/`, the bundled rules are what runs.
#[test]
fn the_bundled_rules_run_against_a_page() {
    let project = site(
        "prose-bundled",
        "Restart the server in order to apply the change.",
    );

    let outcome = Run::new(["verify", "--only", "prose", "--offline"])
        .cwd(project.path())
        .output();

    assert!(outcome.all().contains("W0631"), "{}", outcome.all());
    assert!(outcome.all().contains("in order to"), "{}", outcome.all());
    // A style finding is a warning: it does not fail the run on its own.
    assert_eq!(outcome.code, Exit::Success.code(), "{}", outcome.all());
}

/// Prose that breaks no rule reports nothing, which is the half that says the
/// pass is reading the page rather than matching everything.
#[test]
fn a_page_that_breaks_no_rule_reports_nothing() {
    let project = site("prose-clean", "Restart the server to apply the change.");

    let outcome = Run::new(["verify", "--only", "prose", "--offline"])
        .cwd(project.path())
        .output();

    assert!(!outcome.all().contains("W0631"), "{}", outcome.all());
    assert!(
        !outcome.all().contains("W0019"),
        "`prose` runs now, so it must not report that it did not: {}",
        outcome.all()
    );
}

/// The count is printed once for the run, not once per page.
#[test]
fn the_run_says_how_many_findings_it_had() {
    let project = site("prose-count", "This is very simply a test.");

    let outcome = Run::new(["verify", "--only", "prose", "--offline"])
        .cwd(project.path())
        .output();

    assert!(
        outcome.all().contains("prose: 2 findings"),
        "{}",
        outcome.all()
    );
}
