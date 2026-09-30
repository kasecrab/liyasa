//! The typed diff the output gate reads (AGT-06).
//!
//! A gate that trusts a caller's line counts is not a gate: a run that reports
//! `lines_added: 0` while adding five hundred lines would walk past
//! `maxLinesChanged`. So [`FileChange`] carries the text on each side and
//! [`FileChange::lines`] computes the counts, and there is no constructor that
//! takes them.
//!
//! The count is git's: `before.len() - lcs` removed and `after.len() - lcs`
//! added, over a longest common subsequence of lines. Above
//! [`LCS_CELL_BUDGET`] cells the subsequence is not computed and the counts fall
//! back to the whole of each side after its common prefix and suffix are trimmed
//! — an over-count, never an under-count, because a gate that guesses low on a
//! file too large to diff is a gate with a size key.
//!
//! [`Novelty`] is the other half. The gate scans a page's text *after* the
//! change, not the added lines alone, because reconstructing added lines needs a
//! traceback this does not keep and "only scan what changed" is a hole for
//! anything the run moved rather than typed. What keeps that from reporting the
//! site's existing content as the run's doing is asking whether the same finding
//! is on the before side: [`Novelty::PreExisting`] is a flag for the reviewer,
//! [`Novelty::New`] is the run's.

use crate::scope::{Layout, Target};

/// How many LCS cells are computed before the counts fall back to a bound.
///
/// 4M cells of `u32` is 16 MB of scratch in two rows — far less in practice,
/// since only two rows are held — and covers a 2000-line page against a
/// 2000-line rewrite.
pub const LCS_CELL_BUDGET: usize = 4_000_000;

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ChangeKind {
    Added,
    Modified,
    Deleted,
    Renamed { from: String },
}

/// One file's change, with the text on each side.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct FileChange {
    pub path: String,
    pub kind: ChangeKind,
    /// `None` for an addition.
    pub before: Option<String>,
    /// `None` for a deletion.
    pub after: Option<String>,
}

/// How many lines a change adds and removes.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct LineCount {
    pub added: u32,
    pub removed: u32,
    /// Whether the counts are an upper bound rather than a minimal diff.
    pub bounded: bool,
}

impl LineCount {
    pub const fn total(self) -> u32 {
        self.added.saturating_add(self.removed)
    }
}

impl FileChange {
    pub fn added(path: impl Into<String>, after: impl Into<String>) -> Self {
        Self {
            path: path.into(),
            kind: ChangeKind::Added,
            before: None,
            after: Some(after.into()),
        }
    }

    pub fn modified(
        path: impl Into<String>,
        before: impl Into<String>,
        after: impl Into<String>,
    ) -> Self {
        Self {
            path: path.into(),
            kind: ChangeKind::Modified,
            before: Some(before.into()),
            after: Some(after.into()),
        }
    }

    pub fn deleted(path: impl Into<String>, before: impl Into<String>) -> Self {
        Self {
            path: path.into(),
            kind: ChangeKind::Deleted,
            before: Some(before.into()),
            after: None,
        }
    }

    pub fn renamed(
        from: impl Into<String>,
        to: impl Into<String>,
        before: impl Into<String>,
        after: impl Into<String>,
    ) -> Self {
        Self {
            path: to.into(),
            kind: ChangeKind::Renamed { from: from.into() },
            before: Some(before.into()),
            after: Some(after.into()),
        }
    }

    /// The lines added and removed, computed from the text.
    pub fn lines(&self) -> LineCount {
        let before: Vec<&str> = self.before.as_deref().map_or(Vec::new(), lines_of);
        let after: Vec<&str> = self.after.as_deref().map_or(Vec::new(), lines_of);
        count(&before, &after)
    }

    /// Whether a finding present in the text after the change was already there
    /// before it.
    pub fn novelty(&self, present_before: bool) -> Novelty {
        if present_before {
            Novelty::PreExisting
        } else {
            Novelty::New
        }
    }
}

/// Whether the run introduced a finding or inherited it.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Novelty {
    New,
    PreExisting,
}

fn lines_of(text: &str) -> Vec<&str> {
    text.lines().collect()
}

/// git's added and removed counts, or an upper bound on them.
fn count(before: &[&str], after: &[&str]) -> LineCount {
    let head = before
        .iter()
        .zip(after.iter())
        .take_while(|(a, b)| a == b)
        .count();
    let tail = before[head..]
        .iter()
        .rev()
        .zip(after[head..].iter().rev())
        .take_while(|(a, b)| a == b)
        .count();
    let before = &before[head..before.len() - tail];
    let after = &after[head..after.len() - tail];

    if before.len().saturating_mul(after.len()) > LCS_CELL_BUDGET {
        return LineCount {
            added: after.len() as u32,
            removed: before.len() as u32,
            bounded: true,
        };
    }
    let common = lcs_len(before, after);
    LineCount {
        added: (after.len() - common) as u32,
        removed: (before.len() - common) as u32,
        bounded: false,
    }
}

/// Length of the longest common subsequence, two rows at a time.
fn lcs_len(a: &[&str], b: &[&str]) -> usize {
    if a.is_empty() || b.is_empty() {
        return 0;
    }
    let mut previous = vec![0u32; b.len() + 1];
    let mut current = vec![0u32; b.len() + 1];
    for line in a {
        for (j, other) in b.iter().enumerate() {
            current[j + 1] = if line == other {
                previous[j] + 1
            } else {
                current[j].max(previous[j + 1])
            };
        }
        std::mem::swap(&mut previous, &mut current);
        current.fill(0);
    }
    previous[b.len()] as usize
}

/// The whole change set one run produced.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct Diff {
    pub files: Vec<FileChange>,
}

impl Diff {
    pub fn new(files: impl IntoIterator<Item = FileChange>) -> Self {
        Self {
            files: files.into_iter().collect(),
        }
    }

    /// Files touched. A rename counts once, at its new path.
    pub fn files_changed(&self) -> u32 {
        self.files.len() as u32
    }

    /// Lines added plus lines removed, across every file.
    pub fn lines_changed(&self) -> u32 {
        self.files
            .iter()
            .map(|f| f.lines().total())
            .fold(0u32, u32::saturating_add)
    }

    /// Whether any file's counts are a bound rather than a diff.
    pub fn is_bounded(&self) -> bool {
        self.files.iter().any(|f| f.lines().bounded)
    }

    /// The pages this diff deletes.
    ///
    /// A rename is not a deletion: the page keeps its identity and its route
    /// moves, which is `move_page`, not a delete. Counting it as one would make
    /// AGT-04's bulk-delete cap fire on a reorganisation.
    pub fn deleted_pages(&self, layout: &Layout) -> Vec<String> {
        self.files
            .iter()
            .filter(|f| f.kind == ChangeKind::Deleted)
            .filter(|f| matches!(layout.classify(&f.path), Target::Page(_)))
            .map(|f| f.path.clone())
            .collect()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn an_addition_counts_every_line_as_added() {
        let change = FileChange::added("a.md", "one\ntwo\nthree\n");
        assert_eq!(
            change.lines(),
            LineCount {
                added: 3,
                removed: 0,
                bounded: false
            }
        );
    }

    #[test]
    fn a_deletion_counts_every_line_as_removed() {
        let change = FileChange::deleted("a.md", "one\ntwo\n");
        assert_eq!(
            change.lines(),
            LineCount {
                added: 0,
                removed: 2,
                bounded: false
            }
        );
    }

    #[test]
    fn a_one_line_edit_in_a_long_file_counts_one_each_way() {
        // The property that matters: the count is git's, not the span between the
        // first and last change.
        let before: String = (0..200).map(|n| format!("line {n}\n")).collect();
        let after = before.replace("line 100\n", "line one hundred\n");
        let change = FileChange::modified("a.md", &before, &after);
        assert_eq!(
            change.lines(),
            LineCount {
                added: 1,
                removed: 1,
                bounded: false
            }
        );
    }

    #[test]
    fn two_scattered_edits_count_two_each_way() {
        let before: String = (0..50).map(|n| format!("line {n}\n")).collect();
        let after = before
            .replace("line 5\n", "five\n")
            .replace("line 40\n", "forty\n");
        assert_eq!(
            FileChange::modified("a.md", &before, &after)
                .lines()
                .total(),
            4
        );
    }

    #[test]
    fn an_insertion_is_added_and_not_removed() {
        let change = FileChange::modified("a.md", "one\ntwo\n", "one\nnew\ntwo\n");
        assert_eq!(
            change.lines(),
            LineCount {
                added: 1,
                removed: 0,
                bounded: false
            }
        );
    }

    #[test]
    fn an_unchanged_file_counts_nothing() {
        let change = FileChange::modified("a.md", "one\ntwo\n", "one\ntwo\n");
        assert_eq!(change.lines(), LineCount::default());
    }

    #[test]
    fn a_caller_cannot_understate_the_size_of_a_change() {
        // There is no field to set. The only way to report a small change is to
        // make one.
        let change = FileChange::modified("a.md", "", "x\n".repeat(500));
        assert_eq!(change.lines().added, 500);
    }

    #[test]
    fn a_file_too_large_to_diff_is_counted_high_and_says_so() {
        // Over-counting is the safe direction: the alternative is a size key.
        let before: String = (0..3000).map(|n| format!("a {n}\n")).collect();
        let after: String = (0..3000).map(|n| format!("b {n}\n")).collect();
        let count = FileChange::modified("a.md", &before, &after).lines();
        assert!(count.bounded, "the fallback did not trigger");
        assert_eq!(count.added, 3000);
        assert_eq!(count.removed, 3000);
    }

    #[test]
    fn a_diff_sums_its_files() {
        let diff = Diff::new([
            FileChange::added("a.md", "one\n"),
            FileChange::deleted("b.md", "two\nthree\n"),
        ]);
        assert_eq!(diff.files_changed(), 2);
        assert_eq!(diff.lines_changed(), 3);
    }

    #[test]
    fn only_deleted_pages_count_as_deleted_pages() {
        let layout = Layout::default();
        let diff = Diff::new([
            FileChange::deleted("guides/old.md", "x\n"),
            FileChange::deleted("assets/logo.svg", "x\n"),
            FileChange::added("guides/new.md", "x\n"),
        ]);
        assert_eq!(diff.deleted_pages(&layout), vec!["guides/old.md"]);
    }

    #[test]
    fn a_rename_is_not_a_deletion() {
        // Otherwise AGT-04's bulk-delete cap fires on a reorganisation, which is
        // the one large change a writing agent legitimately makes.
        let layout = Layout::default();
        let diff = Diff::new([FileChange::renamed(
            "guides/old.md",
            "guides/new.md",
            "x\n",
            "x\n",
        )]);
        assert!(diff.deleted_pages(&layout).is_empty());
        assert_eq!(diff.files_changed(), 1);
    }

    #[test]
    fn novelty_says_whether_the_run_introduced_a_finding() {
        let change = FileChange::modified("a.md", "before\n", "after\n");
        assert_eq!(change.novelty(true), Novelty::PreExisting);
        assert_eq!(change.novelty(false), Novelty::New);
    }
}
