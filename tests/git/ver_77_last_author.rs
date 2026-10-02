//! VER-77's fallback review owner, driven across both crates (RFC 1611).
//!
//! `liyasa-git` answers who last changed a path and `liyasa-verify` decides who
//! owns the review. Each half has its own tests; neither proves the join, and
//! the join is the clause: "every page has a review owner (from `DOCOWNERS`,
//! falling back to the last author)".
//!
//! So this drives a real repository through `SystemHistory` into `PageReview`
//! and reads the owner off the candidate `overdue` produces. A test that only
//! asserted both halves exist would be a claim about structure.

use std::path::{Path, PathBuf};
use std::process::Command;
use std::sync::atomic::{AtomicU64, Ordering};
use std::time::SystemTime;

use liyasa_core::ids::Route;
use liyasa_git::history::{History, SystemHistory};
use liyasa_verify::drift::owners::Docowners;
use liyasa_verify::drift::record::DriftKind;
use liyasa_verify::drift::review::{Cadence, PageReview, overdue};

static NTH: AtomicU64 = AtomicU64::new(0);

/// A scratch repository, removed when it goes out of scope.
///
/// The counter is beside the pid, not instead of it: `bin/gate` runs nextest,
/// a process per test, and CI runs `cargo test`, where tests are threads
/// sharing one pid. A pid-only name is unique locally and collides only there.
struct Repo(PathBuf);

impl Repo {
    fn path(&self) -> &Path {
        &self.0
    }

    fn git(&self, args: &[&str]) {
        let status = Command::new("git")
            .current_dir(&self.0)
            .args(args)
            .stdout(std::process::Stdio::null())
            .stderr(std::process::Stdio::null())
            .status()
            .expect("git runs");
        assert!(status.success(), "git {args:?} failed");
    }

    fn write(&self, name: &str, body: &str) {
        std::fs::write(self.0.join(name), body).expect("a write");
    }
}

impl Drop for Repo {
    fn drop(&mut self) {
        let _ = std::fs::remove_dir_all(&self.0);
    }
}

/// `None` when there is no `git` to drive, so the caller skips with a reason
/// rather than failing on a machine without it.
fn repository() -> Option<Repo> {
    Command::new("git").arg("--version").output().ok()?;
    let n = NTH.fetch_add(1, Ordering::Relaxed);
    let root = std::env::temp_dir().join(format!("liyasa-ver77-{}-{n}", std::process::id()));
    std::fs::create_dir_all(&root).expect("a scratch directory");
    let repo = Repo(root);
    repo.git(&["init", "--quiet"]);
    repo.git(&["config", "user.name", "Alice"]);
    repo.git(&["config", "user.email", "alice@example.com"]);
    repo.write("index.md", "one");
    repo.write("guide.md", "one");
    repo.git(&["add", "."]);
    repo.git(&["commit", "--quiet", "-m", "first"]);
    repo.git(&["config", "user.email", "bob@example.com"]);
    repo.write("guide.md", "two");
    repo.git(&["commit", "--quiet", "-am", "second"]);
    Some(repo)
}

#[test]
fn an_overdue_page_nobody_owns_is_owned_by_whoever_last_changed_it() {
    let Some(repo) = repository() else {
        eprintln!("SKIPPED: `git` is not on PATH, so there is no repository to read.");
        return;
    };
    let history = SystemHistory::new(repo.path());

    // `DOCOWNERS` names an owner for /api and nothing else, so /guide falls
    // through to the author and /api/pets must not.
    let owners = Docowners::parse("/api/**  api@example.com\n");
    let pages = [
        PageReview {
            reviewed: Some("2020-01-01".to_owned()),
            last_author: history.last_author("guide.md"),
            ..PageReview::new(Route::new("/guide"))
        },
        PageReview {
            reviewed: Some("2020-01-01".to_owned()),
            last_author: history.last_author("index.md"),
            ..PageReview::new(Route::new("/api/pets"))
        },
    ];

    // The supplier half, asserted before the join so a `None` here cannot be
    // mistaken for the fallback rule declining to fire.
    assert_eq!(
        pages[0].last_author.as_deref(),
        Some("bob@example.com"),
        "the second commit changed guide.md, so bob is its last author"
    );

    let found = overdue(&pages, &owners, &Cadence::default(), SystemTime::now());
    let named: Vec<Vec<String>> = found
        .candidates
        .iter()
        .map(|candidate| match &candidate.kind {
            DriftKind::Review { owners, .. } => owners.clone(),
            other => panic!("every candidate here is a review: {other:?}"),
        })
        .collect();

    assert_eq!(named.len(), 2, "both pages are past the 180-day default");
    assert_eq!(
        named[0],
        vec!["bob@example.com".to_owned()],
        "no DOCOWNERS rule matches /guide, so the reminder goes to its last author"
    );
    assert_eq!(
        named[1],
        vec!["api@example.com".to_owned()],
        "a matching rule wins; alice last changed index.md and must not be mailed"
    );
}

#[test]
fn without_a_repository_the_owner_is_docowners_or_nobody() {
    // The `NoGit` shape of VER-77: no history to read is not an empty owner.
    let pages = [PageReview {
        reviewed: Some("2020-01-01".to_owned()),
        last_author: None,
        ..PageReview::new(Route::new("/guide"))
    }];
    let found = overdue(
        &pages,
        &Docowners::parse(""),
        &Cadence::default(),
        SystemTime::now(),
    );
    match &found.candidates[0].kind {
        DriftKind::Review { owners, .. } => assert!(
            owners.is_empty(),
            "an unsupplied author is no owner, not a blank one: {owners:?}"
        ),
        other => panic!("{other:?}"),
    }
}
