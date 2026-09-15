//! CM-120 and CM-121: a changelog page's `::update` entries become one stream
//! with stable anchors, an RSS feed, an Atom feed, and a JSON feed.

use std::fs;
use std::path::{Path, PathBuf};

use liyasa_build::engine::{self, Options};
use liyasa_build::git::NoGit;
use liyasa_config::vfs::OsVfs;

struct Project(PathBuf);

impl Project {
    fn new(name: &str) -> Self {
        let root =
            std::env::temp_dir().join(format!("liyasa-cm-120-{name}-{}", std::process::id()));
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

fn changelog_site(name: &str) -> Project {
    let project = Project::new(name);
    project
        .write(
            "liyasa.json",
            r#"{"name":"Acme docs","seo":{"canonicalOrigin":"https://docs.acme.com"}}"#,
        )
        .write("index.md", "---\ntitle: Home\n---\n# Home\n")
        .write(
            "changelog.md",
            // `plan/rfcs/0603-update-directive-spelling.md`: CM-120's example
            // writes the leaf form, and the component is a container.
            "---\ntitle: Changelog\n---\n# Changelog\n\n\
             :::update{date=\"2026-09-01\" version=\"2.3\" labels=[\"api\",\"billing\"]}\n\
             Billing endpoints moved to `/v2/billing`.\n\
             :::\n\n\
             :::update{date=\"2026-08-01\" version=\"2.2\" labels=[\"api\"]}\n\
             Added the usage endpoint.\n\
             :::\n",
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
fn every_entry_renders_with_a_stable_anchor() {
    let project = changelog_site("anchors");
    let report = build(&project);
    assert!(!report.failed(false), "{:?}", report.diagnostics);

    let html = project.read_dist("changelog/index.html");
    assert!(html.contains("id=\"2026-09-01-2-3\""), "{html}");
    assert!(html.contains("Billing endpoints moved"), "{html}");
    assert!(html.contains("data-date=\"2026-08-01\""), "{html}");
}

#[test]
fn the_rss_feed_is_written_with_both_entries_newest_first() {
    let project = changelog_site("rss");
    build(&project);
    let rss = project.read_dist("changelog/rss.xml");
    let first = rss.find("2026-09-01").expect("the newer entry");
    let second = rss.find("2026-08-01").expect("the older entry");
    assert!(first < second, "newest first");
    assert!(rss.contains("<category>billing</category>"), "{rss}");
    assert!(
        rss.contains("https://docs.acme.com/changelog#2026-09-01-2-3"),
        "{rss}"
    );
}

#[test]
fn the_atom_and_json_feeds_are_written_too() {
    let project = changelog_site("feeds");
    build(&project);
    let atom = project.read_dist("changelog/atom.xml");
    assert!(
        atom.contains("<feed xmlns=\"http://www.w3.org/2005/Atom\">"),
        "{atom}"
    );

    let json: serde_json::Value =
        serde_json::from_str(&project.read_dist("changelog/feed.json")).expect("a JSON feed");
    assert_eq!(json["items"].as_array().map(Vec::len), Some(2));
    assert_eq!(json["items"][0]["tags"][0], "api");
}

#[test]
fn a_site_with_no_entries_writes_no_feed() {
    let project = Project::new("empty");
    project
        .write("liyasa.json", r#"{"name":"Acme docs"}"#)
        .write("index.md", "---\ntitle: Home\n---\n# Home\n");
    let report = build(&project);
    assert!(!report.wrote("changelog/rss.xml"), "{:?}", report.written);
}

#[test]
fn the_feeds_survive_a_warm_rebuild() {
    let project = changelog_site("warm");
    build(&project);
    let first = project.read_dist("changelog/rss.xml");
    let report = build(&project);
    assert_eq!(report.cache_misses, 0, "{:?}", report.diagnostics);
    assert_eq!(project.read_dist("changelog/rss.xml"), first);
}

#[test]
fn a_directory_of_dated_files_renders_as_one_stream() {
    let project = Project::new("directory");
    project
        .write(
            "liyasa.json",
            r#"{"name":"Acme docs","seo":{"canonicalOrigin":"https://docs.acme.com"}}"#,
        )
        .write("index.md", "---\ntitle: Home\n---\n# Home\n")
        .write(
            "changelog/2026-09-01-billing.md",
            "---\ntitle: Billing moved\nkeywords: [api, billing]\ndescription: Billing endpoints moved.\n---\n# Billing moved\n",
        )
        .write(
            "changelog/2026-08-01-usage.md",
            "---\ntitle: Usage endpoint\ndescription: Added the usage endpoint.\n---\n# Usage endpoint\n",
        );

    let report = build(&project);
    assert!(!report.failed(false), "{:?}", report.diagnostics);

    // Each dated file is still its own page.
    assert!(
        project
            .read_dist("changelog/2026-09-01-billing/index.html")
            .contains("Billing moved")
    );

    // And the build writes the stream that indexes them, newest first. The
    // sidebar lists the same pages alphabetically, so the order is read from
    // the stream itself rather than from the whole page.
    let page = project.read_dist("changelog/index.html");
    let at = page
        .find("data-liyasa=\"changelog\"")
        .expect("the stream is in the page");
    let stream = &page[at..];
    let newer = stream.find("2026-09-01").expect("the newer entry");
    let older = stream.find("2026-08-01").expect("the older entry");
    assert!(newer < older, "newest first");
    assert!(stream.contains("data-labels=\"api,billing\""), "{stream}");

    let rss = project.read_dist("changelog/rss.xml");
    assert!(
        rss.contains("/changelog/2026-09-01-billing#2026-09-01-billing-moved"),
        "{rss}"
    );
}

#[test]
fn a_changelog_page_of_its_own_is_not_replaced_by_a_generated_stream() {
    let project = changelog_site("own-page");
    project.write(
        "changelog/2026-07-01-older.md",
        "---\ntitle: Older\ndescription: An older entry.\n---\n# Older\n",
    );
    build(&project);
    // `changelog.md` renders `/changelog`; the generated index must not
    // overwrite it.
    let page = project.read_dist("changelog/index.html");
    assert!(page.contains("Billing endpoints moved"), "{page}");
}
