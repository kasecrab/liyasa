//! CM-70, CM-71 and CM-74 in a build: a page that includes a snippet renders
//! the snippet's content, a cycle between snippets is reported once rather
//! than as a recursion limit, and a snippet is never a page of its own.
//!
//! `liyasa-markdown` resolves an include against the build's `SourceMap`
//! (`expand::record` looks the name up with `map.find`), so the engine has to
//! intern `snippets/` before it renders anything.

use std::fs;
use std::path::{Path, PathBuf};

use liyasa_build::engine::{self, Options};
use liyasa_build::git::NoGit;
use liyasa_config::vfs::OsVfs;
use liyasa_core::ids::Route;

struct Project(PathBuf);

impl Project {
    fn new(name: &str) -> Self {
        let root = std::env::temp_dir().join(format!("liyasa-cm-70-{name}-{}", std::process::id()));
        let _ = fs::remove_dir_all(&root);
        fs::create_dir_all(&root).expect("a project directory");
        let project = Self(root);
        project.write("liyasa.json", r#"{"name":"Acme docs"}"#);
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
fn cm_70_a_page_includes_a_snippet_by_stem_and_by_path() {
    let project = Project::new("include");
    project
        .write("snippets/note.md", "Mind the **gap**.\n")
        .write(
            "index.md",
            "---\ntitle: Home\n---\n# Home\n\n{% snippet \"note\" %}\n\n\
             {% include \"snippets/note.md\" %}\n",
        );

    let report = build(&project);
    assert!(!report.failed(false), "{:?}", report.diagnostics);
    let home = project.read_dist("index.html");
    assert_eq!(
        home.matches("Mind the ").count(),
        2,
        "both spellings render the snippet: {home}"
    );
    assert!(home.contains("<strong>gap</strong>"), "{home}");
}

#[test]
fn cm_70_a_snippet_takes_variables_from_the_call() {
    let project = Project::new("variables");
    project
        .write("snippets/price.md", "The plan costs {{ amount }}.\n")
        .write(
            "index.md",
            "---\ntitle: Home\n---\n# Home\n\n{% snippet \"price\" amount=\"$10\" %}\n",
        );

    let report = build(&project);
    assert!(!report.failed(false), "{:?}", report.diagnostics);
    assert!(
        project
            .read_dist("index.html")
            .contains("The plan costs $10.")
    );
}

#[test]
fn cm_71_a_snippet_may_include_another_snippet() {
    let project = Project::new("nested");
    project
        .write("snippets/outer.md", "outer, then {% snippet \"inner\" %}\n")
        .write("snippets/inner.md", "inner\n")
        .write(
            "index.md",
            "---\ntitle: Home\n---\n# Home\n\n{% snippet \"outer\" %}\n",
        );

    let report = build(&project);
    assert!(!report.failed(false), "{:?}", report.diagnostics);
    let home = project.read_dist("index.html");
    assert!(home.contains("outer, then"), "{home}");
    assert!(home.contains("inner"), "{home}");
}

#[test]
fn cm_71_a_cycle_between_snippets_is_reported_by_name() {
    let project = Project::new("cycle");
    project
        .write("snippets/a.md", "a then {% snippet \"b\" %}\n")
        .write("snippets/b.md", "b then {% snippet \"a\" %}\n")
        .write(
            "index.md",
            "---\ntitle: Home\n---\n# Home\n\n{% snippet \"a\" %}\n",
        );

    let report = build(&project);
    assert!(
        report
            .diagnostics
            .iter()
            .any(|diagnostic| diagnostic.code.as_str() == "E0206"),
        "a cycle is E0206, not a recursion limit: {:?}",
        report.diagnostics
    );
}

#[test]
fn cm_74_a_snippet_is_not_a_page() {
    let project = Project::new("not-a-page");
    project.write("snippets/note.md", "Mind the gap.\n").write(
        "index.md",
        "---\ntitle: Home\n---\n# Home\n\n{% snippet \"note\" %}\n",
    );

    let report = build(&project);
    assert_eq!(report.pages, 1);
    let manifest = report.manifest.as_ref().expect("a manifest");
    assert!(manifest.route(&Route::new("/snippets/note")).is_none());
    assert!(!project.dist("snippets/note/index.html").exists());
    // Its content is in the page that uses it, which is what CM-74 asks for.
    assert!(project.read_dist("index.html").contains("Mind the gap."));
}

#[test]
fn cm_74_editing_a_snippet_rebuilds_the_pages_that_use_it() {
    let project = Project::new("invalidate");
    project.write("snippets/note.md", "First wording.\n").write(
        "index.md",
        "---\ntitle: Home\n---\n# Home\n\n{% snippet \"note\" %}\n",
    );
    build(&project);

    project.write("snippets/note.md", "Second wording.\n");
    let second = build(&project);
    assert!(!second.failed(false), "{:?}", second.diagnostics);
    assert!(
        project.read_dist("index.html").contains("Second wording."),
        "the page re-rendered with the new snippet"
    );
}
