//! §6.6.2 rule 1: `liyasa build --build-time` fixes the build clock.
//!
//! The rule gives a precedence — `SOURCE_DATE_EPOCH`, then this flag, then the
//! commit timestamp, then the wall clock with a warning — and the manifest's
//! `builtAt` is where the chosen instant becomes observable.

use liyasa_cli::Exit;

use crate::support::{Dir, Run};

/// An arbitrary fixed instant, and the same instant written as a date.
const SECONDS: i64 = 1_700_000_000;
const AS_DATE: &str = "2023-11-14T22:13:20Z";

fn site(name: &str) -> Dir {
    let project = Dir::new(name);
    project
        .write(
            "liyasa.json",
            r#"{"name":"Acme docs","seo":{"canonicalOrigin":"https://docs.acme.com"}}"#,
        )
        .write("index.md", "---\ntitle: Home\n---\n\n# Home\n\nWelcome.\n");
    project
}

fn built_at(project: &std::path::Path) -> i64 {
    let text =
        std::fs::read_to_string(project.join("dist/liyasa-manifest.json")).expect("the manifest");
    let manifest: serde_json::Value = serde_json::from_str(&text).expect("valid JSON");
    manifest["builtAt"]
        .as_i64()
        .unwrap_or_else(|| panic!("no builtAt in {text}"))
}

#[test]
fn a_timestamp_dates_the_build() {
    let project = site("bt-seconds");
    let outcome = Run::new(["build", "--build-time", "1700000000"])
        .cwd(project.path())
        .output();

    assert_eq!(outcome.code, Exit::Success.code(), "{}", outcome.all());
    assert_eq!(built_at(project.path()), SECONDS);
}

#[test]
fn a_date_dates_the_build_the_same_way() {
    let project = site("bt-date");
    let outcome = Run::new(["build", "--build-time", AS_DATE])
        .cwd(project.path())
        .output();

    assert_eq!(outcome.code, Exit::Success.code(), "{}", outcome.all());
    assert_eq!(built_at(project.path()), SECONDS);
}

/// The point of the flag: two builds of the same inputs agree. Without it a
/// project outside git falls back to the wall clock and they do not.
#[test]
fn two_builds_with_the_same_build_time_agree() {
    let first = site("bt-repeat-a");
    let second = site("bt-repeat-b");
    for project in [&first, &second] {
        let outcome = Run::new(["build", "--build-time", "1700000000"])
            .cwd(project.path())
            .output();
        assert_eq!(outcome.code, Exit::Success.code(), "{}", outcome.all());
    }
    assert_eq!(built_at(first.path()), built_at(second.path()));
}

/// W0707 says the build could not be dated and is not reproducible. Giving it
/// a date is exactly what silences it.
#[test]
fn a_dated_build_does_not_warn_that_it_is_undatable() {
    let project = site("bt-warning");

    let undated = Run::new(["build"]).cwd(project.path()).output();
    assert!(
        undated.all().contains("W0707"),
        "a project outside git should warn: {}",
        undated.all()
    );

    let dated = Run::new(["build", "--clean", "--build-time", "1700000000"])
        .cwd(project.path())
        .output();
    assert!(
        !dated.all().contains("W0707"),
        "still warned after being dated: {}",
        dated.all()
    );
}

/// §6.6.2 puts `SOURCE_DATE_EPOCH` ahead of the flag, so a build inside a
/// reproducible-build harness keeps the harness's instant.
#[test]
fn the_environment_wins_over_the_flag() {
    let project = site("bt-precedence");
    let outcome = Run::new(["build", "--build-time", "1700000000"])
        .cwd(project.path())
        .env("SOURCE_DATE_EPOCH", "1600000000")
        .output();

    assert_eq!(outcome.code, Exit::Success.code(), "{}", outcome.all());
    assert_eq!(built_at(project.path()), 1_600_000_000);
}

/// CLI-34: the flag is readable from the environment like every other.
#[test]
fn the_environment_can_supply_the_flag() {
    let project = site("bt-env");
    let outcome = Run::new(["build"])
        .cwd(project.path())
        .env("LIYASA_BUILD_TIME", AS_DATE)
        .output();

    assert_eq!(outcome.code, Exit::Success.code(), "{}", outcome.all());
    assert_eq!(built_at(project.path()), SECONDS);
}

/// A value that is not a time is a usage error, not a build that quietly picks
/// its own instant.
#[test]
fn a_value_that_is_not_a_time_is_refused() {
    let project = site("bt-bad");
    let outcome = Run::new(["build", "--build-time", "yesterday"])
        .cwd(project.path())
        .output();

    assert_eq!(outcome.code, Exit::Usage.code(), "{}", outcome.all());
    assert!(outcome.all().contains("RFC 3339"), "{}", outcome.all());
    assert!(!project.path().join("dist").exists(), "it built anyway");
}

/// A date with no zone would mean a different instant in every timezone, which
/// is the opposite of what the flag is for.
#[test]
fn a_date_without_a_zone_is_refused() {
    let project = site("bt-nozone");
    let outcome = Run::new(["build", "--build-time", "2023-11-14T22:13:20"])
        .cwd(project.path())
        .output();
    assert_eq!(outcome.code, Exit::Usage.code(), "{}", outcome.all());
}

/// The dry-run plan says what the build would be dated from, because that is
/// the question `--dry-run` exists to answer.
#[test]
fn the_plan_reports_the_build_time() {
    let project = site("bt-plan");

    let fixed = Run::new(["build", "--dry-run", "--build-time", "1700000000"])
        .cwd(project.path())
        .output();
    assert!(fixed.stdout.contains("1700000000"), "{}", fixed.stdout);

    let unset = Run::new(["build", "--dry-run"])
        .cwd(project.path())
        .output();
    assert!(unset.stdout.contains("build time"), "{}", unset.stdout);
}
