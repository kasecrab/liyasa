//! CFG-65: the production origin. Without it the build warns and writes no
//! agent surfaces; with it every link in them is absolute under it.

use std::fs;
use std::path::{Path, PathBuf};

use liyasa_build::engine::{self, Options};
use liyasa_build::git::NoGit;
use liyasa_config::vfs::OsVfs;
use liyasa_core::diagnostics::Severity;

const ORIGIN: &str = "https://docs.acme.com";

struct Project(PathBuf);

impl Project {
    fn new(name: &str) -> Self {
        let root =
            std::env::temp_dir().join(format!("liyasa-cfg-65-{name}-{}", std::process::id()));
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

/// The same site twice: once with `seo.canonicalOrigin`, once without.
fn site(name: &str, origin: Option<&str>) -> Project {
    let project = Project::new(name);
    let config = match origin {
        Some(origin) => format!(r#"{{"name":"Acme docs","seo":{{"canonicalOrigin":"{origin}"}}}}"#),
        None => r#"{"name":"Acme docs"}"#.to_owned(),
    };
    project
        .write("liyasa.json", &config)
        .write("index.md", "---\ntitle: Home\n---\n# Home\n")
        .write(
            "guides/install.md",
            "---\ntitle: Install\n---\n# Install\n\nRun the installer.\n",
        )
        .write(
            "changelog.md",
            "---\ntitle: Changelog\n---\n# Changelog\n\n\
             :::update{date=\"2026-09-01\" version=\"2.3\"}\n\
             Billing endpoints moved.\n\
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
fn a_build_without_an_origin_warns_and_writes_no_surfaces() {
    let project = site("missing", None);
    let report = build(&project);
    let warning = report
        .diagnostics
        .iter()
        .find(|diagnostic| diagnostic.code.as_str() == "W0131")
        .expect("the build says the origin is missing");
    assert_eq!(warning.severity, Severity::Warning);
    assert!(
        warning.message.contains("seo.canonicalOrigin"),
        "{}",
        warning.message
    );
    assert!(!report.failed(false), "a missing origin is not fatal");
    assert!(
        !project.dist("llms.txt").exists(),
        "an absolute surface without an origin would be wrong on every line"
    );
}

#[test]
fn every_llms_txt_link_is_absolute_under_the_origin() {
    let project = site("absolute", Some(ORIGIN));
    let report = build(&project);
    assert!(!report.failed(false), "{:?}", report.diagnostics);

    let llms = project.read_dist("llms.txt");
    let links: Vec<&str> = llms
        .match_indices("](")
        .map(|(at, _)| llms[at + 2..].split(')').next().unwrap_or_default())
        .collect();
    assert!(!links.is_empty(), "{llms}");
    for link in links {
        assert!(
            link.starts_with(ORIGIN),
            "`{link}` is not absolute under {ORIGIN}: {llms}"
        );
    }
}

#[test]
fn a_trailing_slash_on_the_origin_does_not_double_up() {
    let project = site("slash", Some("https://docs.acme.com/"));
    build(&project);
    let llms = project.read_dist("llms.txt");
    assert!(!llms.contains("com//"), "{llms}");
    assert!(
        llms.contains("https://docs.acme.com/guides/install"),
        "{llms}"
    );
}

#[test]
fn the_changelog_feed_is_absolute_under_the_origin_too() {
    let project = site("feed", Some(ORIGIN));
    build(&project);
    let rss = project.read_dist("changelog/rss.xml");
    assert!(rss.contains(&format!("{ORIGIN}/changelog")), "{rss}");
    assert!(
        !rss.contains("<link>/"),
        "no site-relative link in a feed: {rss}"
    );
}
