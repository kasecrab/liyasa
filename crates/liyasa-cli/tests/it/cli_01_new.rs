//! CLI-01 and MIG-10: `liyasa new --yes` produces a site that builds and that
//! passes the agent-readiness static checks.

use liyasa_cli::Exit;

use crate::support::{Dir, Run};

#[test]
fn a_scaffold_builds() {
    let root = Dir::new("cli01-builds");
    let project = root.path().join("docs");

    let created = Run::new(["new", "docs", "--yes"]).cwd(root.path()).output();
    assert_eq!(created.code, Exit::Success.code(), "{}", created.all());
    assert!(project.join("liyasa.json").is_file(), "{}", created.all());

    let built = Run::new(["build"]).cwd(&project).output();
    assert_eq!(built.code, Exit::Success.code(), "{}", built.all());
    assert!(project.join("dist/index.html").is_file(), "{}", built.all());
}

/// Nothing the scaffold writes should be reported by the command that checks
/// it. A starter that fails `liyasa validate` teaches the wrong first lesson.
#[test]
fn a_scaffold_validates_clean() {
    let root = Dir::new("cli01-validates");
    let project = root.path().join("docs");
    let created = Run::new(["new", "docs", "--yes"]).cwd(root.path()).output();
    assert_eq!(created.code, Exit::Success.code(), "{}", created.all());

    let validated = Run::new(["validate", "--format", "json"])
        .cwd(&project)
        .output();
    assert_eq!(
        validated.code,
        Exit::Success.code(),
        "the scaffold does not validate:\n{}",
        validated.all()
    );

    // Not merely exit 0: no diagnostic of error severity at all. Warnings are
    // allowed, because every relative link in a starter draws W0406 and the
    // origin in the scaffold is a placeholder.
    let document: serde_json::Value =
        serde_json::from_str(&validated.stdout).expect("the JSON report");
    let errors = document
        .pointer("/summary/errors")
        .and_then(serde_json::Value::as_u64);
    assert_eq!(errors, Some(0), "{}", validated.stdout);
}

/// `liyasa format --check` is in the scaffold's own CI workflow, so the files
/// it writes have to already be in canonical form.
#[test]
fn a_scaffold_is_already_formatted() {
    let root = Dir::new("cli01-formatted");
    let project = root.path().join("docs");
    Run::new(["new", "docs", "--yes"]).cwd(root.path()).output();

    let checked = Run::new(["format", "--check"]).cwd(&project).output();
    assert_eq!(
        checked.code,
        Exit::Success.code(),
        "the scaffold is not canonically formatted:\n{}",
        checked.all()
    );
}

/// MIG-10 asks for a page of each kind, a sample specification, a sample fact,
/// a verified code block, and a checklist page.
#[test]
fn the_scaffold_has_what_the_requirement_names() {
    let root = Dir::new("cli01-contents");
    let project = root.path().join("docs");
    Run::new(["new", "docs", "--yes"]).cwd(root.path()).output();

    for path in [
        "liyasa.json",
        "index.md",
        "guides/quickstart.md",
        "guides/configuration.md",
        "guides/verification.md",
        "checklist.md",
        "openapi/api.yaml",
        "facts/pricing.json",
        "facts/sources.toml",
        "README.md",
        ".gitignore",
        ".liyasaignore",
        ".github/workflows/docs.yml",
    ] {
        assert!(project.join(path).is_file(), "the scaffold has no {path}");
    }

    let quickstart =
        std::fs::read_to_string(project.join("guides/quickstart.md")).expect("the quickstart");
    assert!(
        quickstart.contains("verify="),
        "no verified code block in the quickstart"
    );
}

#[test]
fn the_site_name_comes_from_the_directory_and_can_be_overridden() {
    let root = Dir::new("cli01-name");

    Run::new(["new", "acme-docs", "--yes"])
        .cwd(root.path())
        .output();
    let config =
        std::fs::read_to_string(root.path().join("acme-docs/liyasa.json")).expect("the config");
    assert!(config.contains("\"Acme docs\""), "{config}");

    Run::new(["new", "other", "--yes", "--name", "Handbook"])
        .cwd(root.path())
        .output();
    let named = std::fs::read_to_string(root.path().join("other/liyasa.json")).expect("the config");
    assert!(named.contains("\"Handbook\""), "{named}");
}

#[test]
fn the_sample_specification_can_be_left_out() {
    let root = Dir::new("cli01-no-openapi");
    let project = root.path().join("docs");
    Run::new(["new", "docs", "--yes", "--no-openapi"])
        .cwd(root.path())
        .output();

    assert!(!project.join("openapi/api.yaml").exists());
    let config = std::fs::read_to_string(project.join("liyasa.json")).expect("the config");
    assert!(!config.contains("openapi"), "{config}");

    let built = Run::new(["build"]).cwd(&project).output();
    assert_eq!(built.code, Exit::Success.code(), "{}", built.all());
}

#[test]
fn the_workflow_can_be_left_out() {
    let root = Dir::new("cli01-no-ci");
    Run::new(["new", "docs", "--yes", "--no-ci"])
        .cwd(root.path())
        .output();
    assert!(!root.path().join("docs/.github").exists());
}

#[test]
fn a_preset_that_is_not_one_is_refused_before_anything_is_written() {
    let root = Dir::new("cli01-bad-preset");
    let outcome = Run::new(["new", "docs", "--yes", "--preset", "chartreuse"])
        .cwd(root.path())
        .output();

    assert_eq!(outcome.code, Exit::Errors.code(), "{}", outcome.all());
    assert!(outcome.all().contains("E0013"), "{}", outcome.all());
    assert!(
        !root.path().join("docs/liyasa.json").exists(),
        "a rejected preset still wrote the project"
    );
}

#[test]
fn a_directory_that_already_has_a_project_is_refused() {
    let root = Dir::new("cli01-occupied");
    root.write("docs/liyasa.json", "{}");

    let outcome = Run::new(["new", "docs", "--yes"]).cwd(root.path()).output();
    assert_eq!(outcome.code, Exit::Errors.code(), "{}", outcome.all());
    assert!(outcome.all().contains("E0012"), "{}", outcome.all());
    // The existing file is untouched.
    assert_eq!(
        std::fs::read_to_string(root.path().join("docs/liyasa.json")).expect("the file"),
        "{}"
    );
}

/// A README and a licence are what a person has usually already put in a new
/// repository, and refusing those would make `liyasa new .` useless.
#[test]
fn a_directory_with_only_repository_furniture_is_fine() {
    let root = Dir::new("cli01-furniture");
    root.write("docs/README.md", "# Mine\n");
    root.write("docs/LICENSE", "MIT\n");

    let outcome = Run::new(["new", "docs", "--yes"]).cwd(root.path()).output();
    assert_eq!(outcome.code, Exit::Success.code(), "{}", outcome.all());
    assert!(root.path().join("docs/liyasa.json").is_file());
}

#[test]
fn a_template_this_release_does_not_have_is_refused() {
    let root = Dir::new("cli01-template");
    let outcome = Run::new(["new", "docs", "--yes", "--template", "minimal"])
        .cwd(root.path())
        .output();

    assert_eq!(outcome.code, Exit::Errors.code(), "{}", outcome.all());
    assert!(outcome.all().contains("E0013"), "{}", outcome.all());
    assert!(!root.path().join("docs").exists());
}

#[test]
fn a_dry_run_lists_the_files_and_writes_none_of_them() {
    let root = Dir::new("cli01-dry-run");
    let outcome = Run::new(["new", "docs", "--yes", "--dry-run"])
        .cwd(root.path())
        .output();

    assert_eq!(outcome.code, Exit::Success.code(), "{}", outcome.all());
    assert!(outcome.stdout.contains("liyasa.json"), "{}", outcome.stdout);
    assert!(
        outcome.stdout.contains("checklist.md"),
        "{}",
        outcome.stdout
    );
    assert!(!root.path().join("docs").exists(), "a dry run created it");
}

/// The scaffold is a git repository when git is available, because the first
/// thing anyone does to it is change it.
#[test]
fn a_repository_is_initialised_unless_refused() {
    let root = Dir::new("cli01-git");

    let outcome = Run::new(["new", "with-git", "--yes"])
        .cwd(root.path())
        .output();
    assert_eq!(outcome.code, Exit::Success.code(), "{}", outcome.all());
    let initialised = root.path().join("with-git/.git").exists();
    // Only assert the positive case where git exists to do it with.
    if outcome.stdout.contains("git repository initialised") {
        assert!(initialised);
    }

    Run::new(["new", "without-git", "--yes", "--no-git"])
        .cwd(root.path())
        .output();
    assert!(!root.path().join("without-git/.git").exists());
}
