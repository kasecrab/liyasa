//! CFG-93: `liyasa.<env>.json` is deep-merged when `--env` is passed, and when
//! the server builds a preview.
//!
//! The merge itself is `crates/liyasa-config/tests/it/load.rs`. What this file
//! asserts is the other half of the row — that each of the two triggers it
//! names reaches a build — and it drives each one the way its own caller does:
//! `--env` through `Cli::parse_from` and `commands::dispatch`, the preview
//! through the `Options` the deploy worker derives from a job's plan. Reading
//! either call site would prove the field is assigned; only running it proves
//! the overlay lands in the output.

use std::collections::BTreeMap;
use std::fs;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicU64, Ordering};

use clap::Parser;
use liyasa_build::engine;
use liyasa_build::git::NoGit;
use liyasa_cli::cli::Cli;
use liyasa_cli::{Exit, commands};
use liyasa_config::vfs::OsVfs;
use liyasa_core::ids::ProjectId;
use liyasa_server::deploy::worker::{self, Plan};

/// A pid is one value for the whole suite under `cargo test`, where every test
/// is a thread of one process, so it cannot separate two fixtures on its own.
static SEQ: AtomicU64 = AtomicU64::new(0);

const BASE_ORIGIN: &str = "https://docs.acme.com";
const PREVIEW_ORIGIN: &str = "https://preview.acme.dev";

/// `build.output` is set in the base and left alone by the overlay, so the
/// output directory is where a replace-rather-than-merge would be visible:
/// the overlay's `build` object would take the key with it.
const BASE: &str = r#"{"name":"Acme docs","description":"How Acme works",
  "seo":{"canonicalOrigin":"https://docs.acme.com"},
  "build":{"output":"site"}}"#;

const OVERLAY: &str = r#"{"seo":{"canonicalOrigin":"https://preview.acme.dev"},
  "build":{"drafts":true}}"#;

struct Project(PathBuf);

impl Project {
    fn new(name: &str) -> Self {
        let unique = SEQ.fetch_add(1, Ordering::Relaxed);
        let root = std::env::temp_dir().join(format!(
            "liyasa-cfg-93-{name}-{}-{unique}",
            std::process::id()
        ));
        let _ = fs::remove_dir_all(&root);
        fs::create_dir_all(&root).expect("a project directory");
        let project = Self(root);
        project
            .write("liyasa.json", BASE)
            .write("liyasa.preview.json", OVERLAY)
            .write("index.md", "---\ntitle: Home\n---\n# Home\n\nWelcome.\n")
            .write(
                "guides/install.md",
                "---\ntitle: Install\n---\n# Install\n\nRun it.\n",
            )
            .write(
                "guides/next.md",
                "---\ntitle: Next\ndraft: true\n---\n# Next\n\nSoon.\n",
            );
        project
    }

    fn write(&self, path: &str, text: &str) -> &Self {
        let full = self.0.join(path);
        if let Some(parent) = full.parent() {
            fs::create_dir_all(parent).expect("a directory");
        }
        fs::write(full, text).expect("a file");
        self
    }

    fn path(&self) -> &Path {
        &self.0
    }

    /// `build.output` is `site`, so this is also the assertion that the key
    /// survived the overlay.
    fn out(&self, path: &str) -> PathBuf {
        self.0.join("site").join(path)
    }

    fn read_out(&self, path: &str) -> String {
        fs::read_to_string(self.out(path))
            .unwrap_or_else(|error| panic!("site/{path} is missing: {error}"))
    }
}

impl Drop for Project {
    fn drop(&mut self) {
        let _ = fs::remove_dir_all(&self.0);
    }
}

fn build_through_cli(project: &Project, extra: &[&str]) -> Exit {
    let mut argv: Vec<String> = vec![
        "liyasa".to_owned(),
        "--config".to_owned(),
        project.path().join("liyasa.json").display().to_string(),
        "build".to_owned(),
    ];
    argv.extend(extra.iter().map(|argument| (*argument).to_owned()));
    let cli = Cli::parse_from(argv);
    commands::dispatch(&cli.global, cli.command)
}

#[test]
fn env_preview_merges_the_overlay_into_the_build() {
    let project = Project::new("cli-env");
    assert_eq!(
        build_through_cli(&project, &["--env", "preview"]),
        Exit::Success
    );

    let llms = project.read_out("llms.txt");
    assert!(
        llms.contains(PREVIEW_ORIGIN) && !llms.contains(BASE_ORIGIN),
        "the overlay's origin is the one the build used: {llms}"
    );
    assert!(
        project.out("guides/next/index.html").exists(),
        "`build.drafts` from the overlay put the draft in the output"
    );
}

#[test]
fn without_env_the_overlay_is_not_read() {
    // The same project, the same file on disk: the trigger is the only
    // difference, which is what makes the test above about the trigger.
    let project = Project::new("cli-plain");
    assert_eq!(build_through_cli(&project, &[]), Exit::Success);

    let llms = project.read_out("llms.txt");
    assert!(
        llms.contains(BASE_ORIGIN) && !llms.contains(PREVIEW_ORIGIN),
        "{llms}"
    );
    assert!(
        !project.out("guides/next/index.html").exists(),
        "a draft is out of a build nobody asked for drafts in"
    );
}

#[test]
fn the_overlay_adds_to_the_base_rather_than_replacing_it() {
    let project = Project::new("deep");
    assert_eq!(
        build_through_cli(&project, &["--env", "preview"]),
        Exit::Success
    );

    // `build.output` is in the base only, and the overlay's `build` object
    // holds `drafts`. Both are in effect, so the merge went key by key.
    assert!(project.out("index.html").exists(), "build.output survived");
    assert!(project.out("guides/next/index.html").exists());
    assert!(
        !project.path().join("dist").exists(),
        "nothing fell back to the default output directory"
    );
}

#[test]
fn a_preview_build_from_a_deploy_plan_reads_the_preview_overlay() {
    // The server's half of the row. `Plan.env` comes off the job row and is
    // the only thing here that names an environment, so a build that lands on
    // the preview origin is the overlay being merged for a preview with no
    // `--env` anywhere in it.
    let project = Project::new("preview-plan");
    let plan = Plan {
        // A ULID, which is what a project id is; the build never reads it.
        project: ProjectId::parse("01ARZ3NDEKTSV4RRFFQ69G5FAV").expect("a project id"),
        env: "preview".to_owned(),
        branch: "patch-1".to_owned(),
        commit: "abc".to_owned(),
        untrusted: false,
        workspace: None,
        base_commit: None,
        cache_from: None,
        pull_request: Some(7),
    };
    let options = worker::options_for(&plan, BTreeMap::new());
    assert_eq!(options.env.as_deref(), Some("preview"));

    let vfs = OsVfs::new(project.path());
    let report = engine::build(&vfs, &NoGit, project.path(), &options);
    assert!(!report.failed(false), "{:?}", report.diagnostics);

    let llms = project.read_out("llms.txt");
    assert!(
        llms.contains(PREVIEW_ORIGIN) && !llms.contains(BASE_ORIGIN),
        "{llms}"
    );
    // Nothing is asserted here about the draft. The worker pins
    // `drafts: false`, but the engine takes `options.drafts || settings.drafts`
    // (`engine/mod.rs:181`), so the overlay's `build.drafts` decides it — which
    // is a question about who wins between a caller and a config, not about
    // whether the overlay was read.
}
