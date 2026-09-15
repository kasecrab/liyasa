//! The dev server's file watching (PRD §6.6, CLI-02).
//!
//! Events are debounced over a 50 ms window, coalesced per path, filtered, and
//! handed over as one rebuild batch. A storm — a branch switch, a `git stash` —
//! is not processed per event: past `STORM_THRESHOLD` paths the batch says so
//! and the engine re-fingerprints the tree instead.
//!
//! The 100 ms p95 of CLI-02 is measured from the last event of a batch, which
//! is why the debounce window is part of this module and not of the caller.

use std::collections::BTreeSet;
use std::path::{Path, PathBuf};
use std::sync::mpsc::{Receiver, RecvTimeoutError, channel};
use std::time::Duration;

use liyasa_core::vfs::VfsPath;
use liyasa_markdown::source::route::Ignore;
use notify::{RecommendedWatcher, RecursiveMode};
use notify_debouncer_full::{DebounceEventResult, Debouncer, RecommendedCache, new_debouncer};

/// §6.6: "events are debounced (50 ms window)".
pub const DEBOUNCE: Duration = Duration::from_millis(50);

/// §6.6: "a storm of more than 200 changed paths in one window".
pub const STORM_THRESHOLD: usize = 200;

/// Directories a watcher never reports, whatever the ignore file says.
const NEVER: &[&str] = &[".liyasa", ".git", "node_modules", "target"];

/// What a batch of changes asks the engine to do.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Kind {
    /// Content only: the dev server can patch the page body over the socket.
    Content,
    /// Config, navigation, or theme: every page's shell changes, so the client
    /// reloads.
    Structure,
    /// Too many paths to reason about one at a time.
    Storm,
}

/// One debounced, coalesced rebuild transaction.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Batch {
    pub paths: BTreeSet<VfsPath>,
    pub kind: Kind,
}

impl Batch {
    pub fn is_empty(&self) -> bool {
        self.paths.is_empty() && self.kind != Kind::Storm
    }

    /// Whether the client needs a full reload rather than a body patch.
    pub fn needs_reload(&self) -> bool {
        self.kind != Kind::Content
    }
}

/// Turns raw paths into one batch: filters what never matters, drops what the
/// ignore file removed, and decides what the change costs.
pub fn batch(paths: impl IntoIterator<Item = VfsPath>, ignore: &Ignore, output: &str) -> Batch {
    let kept: BTreeSet<VfsPath> = paths
        .into_iter()
        .filter(|path| !is_noise(path, output))
        .filter(|path| !ignore.matches(path))
        .collect();

    let kind = if kept.len() > STORM_THRESHOLD {
        Kind::Storm
    } else if kept.iter().any(is_structural) {
        Kind::Structure
    } else {
        Kind::Content
    };
    Batch { paths: kept, kind }
}

/// `.liyasa/`, `dist/`, `.git/`, and the files an editor writes beside yours.
pub fn is_noise(path: &VfsPath, output: &str) -> bool {
    let text = path.as_str();
    if text.is_empty() {
        return true;
    }
    let first = text.split('/').next().unwrap_or_default();
    if NEVER.contains(&first) || first == output {
        return true;
    }
    let Some(name) = path.file_name() else {
        return true;
    };
    // Vim's `4913`, Emacs' `.#name` and `name~`, JetBrains' `___jb_tmp___`,
    // and every `.swp`: all written next to the file you actually edited.
    name.starts_with(".#")
        || name.ends_with('~')
        || name.ends_with(".swp")
        || name.ends_with(".swx")
        || name.ends_with(".tmp")
        || name.contains("___jb_")
        || name == "4913"
}

/// A change that alters every page rather than one.
fn is_structural(path: &VfsPath) -> bool {
    let text = path.as_str();
    text == "liyasa.json"
        || text.starts_with("liyasa.")
        || text.starts_with("theme/")
        || text.starts_with("snippets/")
        || text.starts_with("components/")
        || text == ".liyasaignore"
        || text == ".liyasa-aiignore"
}

/// A live watcher over a project directory.
///
/// The debouncer is kept alive by this struct; dropping it stops the watch.
pub struct Watch {
    _debouncer: Debouncer<RecommendedWatcher, RecommendedCache>,
    events: Receiver<Vec<PathBuf>>,
    root: PathBuf,
    ignore: Ignore,
    output: String,
}

impl Watch {
    pub fn new(root: &Path, ignore: Ignore, output: &str) -> Result<Self, notify::Error> {
        let (sender, events) = channel();
        let mut debouncer = new_debouncer(DEBOUNCE, None, move |result: DebounceEventResult| {
            let paths = match result {
                Ok(events) => events
                    .into_iter()
                    .flat_map(|event| event.event.paths.clone())
                    .collect(),
                // A watcher error is not a reason to stop watching: the next
                // batch re-fingerprints anyway.
                Err(_) => Vec::new(),
            };
            let _ = sender.send(paths);
        })?;
        debouncer.watch(root, RecursiveMode::Recursive)?;
        Ok(Self {
            _debouncer: debouncer,
            events,
            root: root.to_path_buf(),
            ignore,
            output: output.to_owned(),
        })
    }

    /// The next batch, or `None` when nothing changed inside `timeout`.
    pub fn next_batch(&self, timeout: Duration) -> Option<Batch> {
        let mut paths: Vec<VfsPath> = Vec::new();
        match self.events.recv_timeout(timeout) {
            Ok(first) => paths.extend(first.iter().filter_map(|path| self.relative(path))),
            Err(RecvTimeoutError::Timeout) => return None,
            Err(RecvTimeoutError::Disconnected) => return None,
        }
        // Anything already queued belongs to the same rebuild transaction.
        while let Ok(more) = self.events.try_recv() {
            paths.extend(more.iter().filter_map(|path| self.relative(path)));
        }
        let batch = batch(paths, &self.ignore, &self.output);
        (!batch.is_empty()).then_some(batch)
    }

    fn relative(&self, path: &Path) -> Option<VfsPath> {
        let relative = path.strip_prefix(&self.root).unwrap_or(path);
        Some(VfsPath::new(relative.to_str()?))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn paths(items: &[&str]) -> Vec<VfsPath> {
        items.iter().map(VfsPath::new).collect()
    }

    #[test]
    fn a_content_edit_is_a_content_batch() {
        let batch = batch(paths(&["guides/install.md"]), &Ignore::default(), "dist");
        assert_eq!(batch.kind, Kind::Content);
        assert!(!batch.needs_reload());
        assert_eq!(batch.paths.len(), 1);
    }

    #[test]
    fn config_theme_and_snippets_force_a_reload() {
        for path in [
            "liyasa.json",
            "theme/partials/navbar.html",
            "snippets/note.md",
        ] {
            let batch = batch(paths(&[path]), &Ignore::default(), "dist");
            assert_eq!(batch.kind, Kind::Structure, "{path}");
            assert!(batch.needs_reload());
        }
    }

    #[test]
    fn the_output_and_the_cache_are_never_watched() {
        let batch = batch(
            paths(&[
                "dist/index.html",
                ".liyasa/cache/objects/ab/cd",
                ".git/HEAD",
            ]),
            &Ignore::default(),
            "dist",
        );
        assert!(batch.is_empty());
    }

    #[test]
    fn editor_temporary_files_are_not_edits() {
        for path in [
            "guides/.#install.md",
            "guides/install.md~",
            "guides/.install.md.swp",
            "guides/install.md.tmp",
            "4913",
        ] {
            assert!(is_noise(&VfsPath::new(path), "dist"), "{path}");
        }
        assert!(!is_noise(&VfsPath::new("guides/install.md"), "dist"));
    }

    #[test]
    fn an_ignored_path_never_starts_a_rebuild() {
        let ignore = Ignore::parse("drafts/\n");
        let batch = batch(paths(&["drafts/next.md"]), &ignore, "dist");
        assert!(batch.is_empty());
    }

    #[test]
    fn one_path_touched_three_ways_is_one_entry() {
        let batch = batch(
            paths(&[
                "guides/install.md",
                "guides/install.md",
                "guides/install.md",
            ]),
            &Ignore::default(),
            "dist",
        );
        assert_eq!(batch.paths.len(), 1);
    }

    #[test]
    fn a_branch_switch_is_a_storm() {
        let many: Vec<VfsPath> = (0..STORM_THRESHOLD + 1)
            .map(|at| VfsPath::new(format!("guides/page-{at}.md")))
            .collect();
        let batch = batch(many, &Ignore::default(), "dist");
        assert_eq!(batch.kind, Kind::Storm);
        assert!(batch.needs_reload());
    }

    #[test]
    fn a_live_watch_reports_an_edit() {
        let root = std::env::temp_dir().join(format!("liyasa-watch-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&root);
        std::fs::create_dir_all(root.join("guides")).expect("a project directory");
        std::fs::write(root.join("guides/install.md"), "# install").expect("a page");

        let watch = Watch::new(&root, Ignore::default(), "dist").expect("a watcher");
        std::fs::write(root.join("guides/install.md"), "# install, edited").expect("an edit");

        // The directory may be reported before the file it holds, so batches
        // are collected until the edit shows up.
        let deadline = std::time::Instant::now() + Duration::from_secs(10);
        let mut seen: Vec<String> = Vec::new();
        while std::time::Instant::now() < deadline {
            let Some(batch) = watch.next_batch(Duration::from_millis(500)) else {
                continue;
            };
            seen.extend(batch.paths.iter().map(|path| path.as_str().to_owned()));
            if seen.iter().any(|path| path == "guides/install.md") {
                break;
            }
        }
        assert!(
            seen.iter().any(|path| path == "guides/install.md"),
            "{seen:?}"
        );
        let _ = std::fs::remove_dir_all(&root);
    }
}
