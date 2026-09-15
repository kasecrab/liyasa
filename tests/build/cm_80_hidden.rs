//! CM-80: a `hidden: true` page is routable and in nothing else, and each
//! surface comes back on its own with `search: true`, `ai: true`, or
//! `noindex: false`.

use std::fs;
use std::path::{Path, PathBuf};

use liyasa_build::engine::{self, Options};
use liyasa_build::git::NoGit;
use liyasa_config::vfs::OsVfs;

struct Project(PathBuf);

impl Project {
    fn new(name: &str) -> Self {
        let root = std::env::temp_dir().join(format!("liyasa-cm-80-{name}-{}", std::process::id()));
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

    fn dist(&self, path: &str) -> PathBuf {
        self.0.join("dist").join(path)
    }

    fn read_dist(&self, path: &str) -> String {
        fs::read_to_string(self.dist(path))
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
        .write(
            "liyasa.json",
            r#"{"name":"Acme docs","seo":{"canonicalOrigin":"https://docs.acme.com"}}"#,
        )
        .write("index.md", "---\ntitle: Home\n---\n# Home\n")
        .write(
            "internal/runbook.md",
            "---\ntitle: Runbook\nhidden: true\n---\n# Runbook\n\nOn-call steps.\n",
        )
        .write(
            "internal/searchable.md",
            "---\ntitle: Searchable\nhidden: true\nsearch: true\n---\n# Searchable\n",
        )
        .write(
            "internal/listed.md",
            "---\ntitle: Listed\nhidden: true\nnoindex: false\n---\n# Listed\n",
        )
        .write(
            "internal/for-agents.md",
            "---\ntitle: For agents\nhidden: true\nai: true\n---\n# For agents\n",
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
fn a_hidden_page_is_routable() {
    let project = site("routable");
    let report = build(&project);
    assert!(!report.failed(false), "{:?}", report.diagnostics);
    assert!(
        project
            .read_dist("internal/runbook/index.html")
            .contains("On-call steps.")
    );
}

#[test]
fn a_hidden_page_is_not_in_the_sitemap() {
    let project = site("sitemap");
    build(&project);
    let sitemap = project.read_dist("sitemap.xml");
    assert!(sitemap.contains("https://docs.acme.com/"), "{sitemap}");
    assert!(!sitemap.contains("/internal/runbook"), "{sitemap}");
    assert!(!sitemap.contains("/internal/searchable"), "{sitemap}");
}

#[test]
fn noindex_false_puts_a_hidden_page_back_in_the_sitemap() {
    let project = site("noindex");
    build(&project);
    let sitemap = project.read_dist("sitemap.xml");
    assert!(sitemap.contains("/internal/listed"), "{sitemap}");
}

#[test]
fn a_hidden_page_is_not_in_llms_txt_or_the_markdown_routes() {
    let project = site("llms");
    build(&project);
    let llms = project.read_dist("llms.txt");
    assert!(!llms.contains("/internal/runbook"), "{llms}");
    assert!(
        !project.dist("internal/runbook.md").exists(),
        "a hidden page has no agent Markdown route"
    );
    assert!(project.dist("index.md").exists(), "a public page does");
}

#[test]
fn a_hidden_page_is_not_in_navigation() {
    let project = site("navigation");
    build(&project);
    let home = project.read_dist("index.html");
    assert!(!home.contains("/internal/runbook"), "{home}");
}

#[test]
fn the_manifest_says_which_pages_are_hidden() {
    let project = site("manifest");
    let report = build(&project);
    let manifest = report.manifest.as_ref().expect("a manifest");
    let hidden = manifest
        .route(&liyasa_core::ids::Route::new("/internal/runbook"))
        .expect("the hidden page is in the manifest");
    assert!(hidden.hidden);
    let home = manifest
        .route(&liyasa_core::ids::Route::new("/"))
        .expect("the home page");
    assert!(!home.hidden);
}

#[test]
fn ai_true_puts_a_hidden_page_back_in_the_agent_surfaces() {
    let project = site("ai");
    build(&project);
    let llms = project.read_dist("llms.txt");
    assert!(llms.contains("/internal/for-agents"), "{llms}");
    assert!(
        project.dist("internal/for-agents.md").exists(),
        "its Markdown route is served again"
    );
    // The other switches stay off.
    let sitemap = project.read_dist("sitemap.xml");
    assert!(!sitemap.contains("/internal/for-agents"), "{sitemap}");
}
