//! VER-77: the manifest carries each page's `reviewed:` date.
//!
//! The build is the only thing that reads front matter, and the review cadence
//! is computed afterwards from the manifest — so a date the build drops is a
//! date nothing downstream can recover. Without it the review digest has no date
//! to compare a cadence against and reports nothing on every run, which looks
//! like "no page is overdue" rather than like a missing input.

use std::fs;
use std::path::PathBuf;
use std::sync::atomic::{AtomicU64, Ordering};

use liyasa_build::engine::{self, Options};
use liyasa_build::git::NoGit;
use liyasa_config::vfs::OsVfs;

/// Beside the pid: `bin/gate` runs nextest, one process per test, while CI runs
/// `cargo test`, where a binary's tests are threads sharing one pid.
static COUNTER: AtomicU64 = AtomicU64::new(0);

struct Project(PathBuf);

impl Drop for Project {
    fn drop(&mut self) {
        let _ = fs::remove_dir_all(&self.0);
    }
}

fn site() -> Project {
    let root = std::env::temp_dir().join(format!(
        "liyasa-ver-77-{}-{}",
        std::process::id(),
        COUNTER.fetch_add(1, Ordering::SeqCst)
    ));
    let _ = fs::remove_dir_all(&root);
    fs::create_dir_all(root.join("guides")).expect("a project directory");
    fs::write(
        root.join("liyasa.json"),
        r#"{"name":"Acme docs","seo":{"canonicalOrigin":"https://docs.acme.com"}}"#,
    )
    .expect("config");
    fs::write(
        root.join("index.md"),
        "---\ntitle: Home\nreviewed: 2026-01-15\n---\n# Home\n\nChecked in January.\n",
    )
    .expect("a reviewed page");
    fs::write(
        root.join("guides/install.md"),
        "---\ntitle: Install\n---\n# Install\n\nNever reviewed.\n",
    )
    .expect("an unreviewed page");
    Project(root)
}

#[test]
fn the_manifest_carries_a_reviewed_date_and_says_nothing_when_there_is_none() {
    let project = site();
    let vfs = OsVfs::new(&project.0);
    let report = engine::build(
        &vfs,
        &NoGit,
        &project.0,
        &Options {
            build_time: Some(1_789_473_600),
            ..Options::default()
        },
    );
    assert!(!report.failed(false), "{:?}", report.diagnostics);

    let manifest = report.manifest.as_ref().expect("a manifest");
    let reviewed = |route: &str| {
        manifest
            .routes
            .iter()
            .find(|entry| entry.route.as_str() == route)
            .unwrap_or_else(|| panic!("no entry for {route}"))
            .reviewed
            .clone()
    };

    assert_eq!(reviewed("/"), Some("2026-01-15".to_owned()));
    // The control: absent rather than defaulted. A field that is always `Some`
    // would pass the assertion above while telling the cadence that every page
    // was reviewed on some date.
    assert_eq!(reviewed("/guides/install"), None);
}

/// ANA-40's two inputs. `description` is carried as the value, not as a
/// `has_description` predicate, because an author who typed the key and left it
/// blank is a different finding from one who never typed it — and a `bool`
/// cannot say which.
#[test]
fn an_empty_description_is_not_an_absent_one() {
    let project = site();
    fs::write(
        project.0.join("guides/blank.md"),
        "---\ntitle: Blank\ndescription: \"\"\n---\n# Blank\n\nTyped the key, left it empty.\n",
    )
    .expect("a page with an empty description");
    fs::write(
        project.0.join("guides/written.md"),
        "---\ntitle: Written\ndescription: A real summary.\n---\n# Written\n\nBody.\n",
    )
    .expect("a page with a description");

    let vfs = OsVfs::new(&project.0);
    let report = engine::build(
        &vfs,
        &NoGit,
        &project.0,
        &Options {
            build_time: Some(1_789_473_600),
            ..Options::default()
        },
    );
    assert!(!report.failed(false), "{:?}", report.diagnostics);
    let manifest = report.manifest.as_ref().expect("a manifest");
    let described = |route: &str| {
        manifest
            .routes
            .iter()
            .find(|entry| entry.route.as_str() == route)
            .unwrap_or_else(|| panic!("no entry for {route}"))
            .description
            .clone()
    };

    assert_eq!(
        described("/guides/written"),
        Some("A real summary.".to_owned())
    );
    assert_eq!(described("/guides/blank"), Some(String::new()));
    assert_eq!(described("/guides/install"), None);
}

/// `NoGit` answers nothing, so the field is absent rather than zero. A `0` here
/// would date every page to 1970 and fire `StalePopularPages` on every page of
/// every instance, with no test failing.
#[test]
fn a_project_outside_a_repository_has_no_source_date_rather_than_the_epoch() {
    let project = site();
    let vfs = OsVfs::new(&project.0);
    let report = engine::build(
        &vfs,
        &NoGit,
        &project.0,
        &Options {
            build_time: Some(1_789_473_600),
            ..Options::default()
        },
    );
    assert!(!report.failed(false), "{:?}", report.diagnostics);
    let manifest = report.manifest.as_ref().expect("a manifest");
    assert!(
        manifest
            .routes
            .iter()
            .all(|entry| entry.source_updated_unix.is_none()),
        "no git, no dates: {:?}",
        manifest
            .routes
            .iter()
            .map(|entry| (entry.route.as_str(), entry.source_updated_unix))
            .collect::<Vec<_>>()
    );
}

/// The field has to survive the JSON the server actually reads, not only the
/// in-process `Report` — `Bundle` deserialises `liyasa-manifest.json`.
#[test]
fn the_date_survives_the_json_the_server_reads() {
    let project = site();
    let vfs = OsVfs::new(&project.0);
    let report = engine::build(
        &vfs,
        &NoGit,
        &project.0,
        &Options {
            build_time: Some(1_789_473_600),
            ..Options::default()
        },
    );
    assert!(!report.failed(false), "{:?}", report.diagnostics);

    let text = fs::read_to_string(project.0.join("dist/liyasa-manifest.json"))
        .expect("the manifest is written");
    let parsed: liyasa_build::manifest::Manifest =
        serde_json::from_str(&text).expect("the manifest round-trips");
    let home = parsed
        .routes
        .iter()
        .find(|entry| entry.route.as_str() == "/")
        .expect("an entry for /");
    assert_eq!(home.reviewed, Some("2026-01-15".to_owned()));
    assert!(
        !text.contains("\"reviewed\":null"),
        "an absent date is skipped rather than written as null"
    );
}
