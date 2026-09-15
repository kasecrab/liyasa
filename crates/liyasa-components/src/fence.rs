//! Fenced code blocks: the info string and what it renders to (CMP-30, CMP-60).
//!
//! A fence carries more than a language. `title`, `{1,3-5}`, `focus={2}`,
//! `diff`, `lines`, `start=10`, `wrap`, `expandable`, `maxLines`, `copy=false`,
//! `prompt`, `template`, `verify`, `filename`, and `icon` all live in the info
//! string, are parsed here into [`FenceAttrs`], and are rendered here so the
//! HTML and the Markdown serialization cannot drift.

use liyasa_core::diagnostics::{Diagnostic, Diagnostics, code};
use liyasa_core::document::{FenceAttrs, FenceInfo};

use crate::html::Html;
use crate::md::Markdown;

/// Attributes that take a value.
const KV: &[&str] = &[
    "title",
    "start",
    "maxLines",
    "prompt",
    "template",
    "verify",
    "filename",
    "icon",
    "focus",
    "highlight",
    "lang",
    "layout",
    "theme",
    "sync",
    "copy",
    "wrap",
    "expandable",
    "id",
];

/// Attributes written bare.
const FLAGS: &[&str] = &[
    "diff",
    "lines",
    "wrap",
    "expandable",
    "twoslash",
    "zoom",
    "fullscreen",
    "pan",
    "nocopy",
];

/// Splits ` ```rust title="main.rs" {1,3-5} ` into a language and attributes.
///
/// Unknown attributes are kept in [`FenceAttrs`] and reported as `W0302`, so a
/// theme may read one Liyasa does not know about.
pub fn parse_info(info: &str) -> (FenceInfo, Diagnostics) {
    let mut out = FenceInfo::default();
    let mut diagnostics = Diagnostics::new();
    let tokens = tokenize(info);
    for (at, token) in tokens.iter().enumerate() {
        match token {
            Token::Braced(range) => {
                let ranges = parse_ranges(range);
                if ranges.is_empty() {
                    diagnostics.push(Diagnostic::new(
                        code::W0302,
                        format!("`{{{range}}}` is not a line range"),
                    ));
                } else {
                    out.attrs.highlight.extend(ranges);
                }
            }
            Token::Pair(key, value) => {
                if key == "highlight" {
                    out.attrs.highlight.extend(parse_ranges(value));
                    continue;
                }
                if !KV.contains(&key.as_str()) {
                    diagnostics.push(Diagnostic::new(
                        code::W0302,
                        format!("unknown code fence attribute `{key}`"),
                    ));
                }
                out.attrs.kv.insert(key.clone(), value.clone());
            }
            Token::Bare(word) => {
                if at == 0 && !FLAGS.contains(&word.as_str()) {
                    out.lang = Some(word.clone());
                    continue;
                }
                if !FLAGS.contains(&word.as_str()) {
                    diagnostics.push(Diagnostic::new(
                        code::W0302,
                        format!("unknown code fence attribute `{word}`"),
                    ));
                }
                out.attrs.flags.insert(word.clone());
            }
        }
    }
    out.attrs.highlight.sort_unstable();
    out.attrs.highlight.dedup();
    (out, diagnostics)
}

enum Token {
    Bare(String),
    Pair(String, String),
    /// The `1,3-5` inside `{1,3-5}` or `focus={2}`.
    Braced(String),
}

fn tokenize(info: &str) -> Vec<Token> {
    let mut tokens = Vec::new();
    let mut chars = info.chars().peekable();
    while let Some(ch) = chars.next() {
        if ch.is_whitespace() {
            continue;
        }
        if ch == '{' {
            let braced: String = take_until(&mut chars, '}');
            tokens.push(Token::Braced(braced));
            continue;
        }
        let mut word = String::from(ch);
        while let Some(&next) = chars.peek() {
            if next.is_whitespace() || next == '=' {
                break;
            }
            word.push(next);
            chars.next();
        }
        if chars.peek() == Some(&'=') {
            chars.next();
            let value = match chars.peek() {
                Some('"') => {
                    chars.next();
                    take_until(&mut chars, '"')
                }
                Some('\'') => {
                    chars.next();
                    take_until(&mut chars, '\'')
                }
                Some('{') => {
                    chars.next();
                    let braced = take_until(&mut chars, '}');
                    tokens.push(Token::Pair(word, braced));
                    continue;
                }
                _ => {
                    let mut value = String::new();
                    while let Some(&next) = chars.peek() {
                        if next.is_whitespace() {
                            break;
                        }
                        value.push(next);
                        chars.next();
                    }
                    value
                }
            };
            tokens.push(Token::Pair(word, value));
        } else {
            tokens.push(Token::Bare(word));
        }
    }
    tokens
}

fn take_until(chars: &mut std::iter::Peekable<std::str::Chars<'_>>, end: char) -> String {
    let mut out = String::new();
    for ch in chars.by_ref() {
        if ch == end {
            break;
        }
        out.push(ch);
    }
    out
}

/// `1,3-5` to inclusive 1-based ranges.
pub fn parse_ranges(text: &str) -> Vec<(u32, u32)> {
    let mut out = Vec::new();
    for part in text.split(',') {
        let part = part.trim();
        if part.is_empty() {
            continue;
        }
        let range = match part.split_once('-') {
            Some((first, last)) => first
                .trim()
                .parse::<u32>()
                .ok()
                .zip(last.trim().parse::<u32>().ok()),
            None => part.parse::<u32>().ok().map(|n| (n, n)),
        };
        if let Some((first, last)) = range
            && first <= last
            && first > 0
        {
            out.push((first, last));
        }
    }
    out
}

/// What a fence asks for, read once so the renderer does no string matching.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct CodeOptions {
    pub title: Option<String>,
    pub filename: Option<String>,
    pub icon: Option<String>,
    pub prompt: Option<String>,
    pub start: u32,
    pub max_lines: Option<u32>,
    pub diff: bool,
    pub numbers: bool,
    pub wrap: bool,
    pub expandable: bool,
    pub copy: bool,
    pub highlight: Vec<(u32, u32)>,
    pub focus: Vec<(u32, u32)>,
}

impl CodeOptions {
    pub fn read(attrs: &FenceAttrs) -> Self {
        let kv = |key: &str| attrs.kv.get(key).cloned();
        let flag = |key: &str| {
            attrs.flags.contains(key) || attrs.kv.get(key).is_some_and(|v| v != "false" && v != "0")
        };
        Self {
            // A fence with a filename and no title shows the filename.
            title: kv("title").or_else(|| kv("filename")),
            filename: kv("filename"),
            icon: kv("icon"),
            prompt: kv("prompt"),
            start: kv("start").and_then(|v| v.parse().ok()).unwrap_or(1),
            max_lines: kv("maxLines").and_then(|v| v.parse().ok()),
            diff: attrs.flags.contains("diff"),
            numbers: flag("lines"),
            wrap: flag("wrap"),
            expandable: flag("expandable"),
            copy: !attrs.flags.contains("nocopy")
                && attrs.kv.get("copy").is_none_or(|v| v != "false"),
            highlight: attrs.highlight.clone(),
            focus: kv("focus").map(|v| parse_ranges(&v)).unwrap_or_default(),
        }
    }

    fn line_features(&self) -> bool {
        self.diff || self.numbers || !self.highlight.is_empty() || !self.focus.is_empty()
    }
}

fn in_ranges(ranges: &[(u32, u32)], line: u32) -> bool {
    ranges
        .iter()
        .any(|(first, last)| line >= *first && line <= *last)
}

/// Renders a fence to HTML.
///
/// `highlighted` is the syntax-highlighted body when the build produced one.
/// It is split on newlines for the line features, which holds because every
/// highlighter Liyasa uses closes its spans at each line break.
pub fn render_html(
    out: &mut Html,
    lang: Option<&str>,
    body: &str,
    options: &CodeOptions,
    highlighted: Option<&str>,
) {
    out.open("div")
        .attr("class", "ly-code")
        .attr("data-liyasa", "code-block")
        .attr_if("data-lang", lang)
        .attr_if("data-prompt", options.prompt.as_deref())
        .flag_if("data-wrap", options.wrap)
        .flag_if("data-diff", options.diff)
        .flag_if("data-expandable", options.expandable)
        .attr_if(
            "data-max-lines",
            options.max_lines.map(|n| n.to_string()).as_deref(),
        );

    if let Some(title) = &options.title {
        out.open("div").attr("class", "ly-code-title");
        if let Some(icon) = &options.icon {
            out.open("span")
                .attr("class", "ly-icon")
                .attr("data-icon", icon)
                .attr("aria-hidden", "true")
                .close();
        }
        out.text(title).close();
    }

    if options.copy {
        out.open("button")
            .attr("class", "ly-code-copy")
            .attr("type", "button")
            .attr("data-liyasa", "copy")
            .text("Copy")
            .close();
    }

    out.open("pre").attr("class", "ly-code-body");
    out.open("code")
        .attr_if("class", lang.map(|l| format!("language-{l}")).as_deref());

    if options.line_features() {
        render_lines(out, body, options, highlighted);
    } else if let Some(markup) = highlighted {
        out.raw(markup);
    } else {
        out.text(body);
    }
    out.close().close().close();
}

fn render_lines(out: &mut Html, body: &str, options: &CodeOptions, highlighted: Option<&str>) {
    let plain: Vec<&str> = body.split('\n').collect();
    let marked: Vec<&str> = highlighted
        .map(|h| h.split('\n').collect())
        .unwrap_or_default();
    let mut number = options.start;
    for (at, line) in plain.iter().enumerate() {
        // A trailing newline is not a line.
        if at + 1 == plain.len() && line.is_empty() {
            break;
        }
        let ordinal = (at as u32) + 1;
        let (change, text) = match options.diff {
            true => match line.as_bytes().first() {
                Some(b'+') => ("add", &line[1..]),
                Some(b'-') => ("del", &line[1..]),
                _ => ("", line.strip_prefix(' ').unwrap_or(line)),
            },
            false => ("", *line),
        };
        let mut classes = String::from("ly-line");
        if in_ranges(&options.highlight, ordinal) {
            classes.push_str(" ly-line-highlight");
        }
        if in_ranges(&options.focus, ordinal) {
            classes.push_str(" ly-line-focus");
        }
        if !change.is_empty() {
            classes.push_str(" ly-line-");
            classes.push_str(change);
        }
        out.open("span").attr("class", &classes);
        if options.numbers {
            out.attr("data-line", &number.to_string());
        }
        // A diff line keeps its marker out of the copied text.
        if !change.is_empty() {
            out.attr("data-change", change);
        }
        match marked.get(at) {
            Some(markup) if !options.diff => out.raw(markup),
            _ => out.text(text),
        };
        out.close().text("\n");
        // A removed line does not advance the new file's numbering.
        if change != "del" {
            number += 1;
        }
    }
}

/// Serializes a fence for the agent output: the source, unchanged, with the
/// title carried as a comment so the information is not lost (RX-61).
pub fn render_markdown(out: &mut Markdown, lang: Option<&str>, body: &str, options: &CodeOptions) {
    let mut info = String::from(lang.unwrap_or(""));
    if let Some(title) = &options.title {
        info.push_str(&format!(" title=\"{}\"", title.replace('"', "'")));
    }
    out.fence(&info, body);
}

#[cfg(test)]
mod tests {
    use super::*;

    fn info(text: &str) -> FenceInfo {
        parse_info(text).0
    }

    #[test]
    fn the_first_bare_word_is_the_language() {
        assert_eq!(info("rust").lang.as_deref(), Some("rust"));
        assert_eq!(info("  rust  ").lang.as_deref(), Some("rust"));
        assert_eq!(info("").lang, None);
    }

    #[test]
    fn a_leading_flag_is_not_a_language() {
        let parsed = info("diff");
        assert_eq!(parsed.lang, None);
        assert!(parsed.attrs.flags.contains("diff"));
    }

    #[test]
    fn a_brace_group_is_a_highlight_range() {
        assert_eq!(info("rust {1,3-5}").attrs.highlight, vec![(1, 1), (3, 5)]);
    }

    #[test]
    fn focus_takes_its_range_in_braces() {
        let options = CodeOptions::read(&info("rust focus={2-4}").attrs);
        assert_eq!(options.focus, vec![(2, 4)]);
    }

    #[test]
    fn a_quoted_value_keeps_its_spaces() {
        assert_eq!(
            info(r#"sh prompt="$ ""#)
                .attrs
                .kv
                .get("prompt")
                .map(String::as_str),
            Some("$ ")
        );
    }

    #[test]
    fn an_unknown_attribute_warns_but_is_kept() {
        let (parsed, diagnostics) = parse_info("rust wat=1");
        assert_eq!(parsed.attrs.kv.get("wat").map(String::as_str), Some("1"));
        assert_eq!(diagnostics.len(), 1);
        assert_eq!(
            diagnostics.iter().next().map(|d| d.code.as_str()),
            Some("W0302")
        );
    }

    #[test]
    fn copy_is_on_unless_turned_off() {
        assert!(CodeOptions::read(&info("rust").attrs).copy);
        assert!(!CodeOptions::read(&info("rust copy=false").attrs).copy);
    }

    #[test]
    fn a_reversed_range_is_rejected() {
        assert_eq!(parse_ranges("5-1"), Vec::new());
        assert_eq!(parse_ranges("0"), Vec::new());
        assert_eq!(parse_ranges("2-4, 7"), vec![(2, 4), (7, 7)]);
    }

    #[test]
    fn diff_markers_leave_the_text() {
        let options = CodeOptions::read(&info("diff").attrs);
        let mut html = Html::new();
        render_lines(&mut html, "-old\n+new\n", &options, None);
        let markup = html.finish();
        assert!(
            markup.contains(r#"class="ly-line ly-line-del""#),
            "{markup}"
        );
        assert!(markup.contains(">old</span>"), "{markup}");
        assert!(markup.contains(">new</span>"), "{markup}");
    }

    #[test]
    fn line_numbers_skip_removed_lines() {
        let options = CodeOptions::read(&info("diff lines start=10").attrs);
        let mut html = Html::new();
        render_lines(&mut html, "-old\n+new\n kept\n", &options, None);
        let markup = html.finish();
        assert!(markup.contains(r#"data-line="10""#), "{markup}");
        assert!(markup.contains(r#"data-line="11""#), "{markup}");
        assert!(!markup.contains(r#"data-line="12""#), "{markup}");
    }

    #[test]
    fn a_title_survives_the_agent_serialization() {
        let options = CodeOptions::read(&info(r#"rust title="main.rs""#).attrs);
        let mut md = Markdown::new();
        render_markdown(&mut md, Some("rust"), "fn main() {}\n", &options);
        assert_eq!(
            md.finish(),
            "```rust title=\"main.rs\"\nfn main() {}\n```\n"
        );
    }
}
