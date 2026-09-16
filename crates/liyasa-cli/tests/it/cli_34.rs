//! CLI-34: every command honours `LIYASA_*` and `--config`, and prints a plan
//! under `--dry-run` where it has side effects.

use liyasa_cli::Exit;

use crate::support::{Dir, Run};

fn site(name: &str) -> Dir {
    let project = Dir::new(name);
    project
        .write(
            "liyasa.json",
            r#"{"name":"Acme docs","seo":{"canonicalOrigin":"https://docs.acme.com"}}"#,
        )
        .write("index.md", "---\ntitle: Home\n---\n\n# Home\n\nWelcome.\n")
        .write(
            "guides/install.md",
            "---\ntitle: Install\n---\n\n# Install\n\nRun it.\n",
        );
    project
}

/// The acceptance criterion as written: the plan is printed and `dist/` is
/// untouched. The output directory named by the environment is untouched too,
/// which is the part that would otherwise be easy to get wrong.
#[test]
fn a_dry_run_build_prints_the_plan_and_writes_nothing() {
    let project = site("cli34-dry-run");
    let outcome = Run::new(["build", "--dry-run"])
        .cwd(project.path())
        .env("LIYASA_OUTPUT", "out")
        .output();

    assert_eq!(outcome.code, Exit::Success.code(), "{}", outcome.all());
    assert!(
        outcome.stdout.contains("plan"),
        "no plan in: {}",
        outcome.stdout
    );
    assert!(
        outcome.stdout.contains("out"),
        "the plan does not name the output directory: {}",
        outcome.stdout
    );
    assert!(
        outcome.stdout.contains("pages"),
        "the plan does not say how many pages: {}",
        outcome.stdout
    );

    assert!(
        !project.path().join("dist").exists(),
        "a dry run created dist/"
    );
    assert!(
        !project.path().join("out").exists(),
        "a dry run created the output directory"
    );
}

/// `LIYASA_OUTPUT` is where a real build goes, so the dry run above was
/// describing the build this one performs.
#[test]
fn the_environment_names_the_output_directory() {
    let project = site("cli34-env-output");
    let outcome = Run::new(["build"])
        .cwd(project.path())
        .env("LIYASA_OUTPUT", "out")
        .output();

    assert_eq!(outcome.code, Exit::Success.code(), "{}", outcome.all());
    assert!(
        project.path().join("out").is_dir(),
        "LIYASA_OUTPUT was ignored: {}",
        outcome.all()
    );
    assert!(
        !project.path().join("dist").exists(),
        "the build wrote dist/ as well as the requested directory"
    );
}

/// A flag beats the environment, which is the direction every other tool takes.
#[test]
fn the_flag_wins_over_the_environment() {
    let project = site("cli34-flag-wins");
    let outcome = Run::new(["build", "--output", "flag"])
        .cwd(project.path())
        .env("LIYASA_OUTPUT", "env")
        .output();

    assert_eq!(outcome.code, Exit::Success.code(), "{}", outcome.all());
    assert!(project.path().join("flag").is_dir(), "{}", outcome.all());
    assert!(!project.path().join("env").exists());
}

/// `--config` finds a project the working directory is not inside.
#[test]
fn the_config_flag_locates_a_project_elsewhere() {
    let project = site("cli34-config-flag");
    let elsewhere = Dir::new("cli34-elsewhere");

    let config = project.path().join("liyasa.json");
    let outcome = Run::new([
        "build".as_ref(),
        "--dry-run".as_ref(),
        "--config".as_ref(),
        config.as_os_str(),
    ])
    .cwd(elsewhere.path())
    .output();

    assert_eq!(outcome.code, Exit::Success.code(), "{}", outcome.all());
    assert!(outcome.stdout.contains("liyasa.json"), "{}", outcome.stdout);
}

/// `--config` pointing at nothing is the project's problem, not a usage error,
/// and it says which of the two mistakes was made.
#[test]
fn a_config_flag_naming_nothing_is_reported_as_such() {
    let elsewhere = Dir::new("cli34-missing-config");
    let outcome = Run::new(["build", "--config", "nowhere/liyasa.json"])
        .cwd(elsewhere.path())
        .output();

    assert_eq!(outcome.code, Exit::Errors.code(), "{}", outcome.all());
    assert!(outcome.all().contains("E0014"), "{}", outcome.all());
}

/// Outside a project at all is the other mistake, with its own code.
#[test]
fn no_project_anywhere_above_is_its_own_error() {
    let elsewhere = Dir::new("cli34-no-project");
    let outcome = Run::new(["build"]).cwd(elsewhere.path()).output();

    assert_eq!(outcome.code, Exit::Errors.code(), "{}", outcome.all());
    assert!(outcome.all().contains("E0001"), "{}", outcome.all());
}

/// `--dry-run` reaches a command through the environment as well, because
/// CLI-34 says every `LIYASA_*` variable is honoured, not just the ones with a
/// value.
#[test]
fn the_environment_can_ask_for_a_dry_run() {
    let project = site("cli34-env-dry-run");
    let outcome = Run::new(["build"])
        .cwd(project.path())
        .env("LIYASA_DRY_RUN", "true")
        .output();

    assert_eq!(outcome.code, Exit::Success.code(), "{}", outcome.all());
    assert!(outcome.stdout.contains("plan"), "{}", outcome.stdout);
    assert!(!project.path().join("dist").exists());
}
