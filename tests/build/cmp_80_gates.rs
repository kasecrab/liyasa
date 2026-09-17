//! CMP-80 and CMP-82 through a whole build: what a gated block actually ships.
//!
//! `crates/liyasa-components/tests/it/gates.rs` pins the component's own
//! behaviour given a variant. This file is the other half — whether the build
//! hands it one — because the component cannot tell the difference between "no
//! variant was passed" and "the variant admits nothing", and the fix is safe in
//! both cases but finished in only one.
//!
//! Until `liyasa-build` passes the render job's variant, every gated block is
//! withheld from every reader, which is a silent failure: a site that used
//! visibility gates loses content and nothing says why. These tests pin that
//! state so the fix flips them rather than passing unnoticed.

use std::fs;
use std::path::{Path, PathBuf};

use liyasa_build::engine::{self, Options};
use liyasa_build::git::NoGit;
use liyasa_config::vfs::OsVfs;

const SECRET: &str = "The admin console lives at /internal/console.";
const PUBLIC: &str = "Everyone may read this paragraph.";

struct Project(PathBuf);

impl Project {
    fn new(name: &str) -> Self {
        let root =
            std::env::temp_dir().join(format!("liyasa-cmp-80-{name}-{}", std::process::id()));
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

    fn out(&self, path: &str) -> PathBuf {
        self.0.join("dist").join(path)
    }
}

impl Drop for Project {
    fn drop(&mut self) {
        let _ = fs::remove_dir_all(&self.0);
    }
}

/// A one-page site whose page carries `body` after a heading.
fn site(name: &str, body: &str) -> Project {
    let project = Project::new(name);
    project
        .write(
            "liyasa.json",
            r#"{"name":"Acme docs","seo":{"canonicalOrigin":"https://docs.acme.com"}}"#,
        )
        .write(
            "index.md",
            &format!("---\ntitle: Home\n---\n# Home\n\n{PUBLIC}\n\n{body}\n"),
        );
    project
}

fn build(project: &Project) {
    let vfs = OsVfs::new(project.path());
    let report = engine::build(&vfs, &NoGit, project.path(), &Options::default());
    assert!(
        !report.failed(false),
        "the build itself must succeed: {:?}",
        report.diagnostics
    );
}

fn html(project: &Project) -> String {
    fs::read_to_string(project.out("index.html")).expect("the home page")
}

fn markdown(project: &Project) -> String {
    fs::read_to_string(project.out("index.md")).expect("the markdown twin")
}

/// The defect this packet fixed: before it, the gated paragraph was in the
/// HTML, in the twin and in the index for everyone. It must never come back,
/// whatever the build does with variants, so this test has no "yet" about it.
#[test]
fn a_group_gated_block_is_not_served_to_an_anonymous_reader() {
    let project = site(
        "anonymous",
        &format!(":::visibility{{groups=[\"admin\"]}}\n{SECRET}\n:::"),
    );
    build(&project);

    let page = html(&project);
    assert!(page.contains(PUBLIC), "the ungated paragraph is missing");
    assert!(
        !page.contains(SECRET),
        "a group-gated block reached an anonymous reader: {page}"
    );

    let twin = markdown(&project);
    assert!(
        !twin.contains(SECRET),
        "a group-gated block reached the markdown twin: {twin}"
    );
}

#[test]
fn a_region_gated_block_is_not_served_to_a_reader_with_no_region() {
    let project = site(
        "region",
        &format!(":::region{{only=[\"eu\"]}}\n{SECRET}\n:::"),
    );
    build(&project);

    let page = html(&project);
    assert!(page.contains(PUBLIC));
    assert!(!page.contains(SECRET), "{page}");
    assert!(!markdown(&project).contains(SECRET));
}

/// The half that does not work yet. `liyasa-build` builds every component's
/// `Shared` with `Shared::new(&reference).site(..).nonce(..)` and never calls
/// `.variant(..)`, so the render job's variant never reaches the component and
/// every gated block is withheld from every reader — including the one the gate
/// names. `RenderJob` already carries the variant; the five `Shared::new` sites
/// are in `liyasa-build`'s `render/blocks.rs`, `agents/size.rs` and
/// `agents/markdown/mod.rs`, which is WP-06's crate. Pinned here so the fix
/// flips this test.
///
/// Only the sites whose output is keyed by the variant may take a real one. The
/// markdown twin and the search index are written once and served to everyone
/// (§6.6.4), so they keep the default and keep withholding.
#[test]
fn a_gated_block_reaches_nobody_yet_because_the_build_passes_no_variant() {
    let project = site(
        "admitted",
        &format!(":::visibility{{groups=[\"admin\"]}}\n{SECRET}\n:::"),
    );
    build(&project);
    let page = html(&project);
    assert!(
        !page.contains(SECRET),
        "WP-06 passed the render job's variant through to the component: this \
         site should now build an admin variant that DOES carry the block, so \
         assert its presence in that variant's output instead. {page}"
    );
}

/// An ungated block is not affected by any of this, which is what says the
/// withholding above is the gate working rather than the renderer failing.
#[test]
fn an_ungated_block_still_reaches_everyone() {
    let project = site("ungated", &format!(":::visibility\n{SECRET}\n:::"));
    build(&project);
    let page = html(&project);
    assert!(
        page.contains(SECRET),
        "a visibility block with no gate is not gated: {page}"
    );
}
