//! What an import would write, and writing it (MIG-06).
//!
//! An importer's only product is a [`Plan`]: it reads the source project and
//! decides every file, but touches nothing. A dry run is therefore the default
//! rather than a mode — the caller prints [`Plan::summary`] and stops, or calls
//! [`Plan::apply`] to put the files on disk.

use std::path::{Path, PathBuf};

use liyasa_core::diagnostics::{Diagnostic, Diagnostics, code};
use liyasa_core::vfs::{Vfs, VfsPath};

use crate::report::Report;

/// Where a planned file's bytes come from.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Content {
    /// Text the importer produced.
    Text(String),
    /// Bytes carried over unchanged from the source project (MIG-05).
    Copy(VfsPath),
}

impl Content {
    pub fn as_text(&self) -> Option<&str> {
        match self {
            Self::Text(text) => Some(text),
            Self::Copy(_) => None,
        }
    }
}

/// One file the import would write.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct FileWrite {
    /// Relative to the destination root.
    pub path: VfsPath,
    pub content: Content,
}

/// Everything one import would write, and why.
#[derive(Debug, Clone)]
pub struct Plan {
    /// Sorted by path, so a dry run reads the same way twice.
    writes: Vec<FileWrite>,
    pub report: Report,
}

/// How [`Plan::apply`] treats a destination that is not empty.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct Apply {
    /// Overwrite a file that already exists instead of refusing.
    pub force: bool,
}

impl Plan {
    pub fn new(report: Report) -> Self {
        Self {
            writes: Vec::new(),
            report,
        }
    }

    /// Plans one file. Later content at a path the plan already holds replaces
    /// the earlier one, which is how an importer's own default (a stub page, a
    /// generated index) gives way to the author's real file.
    pub fn write(&mut self, path: VfsPath, content: Content) -> &mut Self {
        match self
            .writes
            .binary_search_by(|planned| planned.path.cmp(&path))
        {
            Ok(at) => self.writes[at].content = content,
            Err(at) => self.writes.insert(at, FileWrite { path, content }),
        }
        self
    }

    pub fn text(&mut self, path: impl AsRef<str>, text: impl Into<String>) -> &mut Self {
        self.write(VfsPath::new(path), Content::Text(text.into()))
    }

    /// Carries a source file over unchanged and records it in the report.
    pub fn carry(&mut self, from: VfsPath, to: VfsPath) -> &mut Self {
        self.report.carried.push(to.clone());
        self.write(to, Content::Copy(from))
    }

    pub fn writes(&self) -> &[FileWrite] {
        &self.writes
    }

    pub fn get(&self, path: impl AsRef<str>) -> Option<&Content> {
        let path = VfsPath::new(path);
        self.writes
            .binary_search_by(|planned| planned.path.cmp(&path))
            .ok()
            .map(|at| &self.writes[at].content)
    }

    /// The text planned for a path, for a caller that is about to assert on it.
    pub fn text_at(&self, path: impl AsRef<str>) -> Option<&str> {
        self.get(path)?.as_text()
    }

    pub fn is_empty(&self) -> bool {
        self.writes.is_empty()
    }

    /// What `--dry-run` prints: every file, and nothing on disk.
    pub fn summary(&self) -> String {
        let mut out = String::new();
        for planned in &self.writes {
            let note = match &planned.content {
                Content::Text(text) => format!("{} bytes", text.len()),
                Content::Copy(from) => format!("copy of {from}"),
            };
            out.push_str(&format!("write {} ({note})\n", planned.path));
        }
        out.push_str(&format!("{} files, nothing written\n", self.writes.len()));
        out
    }

    /// Writes the plan under `root`, reading carried bytes from `source`.
    ///
    /// Nothing is written when anything would be refused, so a failed import
    /// does not leave half a project behind.
    pub fn apply(&self, source: &dyn Vfs, root: &Path, options: &Apply) -> Diagnostics {
        let mut diagnostics = Diagnostics::new();
        let mut staged: Vec<(PathBuf, Vec<u8>)> = Vec::with_capacity(self.writes.len());

        for planned in &self.writes {
            let target = root.join(planned.path.as_str());
            if !options.force && target.exists() {
                diagnostics.push(
                    Diagnostic::new(code::E1103, format!("`{}` already exists", planned.path))
                        .help("import into an empty directory, or apply the plan with `Apply { force: true }`"),
                );
                continue;
            }
            match &planned.content {
                Content::Text(text) => staged.push((target, text.as_bytes().to_vec())),
                Content::Copy(from) => match source.read(from) {
                    Ok(bytes) => staged.push((target, bytes.to_vec())),
                    Err(error) => diagnostics.push(Diagnostic::new(
                        code::E1102,
                        format!("cannot read `{from}`: {error}"),
                    )),
                },
            }
        }

        if diagnostics.has_errors() {
            return diagnostics;
        }

        for (target, bytes) in staged {
            if let Some(parent) = target.parent()
                && let Err(error) = std::fs::create_dir_all(parent)
            {
                diagnostics.push(Diagnostic::new(
                    code::E1103,
                    format!("cannot create `{}`: {error}", parent.display()),
                ));
                continue;
            }
            if let Err(error) = std::fs::write(&target, &bytes) {
                diagnostics.push(Diagnostic::new(
                    code::E1103,
                    format!("cannot write `{}`: {error}", target.display()),
                ));
            }
        }
        diagnostics
    }
}

#[cfg(test)]
mod tests {
    use liyasa_config::vfs::MemVfs;

    use super::*;
    use crate::report::{Report, Source};

    fn plan() -> Plan {
        Plan::new(Report::new(Source::Mintlify))
    }

    #[test]
    fn files_are_planned_in_path_order_whatever_order_they_arrive_in() {
        let mut plan = plan();
        plan.text("docs/install.md", "b");
        plan.text("liyasa.json", "c");
        plan.text("docs/index.md", "a");

        let paths: Vec<&str> = plan.writes().iter().map(|w| w.path.as_str()).collect();
        assert_eq!(paths, ["docs/index.md", "docs/install.md", "liyasa.json"]);
    }

    #[test]
    fn writing_a_path_twice_keeps_the_later_content() {
        let mut plan = plan();
        plan.text("liyasa.json", "first");
        plan.text("liyasa.json", "second");
        assert_eq!(plan.writes().len(), 1);
        assert_eq!(plan.text_at("liyasa.json"), Some("second"));
    }

    #[test]
    fn a_summary_lists_every_file_and_says_nothing_was_written() {
        let mut plan = plan();
        plan.text("liyasa.json", "{}");
        plan.carry(
            VfsPath::new("images/logo.svg"),
            VfsPath::new("assets/logo.svg"),
        );

        let summary = plan.summary();
        assert!(summary.contains("write liyasa.json (2 bytes)"));
        assert!(summary.contains("write assets/logo.svg (copy of images/logo.svg)"));
        assert!(summary.contains("2 files, nothing written"));
    }

    #[test]
    fn a_carried_file_is_recorded_in_the_report() {
        let mut plan = plan();
        plan.carry(
            VfsPath::new("images/logo.svg"),
            VfsPath::new("assets/logo.svg"),
        );
        assert_eq!(plan.report.carried.len(), 1);
    }

    #[test]
    fn applying_writes_text_and_copies_bytes() {
        let root = tempdir();
        let source = MemVfs::new().with("images/logo.svg", b"<svg/>".to_vec());
        let mut plan = plan();
        plan.text("liyasa.json", "{\"name\":\"Docs\"}");
        plan.carry(
            VfsPath::new("images/logo.svg"),
            VfsPath::new("assets/logo.svg"),
        );

        let diagnostics = plan.apply(&source, &root, &Apply::default());
        assert!(diagnostics.is_empty(), "{:?}", diagnostics.as_slice());
        assert_eq!(
            std::fs::read_to_string(root.join("liyasa.json")).unwrap_or_default(),
            "{\"name\":\"Docs\"}"
        );
        assert_eq!(
            std::fs::read_to_string(root.join("assets/logo.svg")).unwrap_or_default(),
            "<svg/>"
        );
    }

    #[test]
    fn an_existing_file_is_refused_and_nothing_at_all_is_written() {
        let root = tempdir();
        std::fs::write(root.join("liyasa.json"), "mine").expect("the fixture writes");
        let mut plan = plan();
        plan.text("liyasa.json", "theirs");
        plan.text("docs/index.md", "page");

        let diagnostics = plan.apply(&MemVfs::new(), &root, &Apply::default());
        assert_eq!(
            diagnostics
                .iter()
                .map(|d| d.code.as_str())
                .collect::<Vec<_>>(),
            ["E1103"]
        );
        assert_eq!(
            std::fs::read_to_string(root.join("liyasa.json")).unwrap_or_default(),
            "mine"
        );
        assert!(
            !root.join("docs/index.md").exists(),
            "a refused import wrote a page"
        );
    }

    #[test]
    fn force_overwrites() {
        let root = tempdir();
        std::fs::write(root.join("liyasa.json"), "mine").expect("the fixture writes");
        let mut plan = plan();
        plan.text("liyasa.json", "theirs");

        let diagnostics = plan.apply(&MemVfs::new(), &root, &Apply { force: true });
        assert!(diagnostics.is_empty());
        assert_eq!(
            std::fs::read_to_string(root.join("liyasa.json")).unwrap_or_default(),
            "theirs"
        );
    }

    #[test]
    fn a_carried_file_that_vanished_is_reported_rather_than_panicking() {
        let root = tempdir();
        let mut plan = plan();
        plan.carry(
            VfsPath::new("images/gone.svg"),
            VfsPath::new("assets/gone.svg"),
        );

        let diagnostics = plan.apply(&MemVfs::new(), &root, &Apply::default());
        assert_eq!(
            diagnostics
                .iter()
                .map(|d| d.code.as_str())
                .collect::<Vec<_>>(),
            ["E1102"]
        );
    }

    /// A directory of our own under the test binary's temp root. `tempfile` is
    /// not in the dependency table, and one counter is enough here.
    fn tempdir() -> PathBuf {
        use std::sync::atomic::{AtomicU32, Ordering};
        static NEXT: AtomicU32 = AtomicU32::new(0);
        let at = NEXT.fetch_add(1, Ordering::Relaxed);
        let path = std::env::temp_dir().join(format!("liyasa-import-{}-{at}", std::process::id()));
        let _ = std::fs::remove_dir_all(&path);
        std::fs::create_dir_all(&path).expect("a temp directory");
        path
    }
}
