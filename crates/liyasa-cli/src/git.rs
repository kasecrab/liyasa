//! The `GitMeta` implementation RFC 0600 says the CLI injects.
//!
//! It runs the system `git` rather than linking `gix`, because §6.2 puts `gix`
//! in `liyasa-git` and that crate does not exist; adding it here instead would
//! be the unilateral dependency move §31.6 forbids. TODO(rfc-0900): when
//! `liyasa-git` lands this module becomes an adapter over it and the subprocess
//! path goes away.
//!
//! Without git on PATH, or outside a repository, every answer is `None` and the
//! build clock falls back to its own last rule, exactly as
//! [`NoGit`](liyasa_build::git::NoGit) does.

use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::process::Command;
use std::sync::OnceLock;
use std::time::{Duration, SystemTime};

use liyasa_build::git::GitMeta;
use liyasa_core::vfs::VfsPath;

pub struct SystemGit {
    root: PathBuf,
    /// Built on first use: one `git log` pass rather than one subprocess per
    /// file, which on a 1,000-page site is 1,000 process spawns inside the
    /// build's hot path.
    modified: OnceLock<HashMap<String, i64>>,
    head: OnceLock<Option<(String, i64)>>,
}

impl SystemGit {
    pub fn new(root: impl Into<PathBuf>) -> Self {
        Self {
            root: root.into(),
            modified: OnceLock::new(),
            head: OnceLock::new(),
        }
    }

    /// Whether this directory is inside a work tree git can read.
    pub fn is_repository(root: &Path) -> bool {
        run(root, &["rev-parse", "--is-inside-work-tree"]).is_some_and(|text| text.trim() == "true")
    }

    /// Whether the `git` program is on PATH at all, which is what
    /// `liyasa doctor` reports.
    pub fn is_available() -> bool {
        Command::new("git")
            .arg("--version")
            .stdout(std::process::Stdio::null())
            .stderr(std::process::Stdio::null())
            .status()
            .is_ok_and(|status| status.success())
    }

    fn head_pair(&self) -> &Option<(String, i64)> {
        self.head.get_or_init(|| {
            let text = run(&self.root, &["log", "-1", "--pretty=format:%H %ct"])?;
            let mut parts = text.split_whitespace();
            let revision = parts.next()?.to_owned();
            let seconds = parts.next()?.parse().ok()?;
            Some((revision, seconds))
        })
    }

    fn modified_map(&self) -> &HashMap<String, i64> {
        self.modified.get_or_init(|| {
            let mut map = HashMap::new();
            // Newest commit first, so the first time a path appears is its last
            // modification. `--no-renames` keeps a rename from reporting the
            // old path, which is not a path the build will ask about.
            let Some(text) = run(
                &self.root,
                &[
                    "log",
                    "--pretty=format:@%ct",
                    "--name-only",
                    "--no-renames",
                    "--no-merges",
                ],
            ) else {
                return map;
            };
            let mut at = 0_i64;
            for line in text.lines() {
                if let Some(seconds) = line.strip_prefix('@') {
                    at = seconds.trim().parse().unwrap_or(at);
                } else if !line.trim().is_empty() {
                    map.entry(line.to_owned()).or_insert(at);
                }
            }
            map
        })
    }
}

impl GitMeta for SystemGit {
    fn head_commit_time(&self) -> Option<SystemTime> {
        self.head_pair()
            .as_ref()
            .and_then(|(_, seconds)| unix(*seconds))
    }

    fn head_revision(&self) -> Option<String> {
        self.head_pair()
            .as_ref()
            .map(|(revision, _)| revision.clone())
    }

    fn last_modified(&self, path: &VfsPath) -> Option<SystemTime> {
        self.modified_map()
            .get(path.as_str())
            .copied()
            .and_then(unix)
    }
}

fn unix(seconds: i64) -> Option<SystemTime> {
    u64::try_from(seconds)
        .ok()
        .map(|seconds| SystemTime::UNIX_EPOCH + Duration::from_secs(seconds))
}

fn run(root: &Path, arguments: &[&str]) -> Option<String> {
    let output = Command::new("git")
        .arg("-C")
        .arg(root)
        .args(arguments)
        .stderr(std::process::Stdio::null())
        .output()
        .ok()?;
    if !output.status.success() {
        return None;
    }
    String::from_utf8(output.stdout).ok()
}

/// `git init` for `liyasa new` (CLI-01). Reports whether it ran, so the
/// scaffold can say so rather than claiming a repository that is not there.
pub fn init(root: &Path) -> bool {
    Command::new("git")
        .arg("init")
        .arg("--quiet")
        .arg(root)
        .stdout(std::process::Stdio::null())
        .stderr(std::process::Stdio::null())
        .status()
        .is_ok_and(|status| status.success())
}
