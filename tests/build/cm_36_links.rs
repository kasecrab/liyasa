//! CM-35 and CM-36: the build resolves the link and image forms
//! `liyasa-markdown` can only classify, and reports the ones that go nowhere.

use std::fs;
use std::path::{Path, PathBuf};

use liyasa_build::engine::{self, Options};
use liyasa_build::git::NoGit;
use liyasa_config::vfs::OsVfs;

struct Project(PathBuf);

impl Project {
    fn new(name: &str) -> Self {
        let root = std::env::temp_dir().join(format!("liyasa-cm-36-{name}-{}", std::process::id()));
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

fn site(name: &str, body: &str) -> Project {
    let project = Project::new(name);
    project
        .write("liyasa.json", r#"{"name":"Acme docs"}"#)
        .write("index.md", "---\ntitle: Home\n---\n# Home\n")
        .write(
            "guides/upgrade.md",
            "---\ntitle: Upgrade\n---\n# Upgrade\n\n## Breaking changes\n",
        )
        .write("assets/hero.png", "\u{89}PNG\r\n\u{1a}\n fixture")
        .write(
            "guides/install.md",
            &format!("---\ntitle: Install\n---\n# Install\n\n{body}\n"),
        );
    project
}

#[test]
fn a_relative_link_becomes_the_route_it_means() {
    let project = site("relative", "[Upgrade](./upgrade.md)");
    let report = build(&project);
    assert!(!report.failed(false), "{:?}", report.diagnostics);
    assert!(
        project
            .read_dist("guides/install/index.html")
            .contains("href=\"/guides/upgrade\""),
        "{}",
        project.read_dist("guides/install/index.html")
    );
}

#[test]
fn a_link_to_nowhere_fails_the_build() {
    let project = site("broken", "[Ghost](./ghost.md)");
    let report = build(&project);
    assert!(
        report
            .diagnostics
            .iter()
            .any(|diagnostic| diagnostic.code.as_str() == "E0401"),
        "{:?}",
        report.diagnostics
    );
    assert!(report.failed(false));
}

#[test]
fn a_fragment_on_the_page_itself_is_checked() {
    let project = site("fragment", "# Install\n\n## Steps\n\n[Steps](#steps)");
    let report = build(&project);
    assert!(!report.failed(false), "{:?}", report.diagnostics);

    let broken = site("fragment-broken", "[Nowhere](#nowhere)");
    let report = build(&broken);
    assert!(
        report
            .diagnostics
            .iter()
            .any(|diagnostic| diagnostic.code.as_str() == "E0402"),
        "{:?}",
        report.diagnostics
    );
}

#[test]
fn an_image_resolves_to_the_file_the_build_copies() {
    let project = site("image", "![A hero](/assets/hero.png)");
    let report = build(&project);
    assert!(!report.failed(false), "{:?}", report.diagnostics);
    assert!(
        project
            .read_dist("guides/install/index.html")
            .contains("/assets/hero.png")
    );
}

#[test]
fn a_missing_image_is_reported() {
    let project = site("missing-image", "![Gone](./missing.png)");
    let report = build(&project);
    assert!(
        report
            .diagnostics
            .iter()
            .any(|diagnostic| diagnostic.code.as_str() == "E0403"),
        "{:?}",
        report.diagnostics
    );
}

#[test]
fn lenient_link_checking_keeps_the_build_green() {
    let project = Project::new("lenient");
    project
        .write(
            "liyasa.json",
            r#"{"name":"Acme docs","build":{"strictLinks":false}}"#,
        )
        .write(
            "index.md",
            "---\ntitle: Home\n---\n# Home\n\n[Ghost](./ghost.md)\n",
        );
    let report = build(&project);
    assert!(!report.failed(false), "{:?}", report.diagnostics);
    assert!(
        report
            .diagnostics
            .iter()
            .any(|diagnostic| diagnostic.code.as_str() == "E0401")
    );
}

#[test]
fn an_external_link_is_left_as_written() {
    let project = site("external", "[Status](https://status.acme.com)");
    let report = build(&project);
    assert!(!report.failed(false), "{:?}", report.diagnostics);
    assert!(
        project
            .read_dist("guides/install/index.html")
            .contains("https://status.acme.com")
    );
}

#[test]
fn a_page_id_link_survives_a_rename() {
    // `page:<id>` is the form CM-36 gives an author who expects to move a
    // page. The sanitizer lets it through on a Markdown link only (WP-03's
    // `INTERNAL_SCHEMES`), and the build rewrites it to the route.
    let project = Project::new("page-id");
    project
        .write("liyasa.json", r#"{"name":"Acme docs"}"#)
        .write("index.md", "---\ntitle: Home\n---\n# Home\n")
        .write(
            "guides/upgrade.md",
            "---\ntitle: Upgrade\nid: 01J0000000000000000000000A\n---\n# Upgrade\n",
        )
        .write(
            "guides/install.md",
            "---\ntitle: Install\n---\n# Install\n\n\
             [Upgrade](page:01J0000000000000000000000A)\n",
        );

    let report = build(&project);
    assert!(!report.failed(false), "{:?}", report.diagnostics);
    let page = project.read_dist("guides/install/index.html");
    assert!(page.contains("href=\"/guides/upgrade\""), "{page}");
    assert!(!page.contains("page:"), "{page}");
}

#[test]
fn a_page_id_that_names_nothing_is_reported() {
    let project = Project::new("page-id-missing");
    project
        .write("liyasa.json", r#"{"name":"Acme docs"}"#)
        .write(
            "index.md",
            "---\ntitle: Home\n---\n# Home\n\n[Gone](page:01J0000000000000000000000B)\n",
        );
    let report = build(&project);
    assert!(
        report
            .diagnostics
            .iter()
            .any(|diagnostic| diagnostic.code.as_str() == "E0401"),
        "{:?}",
        report.diagnostics
    );
}

#[test]
fn an_image_may_not_use_the_page_scheme() {
    // Deliberate asymmetry: only a Markdown link is rewritten, so an image
    // `src` carrying `page:` is refused during parse rather than shipped as a
    // dead URL.
    let project = Project::new("page-id-image");
    project
        .write("liyasa.json", r#"{"name":"Acme docs"}"#)
        .write(
            "index.md",
            "---\ntitle: Home\n---\n# Home\n\n![Diagram](page:01J0000000000000000000000A)\n",
        );
    let report = build(&project);
    assert!(
        report
            .diagnostics
            .iter()
            .any(|diagnostic| diagnostic.code.as_str() == "E0304"),
        "{:?}",
        report.diagnostics
    );
}
