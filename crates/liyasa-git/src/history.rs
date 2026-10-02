//! Who last changed a path, read from a local repository (VER-77, RFC 1611).
//!
//! `liyasa_verify::drift::review::PageReview::last_author` is VER-77's fallback
//! review owner when `DOCOWNERS` names none, and its doc comment says the field
//! is "Git's, so it is supplied rather than read". This is the supplier.
//!
//! It runs the system `git` rather than linking `gix`, for the reason
//! `liyasa-cli/src/git.rs` records: `gix` is in no crate's manifest and adding
//! it here would be the unilateral dependency move §31.6 forbids. That module
//! carries `TODO(rfc-0900)` — when something reads a local repository with
//! `gix`, it becomes an adapter and the subprocess path goes away. This is that
//! something, minus the `gix`, and it becomes an adapter on the same day.
//!
//! Without `git` on PATH, or outside a work tree, every answer is `None`, as
//! `liyasa_build::git::NoGit` already does for the build clock.

use std::collections::BTreeMap;
use std::path::{Path, PathBuf};
use std::process::Command;
use std::sync::OnceLock;

/// The prefix on an author record, interleaved with `--name-only` output.
///
/// NUL rather than `@`. `liyasa-cli/src/git.rs` uses `@` for its timestamp
/// records, and a path may begin with `@` — `@types/index.md` is an ordinary
/// name — so that parse is ambiguous in principle. A path may not contain NUL.
const RECORD: char = '\0';

/// Who last changed a path. One method, so a caller tests without a repository.
pub trait History: Send + Sync {
    /// The email of whoever last changed `path`, repository-relative, with
    /// `.mailmap` applied.
    fn last_author(&self, path: &str) -> Option<String>;
}

/// No repository, no `git`, or a caller that injected nothing.
#[derive(Debug, Clone, Copy, Default)]
pub struct NoHistory;

impl History for NoHistory {
    fn last_author(&self, _path: &str) -> Option<String> {
        None
    }
}

/// Reads the repository at `root` by running `git`.
#[derive(Debug)]
pub struct SystemHistory {
    root: PathBuf,
    /// Built on first use: one `git log` pass rather than one subprocess per
    /// file, which on a 1,000-page site is 1,000 process spawns.
    authors: OnceLock<BTreeMap<String, String>>,
}

impl SystemHistory {
    pub fn new(root: impl Into<PathBuf>) -> Self {
        Self {
            root: root.into(),
            authors: OnceLock::new(),
        }
    }

    /// Whether this directory is inside a work tree `git` can read.
    pub fn is_repository(root: &Path) -> bool {
        run(root, &["rev-parse", "--is-inside-work-tree"]).is_some_and(|text| text.trim() == "true")
    }

    fn map(&self) -> &BTreeMap<String, String> {
        self.authors.get_or_init(|| {
            // `%aE` applies `.mailmap`: VER-77 emails this address, and a
            // mailmap is the repository's own statement about which address is
            // current, so the raw `%ae` would mail a stale one on purpose.
            //
            // `--no-renames` so a rename reports the path as it is named now,
            // which is the path a caller asks about. `--no-merges` because a
            // merge commit's author is whoever merged — mailing them is worse
            // than mailing nobody, since it looks like an owner.
            run(
                &self.root,
                &[
                    "log",
                    "--pretty=format:%x00%aE",
                    "--name-only",
                    "--no-renames",
                    "--no-merges",
                ],
            )
            .map(|text| authors_of(&text))
            .unwrap_or_default()
        })
    }
}

impl History for SystemHistory {
    fn last_author(&self, path: &str) -> Option<String> {
        self.map().get(path).cloned()
    }
}

/// Parses interleaved author records and path lines into path -> author.
///
/// `git log` is newest first, so the first time a path appears is its last
/// change; later commits touching it are older and must not overwrite.
///
/// A free function over the text so the whole parse is testable without a
/// repository, which is most of what can be wrong here.
pub fn authors_of(text: &str) -> BTreeMap<String, String> {
    let mut out = BTreeMap::new();
    let mut author: Option<&str> = None;
    for line in text.lines() {
        if let Some(email) = line.strip_prefix(RECORD) {
            // An empty `%aE` is a commit with no author email. Treated as no
            // answer rather than as an empty owner, so the fallback stays
            // `DOCOWNERS` instead of becoming a blank address.
            author = (!email.trim().is_empty()).then_some(email.trim());
        } else if !line.trim().is_empty()
            && let Some(email) = author
        {
            out.entry(line.to_owned())
                .or_insert_with(|| email.to_owned());
        }
    }
    out
}

fn run(root: &Path, args: &[&str]) -> Option<String> {
    let output = Command::new("git")
        .current_dir(root)
        .args(args)
        .output()
        .ok()?;
    output
        .status
        .success()
        .then(|| String::from_utf8_lossy(&output.stdout).into_owned())
}

#[cfg(test)]
mod tests {
    use std::sync::atomic::{AtomicU64, Ordering};

    use super::*;

    /// A unique scratch directory, removed when it goes out of scope.
    ///
    /// Not `tempfile`: it is in no crate's manifest and no row of the PRD's
    /// dependency table, and a dev-dependency is still a dependency.
    ///
    /// The counter is beside the pid rather than instead of it, because
    /// `bin/gate` runs nextest — a process per test — and CI runs
    /// `cargo test`, where they are threads sharing ONE pid. A pid-keyed name
    /// is unique locally and collides only on CI.
    struct Scratch(PathBuf);

    static NTH: AtomicU64 = AtomicU64::new(0);

    impl Scratch {
        fn new() -> Self {
            let nth = NTH.fetch_add(1, Ordering::Relaxed);
            let path = std::env::temp_dir()
                .join(format!("liyasa-git-history-{}-{nth}", std::process::id()));
            std::fs::create_dir_all(&path).expect("a scratch directory");
            Self(path)
        }

        fn path(&self) -> &Path {
            &self.0
        }
    }

    impl Drop for Scratch {
        fn drop(&mut self) {
            let _ = std::fs::remove_dir_all(&self.0);
        }
    }

    #[test]
    fn the_newest_commit_touching_a_path_wins() {
        // Newest first, as `git log` emits it.
        let text =
            "\0bob@example.com\nguide.md\nindex.md\n\n\0alice@example.com\nindex.md\nold.md\n";
        let map = authors_of(text);
        assert_eq!(
            map.get("guide.md").map(String::as_str),
            Some("bob@example.com")
        );
        assert_eq!(
            map.get("index.md").map(String::as_str),
            Some("bob@example.com"),
            "index.md is in both commits and the newer author must win"
        );
        assert_eq!(
            map.get("old.md").map(String::as_str),
            Some("alice@example.com")
        );
    }

    #[test]
    fn a_path_beginning_with_an_at_sign_is_a_path() {
        // The ambiguity the `@` prefix would have had: `@types/index.md` is an
        // ordinary name, and under `liyasa-cli/src/git.rs`'s parse its leading
        // `@` would make it a record rather than a file.
        let map = authors_of("\0bob@example.com\n@types/index.md\n");
        assert_eq!(
            map.get("@types/index.md").map(String::as_str),
            Some("bob@example.com")
        );
        assert!(!map.contains_key("types/index.md"));
    }

    #[test]
    fn a_commit_with_no_author_email_supplies_no_owner() {
        // Not an empty-string owner: VER-77 mails this, and a blank address is
        // a worse answer than no answer.
        let map = authors_of("\0\nghost.md\n\n\0alice@example.com\nindex.md\n");
        assert!(!map.contains_key("ghost.md"), "{map:?}");
        assert_eq!(
            map.get("index.md").map(String::as_str),
            Some("alice@example.com")
        );
    }

    #[test]
    fn paths_before_any_author_record_are_dropped() {
        let map = authors_of("stray.md\n\0alice@example.com\nindex.md\n");
        assert!(!map.contains_key("stray.md"));
        assert_eq!(map.len(), 1);
    }

    #[test]
    fn nothing_to_read_answers_none_rather_than_failing() {
        assert_eq!(NoHistory.last_author("index.md"), None);

        let empty = Scratch::new();
        assert!(!SystemHistory::is_repository(empty.path()));
        assert_eq!(
            SystemHistory::new(empty.path()).last_author("index.md"),
            None,
            "outside a work tree every answer is None"
        );
    }

    #[test]
    fn the_last_author_of_a_real_repository() {
        if run(Path::new("."), &["--version"]).is_none() {
            eprintln!("SKIPPED: `git` is not on PATH, so there is no repository to read.");
            return;
        }
        let dir = Scratch::new();
        let root = dir.path();
        let git = |args: &[&str]| {
            assert!(
                run(root, args).is_some(),
                "git {args:?} failed in the fixture"
            );
        };
        git(&["init", "--quiet"]);
        git(&["config", "user.name", "Alice"]);
        git(&["config", "user.email", "alice@example.com"]);
        std::fs::write(root.join("index.md"), "one").expect("a write");
        std::fs::write(root.join("guide.md"), "one").expect("a write");
        git(&["add", "."]);
        git(&["commit", "--quiet", "-m", "first"]);

        git(&["config", "user.email", "bob@example.com"]);
        std::fs::write(root.join("guide.md"), "two").expect("a write");
        git(&["commit", "--quiet", "-am", "second"]);

        let history = SystemHistory::new(root);
        assert!(SystemHistory::is_repository(root));
        assert_eq!(
            history.last_author("guide.md").as_deref(),
            Some("bob@example.com"),
            "the second commit changed guide.md"
        );
        assert_eq!(
            history.last_author("index.md").as_deref(),
            Some("alice@example.com"),
            "index.md was never touched again, so its author is the first commit's"
        );
        assert_eq!(
            history.last_author("absent.md"),
            None,
            "a path with no history has no author rather than a default one"
        );
    }
}
