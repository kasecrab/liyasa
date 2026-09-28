//! CLI-14: `liyasa migrate-config` on the input it exists for.
//!
//! The command used to refuse every v0 config and accept only one that was
//! already v1 — one that needed no migration (RFC 0110). These run the
//! command over WP-01's own v0 fixture.

use liyasa_cli::Exit;

use crate::support::{Dir, Run};

const V0: &str = include_str!("../../../liyasa-config/tests/fixtures/v0.json");
const MIGRATED: &str = include_str!("../../../liyasa-config/tests/fixtures/v0-migrated.json");

fn project(name: &str) -> Dir {
    let project = Dir::new(name);
    project.write("liyasa.json", V0);
    project
}

/// The v0 fixture reports three `E0102`s on load: the schema itself, and the
/// two keys whose shape the migration changes. None of the three is a reason
/// to refuse.
#[test]
fn a_v0_config_is_migrated_rather_than_refused() {
    let project = project("mig-v0");

    let outcome = Run::new(["migrate-config", "--write"])
        .cwd(project.path())
        .output();

    assert_eq!(outcome.code, Exit::Success.code(), "{}", outcome.all());
    let written: serde_json::Value = serde_json::from_str(
        &std::fs::read_to_string(project.path().join("liyasa.json")).expect("the config is there"),
    )
    .expect("the written config is JSON");
    let golden: serde_json::Value = serde_json::from_str(MIGRATED).expect("the golden is JSON");
    assert_eq!(written, golden, "{}", outcome.all());
}

/// Running it twice changes nothing: the second run loads a v1 config, which
/// has nothing older to migrate.
#[test]
fn a_second_run_changes_nothing() {
    let project = project("mig-twice");
    Run::new(["migrate-config", "--write"])
        .cwd(project.path())
        .output();
    let once =
        std::fs::read_to_string(project.path().join("liyasa.json")).expect("the config is there");

    let outcome = Run::new(["migrate-config", "--write"])
        .cwd(project.path())
        .output();

    assert_eq!(outcome.code, Exit::Success.code(), "{}", outcome.all());
    assert_eq!(
        once,
        std::fs::read_to_string(project.path().join("liyasa.json")).expect("the config is there")
    );
}

/// A file that is not JSON still refuses: `E0101` leaves nothing to migrate,
/// and the skipped gate was never about that.
#[test]
fn a_config_that_is_not_json_is_still_refused() {
    let project = Dir::new("mig-not-json");
    project.write("liyasa.json", "{ this is not json");

    let outcome = Run::new(["migrate-config", "--write"])
        .cwd(project.path())
        .output();

    assert_eq!(outcome.code, Exit::Errors.code(), "{}", outcome.all());
    assert!(outcome.all().contains("E0101"), "{}", outcome.all());
}

/// Without `--write` the file is untouched and the changes are only listed.
#[test]
fn a_dry_run_lists_the_changes_and_writes_nothing() {
    let project = project("mig-dry");

    let outcome = Run::new(["migrate-config"]).cwd(project.path()).output();

    assert_eq!(outcome.code, Exit::Success.code(), "{}", outcome.all());
    assert_eq!(
        std::fs::read_to_string(project.path().join("liyasa.json")).expect("the config is there"),
        V0,
        "the file was rewritten without --write"
    );
    assert!(outcome.all().contains("logo"), "{}", outcome.all());
}
