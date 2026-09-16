//! Which registered codes nothing raises (RFC 0008).
//!
//! Claiming a code before the thing that raises it is the intended workflow, so
//! this is not a defect list — it is a set that should only shrink, pinned in
//! `tests/pins/unraised-codes.txt`.
//!
//! The scan is literal, which is exact on this tree: emission is `code::NNNN`
//! generated from the registry, or `Code::new("NNNN")`. The `Code::new(<var>)`
//! sites are lookups — a user's filter, a code parsed back out of a minijinja
//! message, a SARIF rule id — not emissions.
//!
//! `tests/` and `xtask/` are deliberately not scanned. A code a test names is
//! not a code the product raises, which is how `E0721` and `W0720` stay
//! visible: they appear only in `tests/src/budget.rs` under `TODO(rfc-1101)`.

use std::collections::BTreeSet;
use std::path::{Path, PathBuf};

/// Registered codes that nothing in `crates/*/src/` raises.
pub fn unraised(root: &Path) -> Result<BTreeSet<String>, String> {
    let registry = root.join("crates/liyasa-core/src/diagnostics/codes.toml");
    let toml =
        std::fs::read_to_string(&registry).map_err(|e| format!("{}: {e}", registry.display()))?;
    let registered = registered(&toml);
    if registered.len() < 100 {
        return Err(format!(
            "only {} codes parsed out of codes.toml",
            registered.len()
        ));
    }

    let mut raised = BTreeSet::new();
    for file in sources(&root.join("crates"))? {
        raised.extend(codes_in(
            &std::fs::read_to_string(&file).unwrap_or_default(),
        ));
    }
    if raised.len() < 100 {
        return Err(format!(
            "the scan found only {} codes, so it is broken",
            raised.len()
        ));
    }
    Ok(registered.difference(&raised).cloned().collect())
}

/// The keys of the `[codes]` table.
fn registered(toml: &str) -> BTreeSet<String> {
    let mut inside = false;
    let mut out = BTreeSet::new();
    for line in toml.lines() {
        let line = line.trim();
        if line.starts_with('[') {
            inside = line == "[codes]";
            continue;
        }
        if !inside {
            continue;
        }
        if let Some((key, _)) = line.split_once('=')
            && is_code(key.trim())
        {
            out.insert(key.trim().to_owned());
        }
    }
    out
}

fn is_code(text: &str) -> bool {
    let bytes = text.as_bytes();
    bytes.len() == 5 && matches!(bytes[0], b'E' | b'W') && bytes[1..].iter().all(u8::is_ascii_digit)
}

/// Every `[EW]dddd` in a file's code and string literals, skipping comments.
///
/// Comments have to go: five codes — `E0110`, `E0620`, `W0622`, `W1001`,
/// `W1005` — are named in exactly one doc comment each and raised nowhere, and
/// a scan that counted prose would call them raised. String literals stay,
/// because `Code::new("E0805")` is a real emission. So this tracks state rather
/// than stripping `//` by regex, which a URL inside a string would defeat.
pub fn codes_in(body: &str) -> BTreeSet<String> {
    #[derive(PartialEq)]
    enum In {
        Code,
        Str,
        RawStr,
        Line,
        Block,
    }

    let bytes = body.as_bytes();
    let mut out = BTreeSet::new();
    let mut state = In::Code;
    let mut at = 0usize;
    while at < bytes.len() {
        let rest = &bytes[at..];
        match state {
            In::Line if rest[0] == b'\n' => state = In::Code,
            In::Block if rest.starts_with(b"*/") => {
                state = In::Code;
                at += 1;
            }
            In::Line | In::Block => {}
            In::Str if rest[0] == b'\\' => at += 1,
            In::Str | In::RawStr if rest[0] == b'"' => state = In::Code,
            In::Code if rest.starts_with(b"//") => state = In::Line,
            In::Code if rest.starts_with(b"/*") => {
                state = In::Block;
                at += 1;
            }
            In::Code if rest.starts_with(b"r\"") => {
                state = In::RawStr;
                at += 1;
            }
            In::Code if rest[0] == b'"' => state = In::Str,
            In::Code | In::Str | In::RawStr => {
                if let Some(code) = code_at(body, bytes, at) {
                    out.insert(code);
                }
            }
        }
        at += 1;
    }
    out
}

fn code_at(body: &str, bytes: &[u8], at: usize) -> Option<String> {
    if !matches!(bytes[at], b'E' | b'W') {
        return None;
    }
    let before = at > 0 && (bytes[at - 1].is_ascii_alphanumeric() || bytes[at - 1] == b'_');
    let after = bytes
        .get(at + 5)
        .is_some_and(|b| b.is_ascii_alphanumeric() || *b == b'_');
    if before || after {
        return None;
    }
    let candidate = body.get(at..at + 5)?;
    is_code(candidate).then(|| candidate.to_owned())
}

fn sources(dir: &Path) -> Result<Vec<PathBuf>, String> {
    let mut out = Vec::new();
    for entry in std::fs::read_dir(dir).map_err(|e| format!("{}: {e}", dir.display()))? {
        let src = entry
            .map_err(|e| format!("{}: {e}", dir.display()))?
            .path()
            .join("src");
        if src.is_dir() {
            walk(&src, &mut out)?;
        }
    }
    out.sort();
    Ok(out)
}

fn walk(dir: &Path, out: &mut Vec<PathBuf>) -> Result<(), String> {
    for entry in std::fs::read_dir(dir).map_err(|e| format!("{}: {e}", dir.display()))? {
        let path = entry.map_err(|e| format!("{}: {e}", dir.display()))?.path();
        if path.is_dir() {
            walk(&path, out)?;
        } else if path.extension().is_some_and(|e| e == "rs") {
            out.push(path);
        }
    }
    Ok(())
}
