//! Turns `src/diagnostics/codes.toml` into constants and a registry table.
//!
//! Hand-rolled rather than using the `toml` crate: the registry is append-only
//! and line-oriented by construction (see the file header), so a line parser is
//! enough and `liyasa-core` gains no build dependency. Malformed lines fail the
//! build here; registry *invariants* (duplicates, ranges, prefix agreement) are
//! asserted by `tests/registry.rs`, where the failure is readable.

use std::fmt::Write as _;
use std::path::Path;

const REGISTRY: &str = "src/diagnostics/codes.toml";

fn main() {
    println!("cargo::rerun-if-changed={REGISTRY}");
    let text = std::fs::read_to_string(REGISTRY).expect("codes.toml is readable");

    let mut ranges = String::new();
    let mut rows: Vec<(String, String, String)> = Vec::new();
    let mut section = "";

    for (n, raw) in text.lines().enumerate() {
        let line = raw.trim();
        if line.is_empty() || line.starts_with('#') {
            continue;
        }
        if let Some(name) = line.strip_prefix('[').and_then(|l| l.strip_suffix(']')) {
            section = match name {
                "ranges" => "ranges",
                "codes" => "codes",
                other => panic!("{REGISTRY}:{}: unknown section [{other}]", n + 1),
            };
            continue;
        }
        let (key, value) = split_row(line)
            .unwrap_or_else(|| panic!("{REGISTRY}:{}: expected `key = {{ .. }}`", n + 1));
        let at = |field: &str| {
            field_of(value, field)
                .unwrap_or_else(|| panic!("{REGISTRY}:{}: missing `{field}`", n + 1))
        };
        match section {
            "ranges" => {
                let (lo, hi) = key
                    .split_once('-')
                    .unwrap_or_else(|| panic!("{REGISTRY}:{}: range key is `NNNN-NNNN`", n + 1));
                let parse = |s: &str| {
                    s.parse::<u16>()
                        .unwrap_or_else(|_| panic!("{REGISTRY}:{}: `{s}` is not a number", n + 1))
                };
                let _ = writeln!(
                    ranges,
                    "    Range {{ first: {}, last: {}, area: {:?}, krate: {:?} }},",
                    parse(lo),
                    parse(hi),
                    at("area"),
                    at("crate"),
                );
            }
            "codes" => {
                let severity = match at("severity").as_str() {
                    "error" => "Severity::Error",
                    "warning" => "Severity::Warning",
                    "info" => "Severity::Info",
                    "hint" => "Severity::Hint",
                    other => panic!("{REGISTRY}:{}: unknown severity `{other}`", n + 1),
                };
                let url = match field_of(value, "url") {
                    Some(u) => format!("Some({u:?})"),
                    None => "None".to_owned(),
                };
                rows.push((
                    key.to_owned(),
                    format!(
                        "    CodeInfo {{ code: Code({key:?}), severity: {severity}, \
                         krate: {:?}, title: {:?}, url: {url} }},",
                        at("crate"),
                        at("title"),
                    ),
                    format!("    pub const {key}: Code = Code({key:?});"),
                ));
            }
            _ => panic!("{REGISTRY}:{}: row outside any section", n + 1),
        }
    }

    // `codes.toml` is append-only, so it is in claim order; `Code::new`
    // binary-searches the emitted table, which must be sorted.
    rows.sort_by(|a, b| a.0.cmp(&b.0));
    let entries: String = rows.iter().map(|r| format!("{}\n", r.1)).collect();
    let consts: String = rows.iter().map(|r| format!("{}\n", r.2)).collect();

    let out = format!(
        "// @generated from {REGISTRY} by build.rs; do not edit.\n\
         pub(crate) static RANGES: &[Range] = &[\n{ranges}];\n\n\
         pub(crate) static REGISTRY: &[CodeInfo] = &[\n{entries}];\n\n\
         /// One constant per row of `codes.toml`, so a `Code` that is not\n\
         /// registered cannot be named.\n\
         pub mod code {{\n    use super::Code;\n\n{consts}}}\n"
    );
    let dir = std::env::var("OUT_DIR").expect("OUT_DIR is set");
    std::fs::write(Path::new(&dir).join("codes.rs"), out).expect("generated file is writable");
}

/// `KEY = { .. }` into `("KEY", "..")`, tolerating a quoted key.
fn split_row(line: &str) -> Option<(&str, &str)> {
    let (key, rest) = line.split_once('=')?;
    let body = rest
        .trim()
        .strip_prefix('{')?
        .trim_end()
        .strip_suffix('}')?;
    Some((key.trim().trim_matches('"'), body))
}

/// The value of `name = "…"` within one inline table, unescaped. Values are
/// quoted strings whose only escapes are `\"` and `\\`, which is all
/// `codes.toml` ever holds.
fn field_of(body: &str, name: &str) -> Option<String> {
    let mut rest = body;
    loop {
        let eq = rest.find('=')?;
        let key = rest[..eq].trim().trim_start_matches(',').trim();
        let after = rest[eq + 1..].trim_start();
        let open = after.strip_prefix('"')?;
        let mut end = 0;
        let bytes = open.as_bytes();
        while end < bytes.len() {
            match bytes[end] {
                b'\\' => end += 2,
                b'"' => break,
                _ => end += 1,
            }
        }
        if key == name {
            return Some(open[..end].replace("\\\"", "\"").replace("\\\\", "\\"));
        }
        rest = open.get(end + 1..)?;
    }
}
