//! Reading the conformance corpus (PRD §30.9).
//!
//! The corpus is the specification as examples, and it lives outside the
//! repository, so these tests assert when it is there and say so when it is
//! not — they never quietly pass because a directory is missing.
//!
//! `xtask conformance` runs a corpus case against a Markdown engine, which
//! produces no verification diagnostics, so every `ver-*` case is reported
//! skipped there. The checks in this crate that read the page **source**
//! rather than a parsed AST can run against those cases directly, and that is
//! what this module does. The cases that need an AST stay for whoever wires a
//! verify engine into the runner (`NEEDS-INPUT.md`).

use std::path::{Path, PathBuf};

use serde::Deserialize;

/// What a case asserts about a diagnostic. The same shape `xtask` reads; only
/// the fields a case sets are compared.
#[derive(Debug, Clone, PartialEq, Eq, Deserialize)]
pub struct ExpectedDiagnostic {
    pub code: String,
    #[serde(default)]
    pub message: Option<String>,
}

#[derive(Debug, Clone, Default, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct CaseHeader {
    pub id: String,
    #[serde(default)]
    pub requirement: Option<String>,
    #[serde(default)]
    pub tags: Vec<String>,
    #[serde(default)]
    pub pending: Option<String>,
}

#[derive(Debug, Clone)]
pub struct Case {
    pub path: PathBuf,
    pub header: CaseHeader,
    pub source: String,
    pub diagnostics: Option<Vec<ExpectedDiagnostic>>,
}

impl Case {
    /// The front matter, as JSON, and the body below it.
    pub fn split(&self) -> (serde_json::Value, &str) {
        let Some(rest) = self.source.strip_prefix("---\n") else {
            return (serde_json::Value::Null, &self.source);
        };
        match rest.split_once("\n---\n") {
            Some((yaml, body)) => (
                liyasa_core::yaml::parse_value(yaml, None).unwrap_or(serde_json::Value::Null),
                body,
            ),
            None => (serde_json::Value::Null, &self.source),
        }
    }

    pub fn expects(&self, code: &str) -> bool {
        self.diagnostics
            .as_deref()
            .unwrap_or_default()
            .iter()
            .any(|d| d.code == code)
    }
}

/// Where the corpus is, or `None` when this checkout does not have it.
///
/// `LIYASA_CORPUS` wins; otherwise the layout `bin/wt` builds:
/// `<prep>/wt/wp-NN/crates/liyasa-verify` with the corpus at
/// `<prep>/spec/markdown`.
pub fn root() -> Option<PathBuf> {
    let candidate = match std::env::var("LIYASA_CORPUS") {
        Ok(path) => PathBuf::from(path),
        Err(_) => Path::new(env!("CARGO_MANIFEST_DIR")).join("../../../../spec/markdown"),
    };
    holds_verification_cases(&candidate).then_some(candidate)
}

/// Whether a corpus root holds THIS crate's cases.
///
/// `LIYASA_CORPUS` names the corpus root, and two crates read it for
/// different things: `liyasa-markdown` wants the conformance cases, this
/// crate wants the `ver-*` verification cases that sit beside them in
/// `spec/markdown`. CI imports only the upstream CommonMark and GFM suites,
/// because those are the half with an external source of truth — a complete
/// corpus for one reader and an empty one for the other. So "the directory
/// exists" is not evidence that this crate's cases are in it, and taking it
/// as evidence turned four no-ops into four hard failures the day CI started
/// setting the variable.
fn holds_verification_cases(root: &Path) -> bool {
    let Ok(entries) = std::fs::read_dir(root) else {
        return false;
    };
    entries.flatten().any(|entry| {
        entry.path().is_dir() && entry.file_name().to_string_lossy().starts_with("ver-")
    })
}

/// Where the third-party Vale style packages are, or `None` when this checkout
/// does not have them.
///
/// VER-61 names Google's and Microsoft's packages. They are fetched rather
/// than committed, the same as the Markdown corpus, and
/// `spec/vale/styles/PINNED` records the commit each was taken from.
/// `LIYASA_VALE_STYLES` wins; otherwise the layout `bin/wt` builds.
pub fn vale_styles() -> Option<PathBuf> {
    if let Ok(path) = std::env::var("LIYASA_VALE_STYLES") {
        let path = PathBuf::from(path);
        return path.is_dir().then_some(path);
    }
    let manifest = Path::new(env!("CARGO_MANIFEST_DIR"));
    let candidate = manifest.join("../../../../spec/vale");
    candidate.is_dir().then_some(candidate)
}

/// Every case under `root/<dir>`, sorted by path.
pub fn cases(root: &Path, dir: &str) -> Vec<Case> {
    let mut out = Vec::new();
    collect(&root.join(dir), &mut out);
    out.sort_by(|a, b| a.path.cmp(&b.path));
    out
}

fn collect(dir: &Path, out: &mut Vec<Case>) {
    let Ok(entries) = std::fs::read_dir(dir) else {
        return;
    };
    for entry in entries.flatten() {
        let path = entry.path();
        if path.is_dir() {
            collect(&path, out);
        } else if path.extension().is_some_and(|e| e == "md")
            && let Ok(text) = std::fs::read_to_string(&path)
            && let Some(case) = parse(&path, &text)
        {
            out.push(case);
        }
    }
}

/// The `%%% <section>` format the corpus README documents. A line is a
/// delimiter only when it is exactly `%%% ` followed by a section name, so
/// `%%%` in content needs no escaping.
fn parse(path: &Path, text: &str) -> Option<Case> {
    const SECTIONS: &[&str] = &[
        "case",
        "source",
        "html",
        "markdown",
        "ast",
        "source-document",
        "diagnostics",
        "end",
    ];
    let mut sections: Vec<(&str, String)> = Vec::new();
    let mut current: Option<(&str, String)> = None;
    for line in text.lines() {
        let delimiter = line
            .strip_prefix("%%% ")
            .and_then(|rest| SECTIONS.iter().find(|s| **s == rest.trim()));
        match delimiter {
            Some(name) => {
                if let Some(done) = current.take() {
                    sections.push(done);
                }
                if *name != "end" {
                    current = Some((name, String::new()));
                }
            }
            None => {
                if let Some((_, body)) = &mut current {
                    body.push_str(line);
                    body.push('\n');
                }
            }
        }
    }
    if let Some(done) = current {
        sections.push(done);
    }

    let find = |name: &str| {
        sections
            .iter()
            .find(|(section, _)| *section == name)
            .map(|(_, body)| body.as_str())
    };
    Some(Case {
        path: path.to_owned(),
        header: serde_json::from_str(find("case")?).ok()?,
        source: find("source")?.to_owned(),
        diagnostics: find("diagnostics").and_then(|body| serde_json::from_str(body).ok()),
    })
}

#[cfg(test)]
mod tests;
