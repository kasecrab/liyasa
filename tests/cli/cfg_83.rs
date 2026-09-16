//! CFG-83: the `build.*` keys a build obeys — where the output lands, whether
//! drafts are in it, what prefix every path carries, and how hard a broken
//! link lands.

use std::fs;
use std::path::{Path, PathBuf};

use liyasa_build::engine::{self, Options};
use liyasa_build::git::NoGit;
use liyasa_config::vfs::OsVfs;
use liyasa_core::diagnostics::Severity;

struct Project(PathBuf);

impl Project {
    fn new(name: &str) -> Self {
        let root =
            std::env::temp_dir().join(format!("liyasa-cfg-83-{name}-{}", std::process::id()));
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

    fn out(&self, directory: &str, path: &str) -> PathBuf {
        self.0.join(directory).join(path)
    }
}

impl Drop for Project {
    fn drop(&mut self) {
        let _ = fs::remove_dir_all(&self.0);
    }
}

/// A site whose `build` block is whatever the case under test needs.
fn site(name: &str, build_block: &str) -> Project {
    let project = Project::new(name);
    project
        .write(
            "liyasa.json",
            &format!(
                r#"{{"name":"Acme docs","seo":{{"canonicalOrigin":"https://docs.acme.com"}}{build_block}}}"#
            ),
        )
        .write("index.md", "---\ntitle: Home\n---\n# Home\n\n[Install](guides/install)\n")
        .write("guides/install.md", "---\ntitle: Install\n---\n# Install\n")
        .write(
            "guides/next.md",
            "---\ntitle: Next\ndraft: true\n---\n# Next release\n",
        );
    project
}

fn build(project: &Project) -> engine::Report {
    let vfs = OsVfs::new(project.path());
    engine::build(
        &vfs,
        &NoGit,
        project.path(),
        &Options {
            build_time: Some(1_789_473_600),
            ..Options::default()
        },
    )
}

#[test]
fn build_output_moves_the_whole_output() {
    let project = site("output", r#","build":{"output":"public"}"#);
    let report = build(&project);
    assert!(!report.failed(false), "{:?}", report.diagnostics);
    assert!(project.out("public", "index.html").exists());
    assert!(!project.out("dist", "index.html").exists());
}

#[test]
fn a_draft_is_out_unless_drafts_are_on() {
    let without = site("drafts-off", "");
    build(&without);
    assert!(!without.out("dist", "guides/next/index.html").exists());

    let with = site("drafts-on", r#","build":{"drafts":true}"#);
    build(&with);
    assert!(with.out("dist", "guides/next/index.html").exists());
}

#[test]
fn base_path_prefixes_the_assets_and_the_markdown_twin() {
    let project = site("base", r#","build":{"basePath":"/docs"}"#);
    build(&project);
    let home = fs::read_to_string(project.out("dist", "index.html")).expect("the home page");
    assert!(
        home.contains("\"/docs/_liyasa/"),
        "the stylesheet and the runtime: {home}"
    );
    assert!(
        home.contains("\"/docs/index.md\""),
        "the Markdown twin: {home}"
    );
    assert!(
        !project.out("dist", "docs").exists(),
        "a base path is a URL prefix, not an output directory"
    );
}

/// The half that does not work yet: a link written in a page comes out
/// site-absolute without the prefix, so every in-content link 404s under a base
/// path. `links::with_anchor` does prefix, and `links::Table` is handed
/// `settings.base_path`, so the prefix is lost between them — `liyasa-build` is
/// WP-06's crate. Pinned here so the fix flips this test.
#[test]
fn a_link_written_in_a_page_is_not_prefixed_yet() {
    let project = site("base-links", r#","build":{"basePath":"/docs"}"#);
    build(&project);
    let home = fs::read_to_string(project.out("dist", "index.html")).expect("the home page");
    assert!(
        home.contains("href=\"/guides/install\""),
        "WP-06 fixed the base path on page links: assert the prefix here instead. {home}"
    );
    assert!(!home.contains("href=\"/docs/guides/install\""), "{home}");
}

#[test]
fn strict_links_decides_whether_a_broken_link_fails_the_build() {
    let strict = site("strict", r#","build":{"strictLinks":true}"#);
    strict.write(
        "index.md",
        "---\ntitle: Home\n---\n# Home\n\n[Gone](guides/gone)\n",
    );
    let report = build(&strict);
    let broken = report
        .diagnostics
        .iter()
        .find(|diagnostic| diagnostic.code.as_str() == "E0401")
        .expect("the broken link is reported");
    assert_eq!(broken.severity, Severity::Error);
    assert!(report.failed(false), "a broken link fails a strict build");

    let lenient = site("lenient", r#","build":{"strictLinks":false}"#);
    lenient.write(
        "index.md",
        "---\ntitle: Home\n---\n# Home\n\n[Gone](guides/gone)\n",
    );
    let report = build(&lenient);
    let broken = report
        .diagnostics
        .iter()
        .find(|diagnostic| diagnostic.code.as_str() == "E0401")
        .expect("the broken link is still reported");
    assert_eq!(broken.severity, Severity::Warning);
    assert!(!report.failed(false), "{:?}", report.diagnostics);
}
