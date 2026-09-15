//! The conformance corpus, run against this crate (PRD §30.9).
//!
//! `xtask conformance` runs the corpus through the parser candidates of the
//! spike, and none of them produces diagnostics, so every `diagnostics` section
//! in `spec/markdown/` is reported as skipped there. This module is what makes
//! those sections assert something: it reads the same files and runs them
//! through `ast::parse`.
//!
//! The corpus is never committed (it is regenerated from its sources), so a
//! checkout without it skips rather than fails. `LIYASA_CORPUS` overrides where
//! to look; otherwise the search walks up from this crate.

use std::path::{Path, PathBuf};

/// One case, reduced to what this crate can be held to.
pub struct Case {
    pub id: String,
    pub path: PathBuf,
    pub source: String,
    pub options: serde_json::Map<String, serde_json::Value>,
    pub tags: Vec<String>,
    pub pending: Option<String>,
    /// The codes the case expects, in the order it lists them.
    pub diagnostics: Option<Vec<String>>,
}

/// The codes `ast::parse` can raise.
///
/// The corpus covers the whole pipeline, so a case may expect `E0101` from the
/// front matter reader or `E0202` from the template scanner. Neither is this
/// crate's parser to raise, and holding it to them would make the test fail for
/// work it does not do.
pub const RAISED_HERE: &[&str] = &[
    "E0303", "E0304", "E0310", "E0311", "E0312", "E0313", "E0314", "E0315", "E0317", "E0318",
    "E0350", "W0302", "W0316", "W0319",
];

/// The subset of [`RAISED_HERE`] that does not depend on a component registry.
///
/// The corpus names components for illustration — `:::a`, `:::b`, `::img` — and
/// ships no schema for them, so whether `E0313` or `E0314` fires on a case is a
/// fact about whichever registry the test supplies, not about the case. Those
/// codes are covered against a known registry in `directives::validate`; here
/// only the codes the parser reaches on its own are held against the corpus.
pub const REGISTRY_FREE: &[&str] = &[
    "E0303", "E0304", "E0310", "E0311", "E0312", "E0318", "W0302", "W0319",
];

/// Every case under the corpus, or `None` when it is not checked out.
pub fn load() -> Option<Vec<Case>> {
    let dir = locate()?;
    let mut paths = Vec::new();
    collect(&dir, &mut paths);
    paths.sort();
    Some(paths.iter().filter_map(|path| read(path)).collect())
}

fn locate() -> Option<PathBuf> {
    if let Ok(from_env) = std::env::var("LIYASA_CORPUS") {
        let path = PathBuf::from(from_env);
        return path.is_dir().then_some(path);
    }
    let mut at: &Path = Path::new(env!("CARGO_MANIFEST_DIR"));
    loop {
        let candidate = at.join("spec/markdown");
        if candidate.is_dir() {
            return Some(candidate);
        }
        at = at.parent()?;
    }
}

fn collect(dir: &Path, out: &mut Vec<PathBuf>) {
    let Ok(entries) = std::fs::read_dir(dir) else {
        return;
    };
    for entry in entries.flatten() {
        let path = entry.path();
        if path.is_dir() {
            collect(&path, out);
        } else if path.extension().is_some_and(|e| e == "md")
            && path.file_name().is_some_and(|n| n != "README.md")
        {
            out.push(path);
        }
    }
}

/// The `%%% <section>` format of `spec/markdown/README.md`, read for the three
/// sections this crate uses.
fn read(path: &Path) -> Option<Case> {
    let text = std::fs::read_to_string(path).ok()?;
    let mut sections: Vec<(String, String)> = Vec::new();
    for line in text.split_inclusive('\n') {
        match line.strip_prefix("%%% ").map(|name| name.trim_end()) {
            Some(name) if is_section(name) => sections.push((name.to_owned(), String::new())),
            _ => {
                if let Some((_, body)) = sections.last_mut() {
                    body.push_str(line);
                }
            }
        }
    }
    let section = |want: &str| {
        sections
            .iter()
            .find(|(name, _)| name == want)
            .map(|(_, body)| body.clone())
    };

    let header: serde_json::Value = serde_json::from_str(&section("case")?).ok()?;
    Some(Case {
        id: header.get("id")?.as_str()?.to_owned(),
        path: path.to_path_buf(),
        source: section("source")?,
        options: header
            .get("options")
            .and_then(|o| o.as_object().cloned())
            .unwrap_or_default(),
        tags: header
            .get("tags")
            .and_then(|t| t.as_array())
            .map(|tags| {
                tags.iter()
                    .filter_map(|tag| tag.as_str().map(str::to_owned))
                    .collect()
            })
            .unwrap_or_default(),
        pending: header
            .get("pending")
            .and_then(|p| p.as_str())
            .map(str::to_owned),
        diagnostics: section("diagnostics").and_then(|body| {
            let raised: Vec<serde_json::Value> = serde_json::from_str(&body).ok()?;
            raised
                .iter()
                .map(|d| Some(d.get("code")?.as_str()?.to_owned()))
                .collect()
        }),
    })
}

fn is_section(name: &str) -> bool {
    matches!(
        name,
        "case" | "source" | "html" | "markdown" | "ast" | "source-document" | "diagnostics" | "end"
    )
}

#[cfg(test)]
mod tests;
