//! Hidden lines (VER-04).
//!
//! A line carrying `verify.hide_prefix` is executed with the prefix stripped
//! and left out of what the reader sees, so setup stays out of the sample.
//! The prefix defaults to `# `, which is also the line comment of half the
//! languages a docs site shows; `applies` is where RFC 2100 draws that line.

use liyasa_core::verify::CheckInput;

/// Languages whose line comment starts with `#`. A hide prefix that is one of
/// their comment markers does not hide in them (RFC 2100).
const COMMENT_HASH: &[&str] = &[
    "bash",
    "cmake",
    "conf",
    "dockerfile",
    "fish",
    "make",
    "makefile",
    "perl",
    "powershell",
    "python",
    "python3",
    "r",
    "ruby",
    "sh",
    "shell",
    "toml",
    "yaml",
    "yml",
    "zsh",
];

/// The default `verify.hide_prefix`.
pub const DEFAULT_PREFIX: &str = "# ";

/// Whether `prefix` hides lines in `lang` at all.
pub fn applies(lang: &str, prefix: &str) -> bool {
    let prefix = prefix.trim();
    if prefix.is_empty() {
        return false;
    }
    let lang = lang.trim().to_ascii_lowercase();
    !(prefix == "#" && COMMENT_HASH.contains(&lang.as_str()))
}

/// A block split into what runs and what is shown.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Split {
    /// Every line, prefixes stripped: what the sandbox is given.
    pub executed: String,
    /// The lines the reader sees, in order.
    pub visible: String,
    /// 1-based line numbers of the hidden lines, for `CheckInput::Code`.
    pub hidden_lines: Vec<u32>,
}

/// Splits `source` on `prefix`. A line is hidden when its first non-blank run
/// starts with the prefix; `##` at that position is the doctest escape and
/// renders as a single `#`.
pub fn split(lang: &str, source: &str, prefix: &str) -> Split {
    if !applies(lang, prefix) {
        return Split {
            executed: source.to_owned(),
            visible: source.to_owned(),
            hidden_lines: Vec::new(),
        };
    }
    let marker = prefix.trim_end();
    let mut executed = Vec::new();
    let mut visible = Vec::new();
    let mut hidden_lines = Vec::new();
    for (index, line) in source.lines().enumerate() {
        let indent = line.len() - line.trim_start().len();
        let (space, rest) = line.split_at(indent);
        if let Some(tail) = rest.strip_prefix(marker) {
            if let Some(escaped) = tail.strip_prefix('#') {
                let shown = format!("{space}#{escaped}");
                executed.push(shown.clone());
                visible.push(shown);
                continue;
            }
            executed.push(format!("{space}{}", tail.strip_prefix(' ').unwrap_or(tail)));
            hidden_lines.push(index as u32 + 1);
        } else {
            executed.push(line.to_owned());
            visible.push(line.to_owned());
        }
    }
    Split {
        executed: join(executed, source),
        visible: join(visible, source),
        hidden_lines,
    }
}

/// The source a runner executes: `CheckInput::Code` may already carry the
/// split, in which case the prefixes are gone and `hidden_lines` records what
/// was hidden. A block that reaches a runner unsplit is split here.
pub fn executed(input: &CheckInput, prefix: &str) -> Option<String> {
    match input {
        CheckInput::Code {
            lang,
            source,
            hidden_lines,
        } => Some(if hidden_lines.is_empty() {
            split(lang, source, prefix).executed
        } else {
            source.clone()
        }),
        CheckInput::Schema { lang, source, .. } => Some(split(lang, source, prefix).executed),
        _ => None,
    }
}

/// `lines()` drops a trailing newline; a block that had one keeps it, because
/// a shell heredoc and a Python file both care.
fn join(lines: Vec<String>, source: &str) -> String {
    let mut out = lines.join("\n");
    if source.ends_with('\n') && !out.is_empty() {
        out.push('\n');
    }
    out
}

#[cfg(test)]
mod tests;
