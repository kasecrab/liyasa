//! CFG-64: two builds of the same history and the same clock write the same
//! `sitemap.xml`.
//!
//! What this does not cover, because the engine does not do it yet: `lastmod`
//! from the git snapshot rather than the build clock, `changefreq`, per-locale
//! sitemaps, and the `seo.robots` and `seo.crawlers` groups. `sitemap.xml` is
//! dated from `updated` front matter or from the clock, and nothing reads
//! `seo.robots`, `seo.sitemap`, or `seo.crawlers`.

use std::fs;
use std::path::{Path, PathBuf};
use std::time::{Duration, SystemTime};

use liyasa_build::engine::{self, Options};
use liyasa_build::git::GitMeta;
use liyasa_config::vfs::OsVfs;
use liyasa_core::vfs::VfsPath;

/// The build clock every build in this file is given, so the only thing that
/// could differ between two runs is the engine.
const CLOCK: i64 = 1_789_473_600;

struct Project(PathBuf);

impl Project {
    fn new(name: &str) -> Self {
        let root =
            std::env::temp_dir().join(format!("liyasa-cfg-64-{name}-{}", std::process::id()));
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

/// A repository whose history is fixed, so `GitSnapshot` has something to
/// freeze and two builds see the same dates.
struct History;

impl GitMeta for History {
    fn head_commit_time(&self) -> Option<SystemTime> {
        Some(SystemTime::UNIX_EPOCH + Duration::from_secs(1_700_000_000))
    }

    fn head_revision(&self) -> Option<String> {
        Some("9f1c0de".to_owned())
    }

    fn last_modified(&self, path: &VfsPath) -> Option<SystemTime> {
        let seconds = match path.as_str() {
            "index.md" => 1_690_000_000,
            "guides/install.md" => 1_680_000_000,
            _ => return None,
        };
        Some(SystemTime::UNIX_EPOCH + Duration::from_secs(seconds))
    }
}

fn site(name: &str) -> Project {
    let project = Project::new(name);
    project
        .write(
            "liyasa.json",
            r#"{"name":"Acme docs","seo":{"canonicalOrigin":"https://docs.acme.com"}}"#,
        )
        .write("index.md", "---\ntitle: Home\n---\n# Home\n")
        .write(
            "guides/install.md",
            "---\ntitle: Install\nupdated: 2026-03-04\n---\n# Install\n",
        )
        .write("guides/upgrade.md", "---\ntitle: Upgrade\n---\n# Upgrade\n");
    project
}

fn build(project: &Project) -> engine::Report {
    let vfs = OsVfs::new(project.path());
    engine::build(
        &vfs,
        &History,
        project.path(),
        &Options {
            build_time: Some(CLOCK),
            ..Options::default()
        },
    )
}

#[test]
fn two_builds_of_the_same_history_write_the_same_sitemap() {
    let first = site("first");
    let second = site("second");
    let report = build(&first);
    assert!(!report.failed(false), "{:?}", report.diagnostics);
    build(&second);

    assert_eq!(
        first.read_dist("sitemap.xml"),
        second.read_dist("sitemap.xml")
    );
}

#[test]
fn a_rebuild_over_a_warm_cache_writes_the_same_sitemap() {
    let project = site("rebuild");
    build(&project);
    let first = project.read_dist("sitemap.xml");
    build(&project);
    assert_eq!(first, project.read_dist("sitemap.xml"));
}

#[test]
fn every_entry_is_absolute_under_the_canonical_origin() {
    let project = site("absolute");
    build(&project);
    let sitemap = project.read_dist("sitemap.xml");
    for route in ["/", "/guides/install", "/guides/upgrade"] {
        assert!(
            sitemap.contains(&format!("<loc>https://docs.acme.com{route}</loc>")),
            "{route} is not in {sitemap}"
        );
    }
}

#[test]
fn a_page_with_no_date_is_dated_from_the_build_clock() {
    let project = site("dates");
    build(&project);
    let sitemap = project.read_dist("sitemap.xml");
    assert!(sitemap.contains("<lastmod>2026-09-15"), "{sitemap}");
    assert!(
        sitemap.contains("<lastmod>2026-03-04"),
        "the author's date wins where there is one: {sitemap}"
    );
}
