//! CMP-80 and CMP-82 through a whole build: what a gated block ships to a site
//! that declares no variants.
//!
//! Three files cover this between them and each one is load-bearing:
//!
//! - `crates/liyasa-components/tests/it/gates.rs` — what a component does with
//!   a variant it is handed.
//! - `tests/build/cmp_82_variants.rs` (WP-06's) — that the engine hands the
//!   right variant to the right page, which needs a site declaring more than
//!   one variant to be visible at all.
//! - this file — the single-variant site: that the default variant admits
//!   nothing gated, and that a gate written as a directive prop alone declares
//!   no variant and so gates nothing into existence.
//!
//! The division matters because of how this file was wrong before. It used to
//! carry a test claiming to pin the half-fixed state — the build not passing
//! the render job's variant — on a fixture with a `groups` directive prop and
//! no `groups` front matter. Variants come from front matter (`variants::syntactic`
//! scans template segments; a directive prop creates none), so that site builds
//! exactly one variant, the default, which correctly admits nothing both before
//! and after the fix. The test passed for a reason unrelated to its name and
//! would never have gone red. A check that passes for the wrong reason reads
//! exactly like a check that passes, which is the whole shape of this defect.
//!
//! So nothing here claims to observe the variant wiring. The single-variant
//! facts below are what this fixture can actually see; `cmp_82_variants.rs`
//! owns the rest and is where a missing variant shows up.

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

/// A one-page site whose page carries `body` after a heading, and which
/// declares nothing in front matter — so the build has one variant to render.
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

/// Every variant of the home page the build wrote, by file name.
fn home_variants(project: &Project) -> Vec<String> {
    let mut names: Vec<String> = fs::read_dir(project.out(""))
        .expect("dist")
        .map(|entry| {
            entry
                .expect("a directory entry")
                .file_name()
                .to_string_lossy()
                .into_owned()
        })
        .filter(|name| name == "index.html" || name.starts_with("index."))
        .filter(|name| name.ends_with(".html"))
        .collect();
    names.sort();
    names
}

/// The defect this packet fixed: before it, the gated paragraph was in the
/// HTML, in the twin and in the index for everyone. It must never come back,
/// whatever the build does with variants.
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

/// A gate written as a directive prop alone declares no variant.
///
/// This is the fact that made the earlier version of this file useless, so it
/// is pinned rather than left to be rediscovered. A page that gates on
/// `groups` but does not declare `groups` in its front matter is not a page
/// with an admin variant whose content is missing — it is a page with one
/// variant, and the gate names a group that page never offers.
///
/// The assertion is on the COUNT, not on absence. Absence alone is what the
/// removed test checked, and absence is true here for two different reasons at
/// once; only the count says which.
#[test]
fn a_directive_prop_alone_declares_no_variant() {
    let project = site(
        "inert",
        &format!(":::visibility{{groups=[\"admin\"]}}\n{SECRET}\n:::"),
    );
    build(&project);

    let names = home_variants(&project);
    assert_eq!(
        names,
        vec!["index.html".to_owned()],
        "a `groups` directive prop must not create a variant; front matter is \
         what declares one, and `tests/build/cmp_82_variants.rs` is where a \
         page that declares one is checked"
    );
    assert!(!html(&project).contains(SECRET));
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
    assert_eq!(home_variants(&project), vec!["index.html".to_owned()]);
}
