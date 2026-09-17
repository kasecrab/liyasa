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

// ---- W0723: a reserved directory swallows a page ----

/// CM-03 reserves several directory names, and a `.md` in one of them is not a
/// page: no route, no navigation, no sitemap, nothing in `dist/`. The
/// reservation is right; saying nothing about it is not.
#[test]
fn a_page_in_a_reserved_directory_is_reported_once() {
    let project = Project::new("reserved");
    project
        .write(
            "liyasa.json",
            r#"{"name":"Acme docs","seo":{"canonicalOrigin":"https://docs.acme.com"}}"#,
        )
        .write("index.md", "---\ntitle: Home\n---\n# Home\n")
        .write("snippets/pricing.md", "A shared fragment.\n")
        .write("snippets/legal.md", "Another one.\n")
        .write("components/card.md", "---\ntitle: Card\n---\n# Card\n")
        .write("facts/pricing.md", "---\ntitle: Facts\n---\n# Facts\n")
        .write("theme/notes.md", "---\ntitle: Theme\n---\n# Theme\n")
        .write("assets/manual.md", "---\ntitle: Manual\n---\n# Manual\n")
        .write("_drafts/next.md", "---\ntitle: Next\n---\n# Next\n");

    let report = build(&project);
    let warnings: Vec<&str> = report
        .diagnostics
        .iter()
        .filter(|diagnostic| diagnostic.code.as_str() == "W0723")
        .map(|diagnostic| diagnostic.message.as_str())
        .collect();

    // One per directory, not one per file: `snippets/` holds two.
    assert_eq!(warnings.len(), 5, "{warnings:#?}");
    for name in ["snippets", "components", "facts", "theme", "assets"] {
        assert!(
            warnings.iter().any(|message| message.contains(name)),
            "{name} is not named in {warnings:#?}"
        );
    }
    // `_drafts/` is withheld by the same walk and is not reported: the author
    // typed the underscore, so the file is where they meant to put it. The
    // repository's own `docs/errors/_notes/` is the case that proves it — every
    // note in it would be a warning on every build.
    assert!(
        !warnings.iter().any(|message| message.contains("_drafts")),
        "the `_` convention is deliberate, not a defect: {warnings:#?}"
    );
    assert!(
        warnings
            .iter()
            .any(|message| message.contains("snippets") && message.contains('2')),
        "the count says how many were swallowed: {warnings:#?}"
    );

    // The reservation itself is unchanged: none of them is routed.
    assert_eq!(report.pages, 1);
}

/// `public/` is an asset directory whose Markdown *is* routed today — the walk
/// treats a page extension there as content. That is visible rather than
/// silent, so it raises nothing; this pins the difference so a later change to
/// either half is deliberate.
#[test]
fn a_page_under_public_is_routed_and_raises_nothing() {
    let project = Project::new("public");
    project
        .write(
            "liyasa.json",
            r#"{"name":"Acme docs","seo":{"canonicalOrigin":"https://docs.acme.com"}}"#,
        )
        .write("index.md", "---\ntitle: Home\n---\n# Home\n")
        .write("public/guide.md", "---\ntitle: Guide\n---\n# Guide\n");

    let report = build(&project);
    assert_eq!(report.pages, 2);
    assert!(
        !report
            .diagnostics
            .iter()
            .any(|diagnostic| diagnostic.code.as_str() == "W0723"),
        "{:?}",
        report.diagnostics
    );
    assert!(project.dist("public/guide/index.html").exists());
}
