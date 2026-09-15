//! The git seam (PRD §6.6.2 rules 1 and 2; `plan/rfcs/0600-git-is-a-seam.md`).
//!
//! `liyasa-build` needs two facts from a repository — the commit time of `HEAD`
//! and each file's last-modified date — and is not the crate allowed to depend
//! on `gix`. Both arrive through this trait, the way file access arrives
//! through `Vfs`.

use std::collections::BTreeMap;
use std::time::SystemTime;

use liyasa_core::vfs::VfsPath;
use serde::{Deserialize, Serialize};

// TODO(rfc-0600): the `gix`-backed implementation belongs to the CLI and the
// server, which already depend on `liyasa-git`.
pub trait GitMeta: Send + Sync {
    fn head_commit_time(&self) -> Option<SystemTime>;
    fn head_revision(&self) -> Option<String>;
    fn last_modified(&self, path: &VfsPath) -> Option<SystemTime>;
}

/// A project that is not in a repository, or a caller that injected nothing.
#[derive(Debug, Clone, Copy, Default)]
pub struct NoGit;

impl GitMeta for NoGit {
    fn head_commit_time(&self) -> Option<SystemTime> {
        None
    }

    fn head_revision(&self) -> Option<String> {
        None
    }

    fn last_modified(&self, _path: &VfsPath) -> Option<SystemTime> {
        None
    }
}

/// `.liyasa/git-meta.json`: git-derived data frozen once per build so that two
/// builds of the same history agree (§6.6.2 rule 2).
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct GitSnapshot {
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub revision: Option<String>,
    /// Seconds since the Unix epoch, because no date crate is in the tree.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub head_commit_unix: Option<i64>,
    /// Route-independent: keyed by project-relative path, sorted, so the file
    /// is byte-identical for the same history.
    #[serde(default)]
    pub last_modified_unix: BTreeMap<String, i64>,
}

impl GitSnapshot {
    pub fn take(git: &dyn GitMeta, paths: impl IntoIterator<Item = VfsPath>) -> Self {
        let mut last_modified_unix = BTreeMap::new();
        for path in paths {
            if let Some(time) = git.last_modified(&path).and_then(unix) {
                last_modified_unix.insert(path.as_str().to_owned(), time);
            }
        }
        Self {
            revision: git.head_revision(),
            head_commit_unix: git.head_commit_time().and_then(unix),
            last_modified_unix,
        }
    }

    pub fn head_commit_time(&self) -> Option<SystemTime> {
        self.head_commit_unix.map(from_unix)
    }

    pub fn last_modified(&self, path: &VfsPath) -> Option<SystemTime> {
        self.last_modified_unix
            .get(path.as_str())
            .copied()
            .map(from_unix)
    }
}

fn unix(time: SystemTime) -> Option<i64> {
    match time.duration_since(SystemTime::UNIX_EPOCH) {
        Ok(since) => i64::try_from(since.as_secs()).ok(),
        Err(before) => i64::try_from(before.duration().as_secs())
            .ok()
            .map(|secs| -secs),
    }
}

fn from_unix(seconds: i64) -> SystemTime {
    let magnitude = std::time::Duration::from_secs(seconds.unsigned_abs());
    if seconds < 0 {
        SystemTime::UNIX_EPOCH - magnitude
    } else {
        SystemTime::UNIX_EPOCH + magnitude
    }
}

#[cfg(test)]
mod tests {
    use std::time::Duration;

    use super::*;

    struct Fake;

    impl GitMeta for Fake {
        fn head_commit_time(&self) -> Option<SystemTime> {
            Some(SystemTime::UNIX_EPOCH + Duration::from_secs(1_700_000_000))
        }

        fn head_revision(&self) -> Option<String> {
            Some("c0ffee".to_owned())
        }

        fn last_modified(&self, path: &VfsPath) -> Option<SystemTime> {
            (path.as_str() == "guides/install.md")
                .then(|| SystemTime::UNIX_EPOCH + Duration::from_secs(1_600_000_000))
        }
    }

    #[test]
    fn a_snapshot_records_only_what_git_knows() {
        let paths = [VfsPath::new("guides/install.md"), VfsPath::new("new.md")];
        let snapshot = GitSnapshot::take(&Fake, paths);
        assert_eq!(snapshot.revision.as_deref(), Some("c0ffee"));
        assert_eq!(snapshot.head_commit_unix, Some(1_700_000_000));
        assert_eq!(snapshot.last_modified_unix.len(), 1);
        assert_eq!(
            snapshot.last_modified(&VfsPath::new("guides/install.md")),
            Some(SystemTime::UNIX_EPOCH + Duration::from_secs(1_600_000_000))
        );
        assert_eq!(snapshot.last_modified(&VfsPath::new("new.md")), None);
    }

    #[test]
    fn no_git_knows_nothing() {
        let snapshot = GitSnapshot::take(&NoGit, [VfsPath::new("index.md")]);
        assert_eq!(snapshot, GitSnapshot::default());
        assert!(snapshot.head_commit_time().is_none());
    }

    #[test]
    fn a_snapshot_round_trips_as_json() {
        let snapshot = GitSnapshot::take(&Fake, [VfsPath::new("guides/install.md")]);
        let text = serde_json::to_string(&snapshot).expect("a snapshot serializes");
        let back: GitSnapshot = serde_json::from_str(&text).expect("a snapshot deserializes");
        assert_eq!(back, snapshot);
    }
}
