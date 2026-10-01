//! CLI-14: `liyasa migrate-config` upgrades a v0 config to v1.
//!
//! Driven through `Cli::parse_from` and `commands::dispatch`, so the argument
//! parsing, the `--config` resolution and the command all run. What it does
//! not cover is the process around them — the exit code a shell sees and the
//! text on stdout — which `crates/liyasa-cli/tests/it/` owns, because only a
//! test in that package can spawn the binary.

use std::fs;
use std::path::PathBuf;
use std::sync::atomic::{AtomicU64, Ordering};

use clap::Parser;
use liyasa_cli::cli::Cli;
use liyasa_cli::{Exit, commands};
use liyasa_config::json::SpanIndex;
use liyasa_config::schema;
use liyasa_core::span::SourceId;
use serde_json::Value;

/// RFC 0103 defines what v0 is; these are the fixtures the config crate
/// migrates, so the command and the library cannot drift apart.
const V0: &str = include_str!("../../crates/liyasa-config/tests/fixtures/v0.json");
const GOLDEN: &str = include_str!("../../crates/liyasa-config/tests/fixtures/v0-migrated.json");

/// Under `cargo test` every test in the suite is a thread of one process, so
/// a pid separates nothing; the counter is what makes two fixtures different.
static SEQ: AtomicU64 = AtomicU64::new(0);

struct Project(PathBuf);

impl Project {
    fn new(name: &str) -> Self {
        let unique = SEQ.fetch_add(1, Ordering::Relaxed);
        let root = std::env::temp_dir().join(format!(
            "liyasa-cli-14-{name}-{}-{unique}",
            std::process::id()
        ));
        let _ = fs::remove_dir_all(&root);
        fs::create_dir_all(&root).expect("a project directory");
        fs::write(root.join("liyasa.json"), V0).expect("the v0 fixture");
        fs::write(root.join("index.md"), "---\ntitle: Home\n---\n# Home\n").expect("a page");
        Self(root)
    }

    fn config(&self) -> PathBuf {
        self.0.join("liyasa.json")
    }
}

impl Drop for Project {
    fn drop(&mut self) {
        let _ = fs::remove_dir_all(&self.0);
    }
}

fn migrate(project: &Project, extra: &[&str]) -> Exit {
    let config = project.config();
    let mut argv: Vec<String> = vec![
        "liyasa".to_owned(),
        "--config".to_owned(),
        config.display().to_string(),
        "migrate-config".to_owned(),
    ];
    argv.extend(extra.iter().map(|argument| (*argument).to_owned()));
    let cli = Cli::parse_from(argv);
    commands::dispatch(&cli.global, cli.command)
}

/// Runs the command and hands back what it wrote.
///
/// This was a self-skipping helper until WP-09 landed RFC 0110: a v0 config
/// always fails to load — `E0102`, "this config is written for schema v0",
/// whose own help is "run `liyasa migrate-config` to upgrade it" — so the
/// command refused the single input it exists for, and these assertions
/// returned early. `commands::migrate` now lets an older declared version
/// through, so the skip and the predicate that verified its cause are gone
/// rather than left to describe a gap that closed.
fn migrated(project: &Project, extra: &[&str]) -> String {
    assert_eq!(migrate(project, extra), Exit::Success);
    fs::read_to_string(project.config()).expect("the migrated config")
}

#[test]
fn a_v0_config_is_written_back_as_the_golden_v1() {
    let project = Project::new("write");
    let written = migrated(&project, &["--write"]);
    assert_eq!(written.trim_end(), GOLDEN.trim_end());
}

#[test]
fn what_it_writes_validates_against_v1() {
    let project = Project::new("valid");
    let written = migrated(&project, &["--write"]);
    let value: Value = serde_json::from_str(&written).expect("valid JSON");
    let report = schema::check(&value, &SpanIndex::scan(SourceId(0), &written));
    assert!(
        !report.diagnostics.has_errors() && report.unknown.is_empty(),
        "{:?}",
        report.diagnostics
    );
    assert_eq!(
        value.pointer("/$schema").and_then(Value::as_str),
        Some(schema::config_schema_id()),
        "the migrated config declares the version it was migrated to"
    );
}

#[test]
fn without_write_the_file_on_disk_is_untouched() {
    let project = Project::new("dry");
    migrated(&project, &[]);
    assert_eq!(
        fs::read_to_string(project.config()).expect("the config"),
        V0,
        "the default prints the result and changes nothing"
    );
}

#[test]
fn migrating_twice_is_migrating_once() {
    // A row about upgrading between versions has "already upgraded" as its
    // interesting input: the second run must be a no-op, not a second
    // migration.
    let project = Project::new("twice");
    let once = migrated(&project, &["--write"]);
    let twice = migrated(&project, &["--write"]);
    assert_eq!(once, twice);
}

/// The library half, which works today and is what the command would call:
/// the migration itself produces the golden file whatever the command does
/// with its exit code.
#[test]
fn the_migration_itself_produces_the_golden_config() {
    let value: Value = serde_json::from_str(V0).expect("the fixture is valid JSON");
    let migrated = liyasa_config::migrate::migrate(&value, &SpanIndex::scan(SourceId(0), V0));
    assert!(
        !migrated.diagnostics.has_errors(),
        "{:?}",
        migrated.diagnostics
    );
    assert_eq!(migrated.json.trim_end(), GOLDEN.trim_end());
}
