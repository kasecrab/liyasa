//! CLI-02: the dev loop. The server and the socket belong to other packages;
//! what is asserted here is the engine half — a first render off the persisted
//! cache, one rebuild per debounced batch, and the mock reader flags.
//!
//! The p95 figure (a single-page edit reflected within 100 ms over 50 edits on
//! the 1,000-page fixture) is the ignored release test at the bottom.

use std::fs;
use std::path::{Path, PathBuf};
use std::time::Duration;

use liyasa_build::dev::{Flags, Session};
use liyasa_build::watch::{self, Kind, Watch};
use liyasa_config::vfs::OsVfs;
use liyasa_core::ids::Route;
use liyasa_markdown::source::route::Ignore;

struct Project(PathBuf);

impl Project {
    fn new(name: &str) -> Self {
        let root =
            std::env::temp_dir().join(format!("liyasa-cli-02-{name}-{}", std::process::id()));
        let _ = fs::remove_dir_all(&root);
        fs::create_dir_all(&root).expect("a project directory");
        Self(root)
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

    fn read_dist(&self, path: &str) -> String {
        fs::read_to_string(self.0.join("dist").join(path))
            .unwrap_or_else(|error| panic!("dist/{path} is missing: {error}"))
    }
}

impl Drop for Project {
    fn drop(&mut self) {
        let _ = fs::remove_dir_all(&self.0);
    }
}

fn site(name: &str) -> Project {
    let project = Project::new(name);
    project
        .write("liyasa.json", r#"{"name":"Acme docs"}"#)
        .write("index.md", "---\ntitle: Home\n---\n# Home\n\nWelcome.\n")
        .write(
            "guides/install.md",
            "---\ntitle: Install\n---\n# Install\n\nRun it.\n",
        );
    project
}

fn batch_of(paths: &[&str]) -> watch::Batch {
    watch::batch(
        paths.iter().map(liyasa_core::vfs::VfsPath::new),
        &Ignore::default(),
        "dist",
    )
}

#[test]
fn the_first_render_builds_the_whole_site() {
    let project = site("first");
    let vfs = OsVfs::new(project.path());
    let mut session = Session::new(project.path(), Flags::default());
    let first = session.first_render(&vfs);
    assert!(
        !first.report.failed(false),
        "{:?}",
        first.report.diagnostics
    );
    assert_eq!(first.report.pages, 2);
    assert!(first.reload);
    assert!(project.read_dist("index.html").contains("Welcome."));
}

#[test]
fn a_warm_first_render_hits_the_persisted_cache() {
    let project = site("warm");
    let vfs = OsVfs::new(project.path());
    Session::new(project.path(), Flags::default()).first_render(&vfs);

    // A second process would see exactly this: the cache on disk, nothing in
    // memory.
    let mut session = Session::new(project.path(), Flags::default());
    let warm = session.first_render(&vfs);
    assert_eq!(warm.report.cache_misses, 0, "{:?}", warm.report.diagnostics);
    assert_eq!(warm.report.cache_hits, warm.report.variants);
}

#[test]
fn an_edit_rebuilds_only_the_edited_page_and_names_it() {
    let project = site("edit");
    let vfs = OsVfs::new(project.path());
    let mut session = Session::new(project.path(), Flags::default());
    session.first_render(&vfs);

    project.write(
        "guides/install.md",
        "---\ntitle: Install\n---\n# Install\n\nRun it twice.\n",
    );
    let rebuild = session.rebuild(&vfs, &batch_of(&["guides/install.md"]));
    assert_eq!(rebuild.report.cache_misses, 1);
    assert_eq!(rebuild.changed, vec![Route::new("/guides/install")]);
    assert!(
        !rebuild.reload,
        "a content edit patches rather than reloads"
    );
    assert!(
        project
            .read_dist("guides/install/index.html")
            .contains("Run it twice.")
    );
}

#[test]
fn a_config_edit_asks_the_client_to_reload() {
    let project = site("config");
    let vfs = OsVfs::new(project.path());
    let mut session = Session::new(project.path(), Flags::default());
    session.first_render(&vfs);

    project.write("liyasa.json", r#"{"name":"Acme documentation"}"#);
    let rebuild = session.rebuild(&vfs, &batch_of(&["liyasa.json"]));
    assert!(rebuild.reload);
    assert_eq!(rebuild.report.cache_misses, rebuild.report.variants);
}

#[test]
fn a_new_page_appears_without_a_restart() {
    let project = site("new-page");
    let vfs = OsVfs::new(project.path());
    let mut session = Session::new(project.path(), Flags::default());
    session.first_render(&vfs);

    project.write("guides/next.md", "---\ntitle: Next\n---\n# Next\n");
    let rebuild = session.rebuild(&vfs, &batch_of(&["guides/next.md"]));
    assert_eq!(rebuild.report.pages, 3);
    assert!(rebuild.changed.contains(&Route::new("/guides/next")));
}

#[test]
fn the_drafts_flag_includes_a_draft() {
    let project = site("drafts");
    project.write(
        "guides/wip.md",
        "---\ntitle: WIP\ndraft: true\n---\n# WIP\n",
    );
    let vfs = OsVfs::new(project.path());

    let without = Session::new(project.path(), Flags::default()).first_render(&vfs);
    assert_eq!(without.report.pages, 2);

    let with = Session::new(
        project.path(),
        Flags {
            drafts: true,
            ..Flags::default()
        },
    )
    .first_render(&vfs);
    assert_eq!(with.report.pages, 3);
}

#[test]
fn the_base_path_flag_moves_every_url() {
    let project = site("base-path");
    let vfs = OsVfs::new(project.path());
    let rebuild = Session::new(
        project.path(),
        Flags {
            base_path: Some("/docs".to_owned()),
            ..Flags::default()
        },
    )
    .first_render(&vfs);
    let manifest = rebuild.report.manifest.as_ref().expect("a manifest");
    assert_eq!(manifest.base_path, "/docs");
    assert_eq!(
        manifest
            .route(&Route::new("/guides/install"))
            .map(|route| route.markdown.as_str()),
        Some("/docs/guides/install.md")
    );
}

#[test]
fn the_mock_reader_flags_pick_the_previewed_variant() {
    let flags = Flags {
        groups: vec!["admin".to_owned(), "partner".to_owned()],
        region: Some("eu".to_owned()),
        locale: Some("de".to_owned()),
        version: Some("v2".to_owned()),
        ..Flags::default()
    };
    let variant = flags.variant();
    assert_eq!(variant.groups.len(), 2);
    assert_eq!(variant.region.as_deref(), Some("eu"));
    assert_eq!(
        variant.version.as_ref().map(|version| version.as_str()),
        Some("v2")
    );
}

#[test]
fn the_watcher_turns_an_edit_into_one_batch() {
    let project = site("watch");
    let watch = Watch::new(project.path(), Ignore::default(), "dist").expect("a watcher");
    project.write(
        "guides/install.md",
        "---\ntitle: Install\n---\n# Install\n\nEdited.\n",
    );

    // A file system may report the directory before the file, so batches are
    // collected until the edited path shows up or the deadline passes.
    let deadline = std::time::Instant::now() + Duration::from_secs(10);
    let mut seen: Vec<String> = Vec::new();
    let mut kind = Kind::Content;
    while std::time::Instant::now() < deadline {
        let Some(batch) = watch.next_batch(Duration::from_millis(500)) else {
            continue;
        };
        kind = batch.kind;
        seen.extend(batch.paths.iter().map(|path| path.as_str().to_owned()));
        if seen.iter().any(|path| path == "guides/install.md") {
            break;
        }
    }
    assert_eq!(kind, Kind::Content);
    assert!(
        seen.iter().any(|path| path == "guides/install.md"),
        "{seen:?}"
    );
}

/// CLI-02's figures: the first render under 1 s and a single-page edit under
/// 100 ms at p95 over 50 edits, on the 1,000-page fixture. Ignored by default;
/// run with `cargo test --release -p liyasa-tests --test cli_02_dev -- --ignored`.
#[test]
#[ignore = "timing figure; run in release"]
fn a_thousand_pages_rebuild_inside_the_budget() {
    let project = Project::new("thousand-dev");
    project.write("liyasa.json", r#"{"name":"Big docs"}"#);
    for page in 0..1_000 {
        project.write(
            &format!("guides/page-{page:04}.md"),
            &format!("---\ntitle: Page {page}\n---\n# Page {page}\n\nBody {page}.\n"),
        );
    }
    let vfs = OsVfs::new(project.path());
    let mut session = Session::new(project.path(), Flags::default());
    session.first_render(&vfs);

    // Warm: what `liyasa dev` starts from on a project built before.
    let mut warm = Session::new(project.path(), Flags::default());
    let first = warm.first_render(&vfs);
    assert!(
        first.elapsed < Duration::from_secs(1),
        "first render took {:?}",
        first.elapsed
    );

    let mut timings = Vec::new();
    for edit in 0..50 {
        project.write(
            "guides/page-0001.md",
            &format!("---\ntitle: Page 1\n---\n# Page 1\n\nEdit {edit}.\n"),
        );
        let rebuild = warm.rebuild(&vfs, &batch_of(&["guides/page-0001.md"]));
        assert_eq!(rebuild.report.cache_misses, 1);
        timings.push(rebuild.elapsed);
    }
    timings.sort();
    let p95 = timings[timings.len() * 95 / 100];
    assert!(p95 < Duration::from_millis(100), "p95 was {p95:?}");
}
