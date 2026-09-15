//! The conformance corpus format (PRD §30.9).
//!
//! One case per file under `spec/markdown/`. A case is a header in JSON
//! followed by the source and whichever expectations the case asserts:
//!
//! ```text
//! %%% case
//! { "id": "cm-50/containers/list-item", "requirement": "CM-50", "tags": ["directive"] }
//! %%% source
//! - item
//!   :::note
//!   body
//!   :::
//! %%% html
//! <ul>…</ul>
//! %%% diagnostics
//! []
//! %%% end
//! ```
//!
//! A line is a delimiter only when it is exactly `%%% ` followed by a known
//! section name, so `%%%` in content needs no escaping. Every section except
//! `case` and `source` is optional; an expectation the engine under test cannot
//! produce is skipped, never silently passed.

use std::collections::BTreeMap;
use std::path::{Path, PathBuf};

use serde::{Deserialize, Serialize};

pub const SECTIONS: &[&str] = &[
    "case",
    "source",
    "html",
    "markdown",
    "ast",
    "source-document",
    "diagnostics",
    "end",
];

/// What a case asserts about a diagnostic. Only the fields present are
/// compared, so a case can pin the code without pinning the wording.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ExpectedDiagnostic {
    pub code: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub line: Option<u32>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub col: Option<u32>,
    /// A substring of the message.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub message: Option<String>,
}

#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct CaseHeader {
    pub id: String,
    /// The requirement ID this case covers, when it has one.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub requirement: Option<String>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub tags: Vec<String>,
    /// Where an imported case came from, such as `commonmark-0.31.2#123`.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub origin: Option<String>,
    /// Parse options that differ from the defaults, as `liyasa.json` spells
    /// them (`content.html`, `content.math`, …).
    #[serde(default, skip_serializing_if = "BTreeMap::is_empty")]
    pub options: BTreeMap<String, serde_json::Value>,
    /// Set when a case is known to fail and is not yet a merge blocker; the
    /// reason is required so the list cannot grow silently.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub pending: Option<String>,
    /// The engine that produced the expectations, when they were generated
    /// rather than written. Generated expectations are candidates until the
    /// fixture reviewer clears them.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub generated: Option<String>,
    /// Set by the fixture adversarial reviewer once this case's expected output
    /// has been checked against the reference implementations.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub reviewed: Option<String>,
}

#[derive(Debug, Clone, PartialEq)]
pub struct Case {
    pub path: PathBuf,
    pub header: CaseHeader,
    pub source: String,
    pub html: Option<String>,
    pub markdown: Option<String>,
    pub ast: Option<serde_json::Value>,
    pub source_document: Option<serde_json::Value>,
    pub diagnostics: Option<Vec<ExpectedDiagnostic>>,
}

impl Case {
    /// The expectations this case asserts, for reporting coverage.
    pub fn asserted(&self) -> Vec<&'static str> {
        let mut out = Vec::new();
        if self.html.is_some() {
            out.push("html");
        }
        if self.markdown.is_some() {
            out.push("markdown");
        }
        if self.ast.is_some() {
            out.push("ast");
        }
        if self.source_document.is_some() {
            out.push("source-document");
        }
        if self.diagnostics.is_some() {
            out.push("diagnostics");
        }
        out
    }
}

pub fn parse(path: &Path, text: &str) -> Result<Case, String> {
    let mut sections: BTreeMap<&str, String> = BTreeMap::new();
    let mut current: Option<&str> = None;
    let mut body = String::new();

    for line in text.split_inclusive('\n') {
        if let Some(name) = delimiter(line) {
            if let Some(previous) = current.replace(name)
                && sections
                    .insert(previous, std::mem::take(&mut body))
                    .is_some()
            {
                return Err(format!(
                    "{}: section `{previous}` appears twice",
                    path.display()
                ));
            }
            if name == "end" {
                break;
            }
            continue;
        }
        if current.is_some() {
            body.push_str(line);
        } else if !line.trim().is_empty() {
            return Err(format!(
                "{}: content before the first `%%% case`",
                path.display()
            ));
        }
    }
    if let Some(previous) = current
        && previous != "end"
    {
        sections.insert(previous, body);
    }

    let header_text = sections
        .get("case")
        .ok_or_else(|| format!("{}: no `%%% case` section", path.display()))?;
    let header: CaseHeader = serde_json::from_str(header_text)
        .map_err(|e| format!("{}: case header is not valid JSON: {e}", path.display()))?;
    let source = sections
        .get("source")
        .ok_or_else(|| format!("{}: no `%%% source` section", path.display()))?
        .clone();

    let json = |name: &str| -> Result<Option<serde_json::Value>, String> {
        sections
            .get(name)
            .map(|text| {
                serde_json::from_str(text)
                    .map_err(|e| format!("{}: `{name}` is not valid JSON: {e}", path.display()))
            })
            .transpose()
    };

    Ok(Case {
        path: path.to_path_buf(),
        header,
        source,
        html: sections.get("html").cloned(),
        markdown: sections.get("markdown").cloned(),
        ast: json("ast")?,
        source_document: json("source-document")?,
        diagnostics: sections
            .get("diagnostics")
            .map(|text| {
                serde_json::from_str(text).map_err(|e| {
                    format!("{}: `diagnostics` is not valid JSON: {e}", path.display())
                })
            })
            .transpose()?,
    })
}

/// Renders a case back to its file form. The inverse of [`parse`].
pub fn write(case: &Case) -> Result<String, String> {
    let json = |value: &serde_json::Value| {
        serde_json::to_string_pretty(value).map_err(|e| format!("{}: {e}", case.header.id))
    };

    let mut out = String::new();
    out.push_str("%%% case\n");
    out.push_str(
        &serde_json::to_string(&case.header).map_err(|e| format!("{}: {e}", case.header.id))?,
    );
    out.push('\n');
    out.push_str("%%% source\n");
    out.push_str(&case.source);
    ensure_newline(&mut out);

    for (name, body) in [
        ("html", case.html.as_ref()),
        ("markdown", case.markdown.as_ref()),
    ] {
        if let Some(body) = body {
            out.push_str(&format!("%%% {name}\n"));
            out.push_str(body);
            ensure_newline(&mut out);
        }
    }
    for (name, value) in [
        ("ast", case.ast.as_ref()),
        ("source-document", case.source_document.as_ref()),
    ] {
        if let Some(value) = value {
            out.push_str(&format!("%%% {name}\n"));
            out.push_str(&json(value)?);
            out.push('\n');
        }
    }
    if let Some(diagnostics) = &case.diagnostics {
        out.push_str("%%% diagnostics\n");
        out.push_str(
            &serde_json::to_string(diagnostics).map_err(|e| format!("{}: {e}", case.header.id))?,
        );
        out.push('\n');
    }
    out.push_str("%%% end\n");
    Ok(out)
}

/// Every case under `dir`, ordered by path so runs are reproducible.
pub fn load(dir: &Path) -> Result<Vec<Case>, String> {
    let mut paths = Vec::new();
    collect(dir, &mut paths)?;
    paths.sort();
    let mut cases = Vec::new();
    for path in paths {
        let text =
            std::fs::read_to_string(&path).map_err(|e| format!("{}: {e}", path.display()))?;
        cases.push(parse(&path, &text)?);
    }
    let mut seen = BTreeMap::new();
    for case in &cases {
        if let Some(first) = seen.insert(case.header.id.clone(), case.path.clone()) {
            return Err(format!(
                "duplicate case id `{}` in {} and {}",
                case.header.id,
                first.display(),
                case.path.display()
            ));
        }
    }
    Ok(cases)
}

fn collect(dir: &Path, out: &mut Vec<PathBuf>) -> Result<(), String> {
    let entries = std::fs::read_dir(dir).map_err(|e| format!("{}: {e}", dir.display()))?;
    for entry in entries {
        let path = entry.map_err(|e| format!("{}: {e}", dir.display()))?.path();
        if path.is_dir() {
            collect(&path, out)?;
        } else if path.extension().is_some_and(|e| e == "md")
            && path.file_name().is_some_and(|n| n != "README.md")
        {
            // README.md is the corpus's own documentation, not a case.
            out.push(path);
        }
    }
    Ok(())
}

fn delimiter(line: &str) -> Option<&'static str> {
    let name = line.strip_prefix("%%% ")?.trim_end_matches(['\r', '\n']);
    SECTIONS.iter().copied().find(|s| *s == name)
}

fn ensure_newline(out: &mut String) {
    if !out.ends_with('\n') {
        out.push('\n');
    }
}
