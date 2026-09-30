//! A fixture path whose only discriminator is the process id (defect 248).
//!
//! `bin/gate` runs `cargo nextest`, which is one process per test. CI runs
//! `cargo test`, which runs a binary's tests as **threads of one process**. So
//! two `#[test]`s that build a scratch directory from `process::id()` alone
//! resolve to the same path on CI and to different paths under the gate — and
//! the usual fixture shape opens with `remove_dir_all` and closes with another,
//! so one test deletes the tree the other is asserting against.
//!
//! `tests/build/api_10_spec_pages.rs` did exactly this and took `main` red
//! while every session read its own green gate. **The gate cannot catch this
//! class at all** — not by being less thorough, but by using a runner whose
//! isolation hides it. That is why the defence is a check over the source
//! rather than a rule to remember.
//!
//! The fix is always the same and always cheap: interpolate something else as
//! well — the test's own name, a counter, the thread id.

use std::path::{Path, PathBuf};

/// A `.join(format!(…))` whose template has one hole and fills it with the
/// process id.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PidOnly {
    pub file: PathBuf,
    pub line: usize,
    /// The format template, as written.
    pub template: String,
}

/// Every pid-only fixture path in one Rust source, as `(line, template)`.
///
/// Reads only `.join(format!(…))`, which is how every fixture in this tree
/// spells it. `PathBuf::from(format!(…))` would be missed; that is a false
/// negative, which is the right way round for a lint — the alternative is
/// matching every `format!` that mentions the process id, and the two that do
/// so outside a path would have to be argued about forever.
pub fn scan(text: &str) -> Vec<(usize, String)> {
    let masked = mask_raw_strings(text);
    let text = masked.as_str();
    let mut found = Vec::new();
    let bytes = text.as_bytes();
    let mut at = 0usize;
    while let Some(hit) = text[at..].find("format!(").map(|i| at + i) {
        at = hit + "format!(".len();
        if !text[..hit].trim_end().ends_with(".join(") {
            continue;
        }
        let Some(close) = matching_paren(bytes, at) else {
            continue;
        };
        let inside = &text[at..close];
        let Some((template, args)) = split_template(inside) else {
            continue;
        };
        if holes(&template) == 1 && is_process_id(args) {
            found.push((text[..hit].matches('\n').count() + 1, template));
        }
        at = close;
    }
    found
}

/// Blank the body of every raw string, keeping byte offsets and line breaks.
///
/// This module's own test data is Rust source inside `r#"…"#`, so the first
/// run reported the test that proves it works — `lints.rs` had the same
/// accident with `#![feature(`. A raw string is data whatever it contains, so
/// masking is the general form of the fix rather than an exemption for this
/// file.
fn mask_raw_strings(text: &str) -> String {
    let bytes = text.as_bytes();
    let mut out = String::with_capacity(text.len());
    let mut at = 0usize;
    while at < bytes.len() {
        let hashes = raw_string_at(bytes, at);
        if hashes == 0 {
            out.push(text[at..].chars().next().unwrap_or(' '));
            at += text[at..].chars().next().map_or(1, char::len_utf8);
            continue;
        }
        let open = at + 1 + hashes + 1; // r, #.., "
        out.push_str(&text[at..open]);
        let terminator = format!("\"{}", "#".repeat(hashes));
        let close = text[open..]
            .find(&terminator)
            .map_or(bytes.len(), |i| open + i);
        for c in text[open..close].chars() {
            out.push(if c == '\n' { '\n' } else { ' ' });
        }
        out.push_str(&text[close..(close + terminator.len()).min(bytes.len())]);
        at = (close + terminator.len()).min(bytes.len());
    }
    out
}

/// The number of `#` in a raw string opening at `at`, or 0 if none opens here.
fn raw_string_at(bytes: &[u8], at: usize) -> usize {
    if bytes[at] != b'r'
        || (at > 0 && (bytes[at - 1] == b'_' || bytes[at - 1].is_ascii_alphanumeric()))
    {
        return 0;
    }
    let mut hashes = 0usize;
    while at + 1 + hashes < bytes.len() && bytes[at + 1 + hashes] == b'#' {
        hashes += 1;
    }
    if hashes > 0 && bytes.get(at + 1 + hashes) == Some(&b'"') {
        hashes
    } else {
        0
    }
}

/// The index of the `)` closing the paren that `from` sits just inside.
fn matching_paren(bytes: &[u8], from: usize) -> Option<usize> {
    let mut depth = 1usize;
    let mut at = from;
    while at < bytes.len() {
        match bytes[at] {
            b'"' => at = end_of_string(bytes, at)?,
            b'(' => depth += 1,
            b')' => {
                depth -= 1;
                if depth == 0 {
                    return Some(at);
                }
            }
            _ => {}
        }
        at += 1;
    }
    None
}

/// The index of the `"` closing the string literal opening at `from`.
fn end_of_string(bytes: &[u8], from: usize) -> Option<usize> {
    let mut at = from + 1;
    while at < bytes.len() {
        match bytes[at] {
            b'\\' => at += 1,
            b'"' => return Some(at),
            _ => {}
        }
        at += 1;
    }
    None
}

/// The format template and the argument text after it.
fn split_template(inside: &str) -> Option<(String, &str)> {
    let open = inside.find('"')?;
    if !inside[..open]
        .trim_matches(|c: char| c.is_whitespace() || c == '\n')
        .is_empty()
    {
        return None;
    }
    let close = end_of_string(inside.as_bytes(), open)?;
    Some((inside[open + 1..close].to_owned(), &inside[close + 1..]))
}

/// Interpolations in a template. `{{` and `}}` are escaped braces, not holes.
fn holes(template: &str) -> usize {
    let mut count = 0;
    let mut chars = template.chars().peekable();
    while let Some(c) = chars.next() {
        match c {
            '{' if chars.peek() == Some(&'{') => {
                chars.next();
            }
            '}' if chars.peek() == Some(&'}') => {
                chars.next();
            }
            '{' => count += 1,
            _ => {}
        }
    }
    count
}

/// Whether the one argument is the process id and nothing else.
fn is_process_id(args: &str) -> bool {
    let bare: String = args
        .chars()
        .filter(|c| !c.is_whitespace())
        .collect::<String>()
        .trim_start_matches(',')
        .trim_end_matches(',')
        .to_owned();
    bare == "std::process::id()" || bare == "process::id()"
}

/// Every pid-only fixture path under the workspace.
pub fn audit(root: &Path) -> Result<Vec<PidOnly>, String> {
    let mut found = Vec::new();
    for dir in ["crates", "xtask", "benches", "tests"] {
        let start = root.join(dir);
        if start.is_dir() {
            walk(&start, &mut found)?;
        }
    }
    found.sort_by(|a, b| (&a.file, a.line).cmp(&(&b.file, b.line)));
    Ok(found)
}

fn walk(dir: &Path, found: &mut Vec<PidOnly>) -> Result<(), String> {
    for entry in std::fs::read_dir(dir).map_err(|e| format!("{}: {e}", dir.display()))? {
        let path = entry.map_err(|e| format!("{}: {e}", dir.display()))?.path();
        if path.is_dir() {
            if path.file_name().is_some_and(|n| n == "target") {
                continue;
            }
            walk(&path, found)?;
        } else if path.extension().is_some_and(|e| e == "rs") {
            let text =
                std::fs::read_to_string(&path).map_err(|e| format!("{}: {e}", path.display()))?;
            for (line, template) in scan(&text) {
                found.push(PidOnly {
                    file: path.clone(),
                    line,
                    template,
                });
            }
        }
    }
    Ok(())
}

impl PidOnly {
    /// The pin entry: where it is, and what it builds. No line number — the pin
    /// would then go stale on every edit above it, and a ratchet that churns is
    /// a ratchet people stop reading.
    pub fn entry(&self, root: &Path) -> String {
        let path = self
            .file
            .strip_prefix(root)
            .unwrap_or(&self.file)
            .to_string_lossy()
            .replace('\\', "/");
        format!("{}:{}", path.trim_start_matches("./"), self.prefix())
    }

    /// The template up to its first hole.
    pub fn prefix(&self) -> String {
        self.template
            .split('{')
            .next()
            .unwrap_or_default()
            .trim_end_matches(['-', '.', '_', '/'])
            .to_owned()
    }
}

/// Entry point for `xtask fixtures`.
pub fn run(root: &Path) -> Result<(), String> {
    let found = audit(root)?;
    for f in &found {
        println!("  {}:{}: {}", f.file.display(), f.line, f.template);
    }
    println!("fixtures: {} keyed on the process id alone", found.len());
    Ok(())
}
