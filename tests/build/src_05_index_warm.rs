//! SRC-05, the half a clean-build test cannot see: a warm build's index.
//!
//! The index is built from each page's rendered AST, and a cache hit renders
//! nothing. If the document is not cached beside the HTML, the second build of
//! an unchanged site indexes zero pages — a full index cold and an empty one
//! warm, which is worse than no index, because every test that builds once
//! passes. This is the diagnostics defect of RFC 0904 one layer up, so it gets
//! the same shape of test.

use std::fs;
use std::path::PathBuf;

use liyasa_build::engine::{self, Options};
use liyasa_build::git::NoGit;
use liyasa_config::vfs::OsVfs;

struct Project(PathBuf);

impl Drop for Project {
    fn drop(&mut self) {
        let _ = fs::remove_dir_all(&self.0);
    }
}

/// `name` discriminates the fixture, as every other build fixture in this
/// directory does. Keyed on the process id alone, the two tests in this file
/// shared one directory: under nextest that is invisible because each test is
/// its own process, but `cargo test` runs them as threads in one, so
/// `remove_dir_all` below ran while the other test was mid-build and the
/// build reported "cannot read `liyasa.json`". CI runs `cargo test`.
fn site(name: &str) -> Project {
    let root =
        std::env::temp_dir().join(format!("liyasa-src-05-warm-{name}-{}", std::process::id()));
    let _ = fs::remove_dir_all(&root);
    fs::create_dir_all(root.join("guides")).expect("a project directory");
    fs::write(
        root.join("liyasa.json"),
        r#"{"name":"Acme docs","seo":{"canonicalOrigin":"https://docs.acme.com"}}"#,
    )
    .expect("config");
    fs::write(
        root.join("index.md"),
        "---\ntitle: Home\n---\n# Home\n\nThe home page of the Acme documentation.\n",
    )
    .expect("a page");
    fs::write(
        root.join("guides/install.md"),
        "---\ntitle: Install\n---\n# Install\n\nRun the installer and answer two questions.\n",
    )
    .expect("a page");
    Project(root)
}

fn build(project: &Project) -> engine::Report {
    let vfs = OsVfs::new(&project.0);
    engine::build(
        &vfs,
        &NoGit,
        &project.0,
        &Options {
            build_time: Some(1_789_473_600),
            ..Options::default()
        },
    )
}

/// Every file the build wrote under `search-index/`, by path, with its bytes.
fn index_files(project: &Project) -> Vec<(String, Vec<u8>)> {
    let root = project.0.join("dist/search-index");
    let mut out = Vec::new();
    let Ok(entries) = fs::read_dir(&root) else {
        return out;
    };
    for entry in entries {
        let entry = entry.expect("a directory entry");
        if entry.path().is_file() {
            out.push((
                entry.file_name().to_string_lossy().into_owned(),
                fs::read(entry.path()).expect("an index file"),
            ));
        }
    }
    out.sort();
    out
}

#[test]
fn a_warm_build_writes_the_same_index_as_a_cold_one() {
    let project = site("warm");

    let cold = build(&project);
    assert!(!cold.failed(false), "{:?}", cold.diagnostics);
    let cold_files = index_files(&project);
    assert!(
        cold_files.iter().any(|(name, _)| name == "manifest.json"),
        "a cold build writes a manifest: {:?}",
        cold_files.iter().map(|(name, _)| name).collect::<Vec<_>>()
    );

    let warm = build(&project);
    assert!(!warm.failed(false), "{:?}", warm.diagnostics);
    let warm_files = index_files(&project);

    // Equality, not "the warm one is non-empty": an index that shrinks on a
    // second build is the defect, and a subset assertion would pass for it.
    let names = |files: &[(String, Vec<u8>)]| -> Vec<String> {
        files.iter().map(|(name, _)| name.clone()).collect()
    };
    assert_eq!(names(&warm_files), names(&cold_files));
    for ((cold_name, cold_bytes), (_, warm_bytes)) in cold_files.iter().zip(warm_files.iter()) {
        assert_eq!(
            cold_bytes.len(),
            warm_bytes.len(),
            "{cold_name} differs between a cold and a warm build"
        );
    }
}

/// The empty case the RFC decides deliberately: a site with nothing indexable
/// still gets an index, so `E0016` means "no index" and never "nothing to
/// index".
#[test]
fn a_site_with_no_indexable_page_still_gets_a_manifest() {
    let project = site("empty");
    fs::write(
        project.0.join("index.md"),
        "---\ntitle: Home\nhidden: true\n---\n# Home\n\nNot indexed.\n",
    )
    .expect("a page");
    fs::write(
        project.0.join("guides/install.md"),
        "---\ntitle: Install\nhidden: true\n---\n# Install\n\nNot indexed either.\n",
    )
    .expect("a page");

    let report = build(&project);
    assert!(!report.failed(false), "{:?}", report.diagnostics);
    assert!(
        index_files(&project)
            .iter()
            .any(|(name, _)| name == "manifest.json"),
        "an empty index is still an index"
    );
}
