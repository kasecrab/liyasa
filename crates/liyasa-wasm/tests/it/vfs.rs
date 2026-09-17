//! ED-07: the editor's file system is seeded, then server-backed.

use std::collections::BTreeMap;
use liyasa_core::conformance::vfs::{Fixture, check};
use liyasa_core::ids::Fingerprint;
use liyasa_core::vfs::{Bytes, Vfs, VfsError, VfsPath};
use liyasa_wasm::vfs::{EditorVfs, Fetch, PRELOAD_LIMIT};

/// `/_liyasa/editor/fs/<path>`, standing in for the draft's server.
struct Draft {
    files: BTreeMap<VfsPath, Bytes>,
}

impl Draft {
    fn with(files: &[(&str, &str)]) -> Self {
        Self {
            files: files
                .iter()
                .map(|(path, text)| (VfsPath::new(path), bytes(text)))
                .collect(),
        }
    }
}

impl Fetch for Draft {
    fn fetch(&self, path: &VfsPath) -> Option<Bytes> {
        self.files.get(path).cloned()
    }
}

fn bytes(text: &str) -> Bytes {
    Bytes::from(text.as_bytes().to_vec())
}

fn seeded(files: &[(&str, &str)]) -> EditorVfs {
    EditorVfs::sealed(
        files
            .iter()
            .map(|(path, text)| (VfsPath::new(path), bytes(text))),
    )
}

#[test]
fn the_browser_vfs_satisfies_the_contract() {
    let vfs = seeded(&[
        ("guides/install.md", "# Install\n"),
        ("guides/upgrade.md", "# Upgrade\n"),
    ]);
    check(
        &vfs,
        &Fixture {
            file: (VfsPath::new("guides/install.md"), b"# Install\n".to_vec()),
            dir: VfsPath::new("guides"),
            missing: VfsPath::new("guides/nothing.md"),
            denied: None,
        },
    );
}

#[test]
fn a_path_outside_the_seed_is_fetched_once_and_then_cached() {
    let draft = Box::new(Draft::with(&[("snippets/note.md", "Mind the gap.\n")]));
    let path = VfsPath::new("snippets/note.md");
    let vfs = EditorVfs::new([(VfsPath::new("index.md"), bytes("# Home\n"))], draft);

    for _ in 0..3 {
        let read = vfs.read(&path).expect("the draft has it");
        assert_eq!(read.as_ref(), b"Mind the gap.\n");
    }
    assert_eq!(vfs.fetched(), 1, "the path was fetched more than once");
    assert_eq!(
        vfs.cached_fingerprint(&path),
        Some(Fingerprint::of("Mind the gap.\n")),
        "a fetched file is fingerprinted like a seeded one"
    );
}

#[test]
fn a_seeded_path_is_never_fetched() {
    let draft = Draft::with(&[("index.md", "the server's copy\n")]);
    let vfs = EditorVfs::new(
        [(VfsPath::new("index.md"), bytes("the draft's copy\n"))],
        Box::new(draft),
    );
    let read = vfs.read(&VfsPath::new("index.md")).expect("seeded");
    assert_eq!(read.as_ref(), b"the draft's copy\n");
    assert_eq!(vfs.fetched(), 0);
}

#[test]
fn a_path_the_draft_does_not_have_is_not_found() {
    let draft = Draft::with(&[]);
    let vfs = EditorVfs::new(std::iter::empty(), Box::new(draft));
    let path = VfsPath::new("snippets/gone.md");
    assert_eq!(vfs.read(&path), Err(VfsError::NotFound(path)));
}

#[test]
fn the_whole_content_tree_is_never_shipped_to_the_browser() {
    // The seed carries the page and what its record named; everything else is
    // asked for by name, one path at a time.
    let draft = Draft::with(&[
        ("snippets/note.md", "note\n"),
        ("snippets/unused.md", "unused\n"),
    ]);
    let vfs = EditorVfs::new(
        [(VfsPath::new("index.md"), bytes("# Home\n"))],
        Box::new(draft),
    );

    vfs.read(&VfsPath::new("index.md")).expect("seeded");
    vfs.read(&VfsPath::new("snippets/note.md"))
        .expect("the draft has it");

    let vfs_asked = vfs.fetched();
    assert_eq!(vfs_asked, 1, "the seeded page was fetched as well");
    assert!(
        vfs.cached_fingerprint(&VfsPath::new("snippets/unused.md"))
            .is_none(),
        "a file nothing asked for reached the browser"
    );
}

#[test]
fn a_preload_set_under_the_cap_previews_in_the_browser() {
    let vfs = seeded(&[("index.md", "# Home\n")]);
    assert!(!vfs.over_budget());
    assert!(vfs.preload_bytes() < PRELOAD_LIMIT);
}

#[test]
fn a_preload_set_over_two_megabytes_moves_the_preview_to_the_server() {
    let big = "x".repeat(PRELOAD_LIMIT as usize + 1);
    let vfs = EditorVfs::sealed([(VfsPath::new("index.md"), bytes(&big))]);
    assert!(vfs.over_budget());
    assert_eq!(vfs.preload_bytes(), PRELOAD_LIMIT + 1);
}
