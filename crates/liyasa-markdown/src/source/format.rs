//! The canonical form of a page (CM-05, CLI-05).
//!
//! The formatter works on the source alone and never sees expanded text, which
//! is what lets it be idempotent: running it twice changes nothing, so `--check`
//! is a reliable CI gate.
//!
//! What it normalizes:
//!
//! - the byte order mark and line endings (CM-05, through [`normalize`]);
//! - tabs, expanded to spaces everywhere except inside code, where a tab is
//!   content (CM-05);
//! - trailing whitespace, except the two spaces that make a hard break;
//! - runs of blank lines, collapsed to one;
//! - the file's final newline;
//! - spacing around block statements (CM-21).
//!
//! What it deliberately leaves alone: the bytes of any code block, and the
//! YAML inside front matter beyond its trailing whitespace. Re-serializing YAML
//! would drop the author's comments and key order, which is a bigger change
//! than a formatter is allowed to make.

use liyasa_core::diagnostics::Diagnostics;
use liyasa_core::document::Segment;
use liyasa_core::span::SourceId;

use super::lines::{self, TAB_STOP};
use super::{normalize, scan};

/// Diagnostics that make the bytes unsafe to rewrite: with an unclosed fence or
/// unparsable front matter the segmentation is a guess, and a formatter may not
/// guess.
const FATAL: &[&str] = &["E0101", "E0102", "E0301"];

#[derive(Debug, Clone, Copy, Default)]
pub struct FormatOptions {
    /// `--directives`: rewrite the tag form to the directive form (CLI-05).
    pub directives: bool,
}

/// The canonical form of `source`.
pub fn format(source: &str) -> Result<String, Diagnostics> {
    format_with(source, &FormatOptions::default())
}

pub fn format_with(source: &str, options: &FormatOptions) -> Result<String, Diagnostics> {
    let normalized = normalize(source).into_owned();
    let text = if options.directives {
        tag_form(&normalized)?
    } else {
        normalized
    };

    let (document, diagnostics) = scan::scan(&text, SourceId(0));
    if diagnostics
        .iter()
        .any(|d| d.is_error() && FATAL.contains(&d.code.as_str()))
    {
        return Err(diagnostics
            .into_iter()
            .filter(|d| FATAL.contains(&d.code.as_str()))
            .collect());
    }

    let verbatim: Vec<(u32, u32)> = document
        .segments
        .iter()
        .filter(|segment| matches!(segment, Segment::Code { .. }))
        .map(|segment| (segment.span().start, segment.span().end))
        .collect();
    let front_end = scan::body_start(&document);

    let mut out = String::with_capacity(text.len());
    let mut blank_run = 0usize;
    let all = lines::split(&text, 0);
    for (at, line) in all.iter().enumerate() {
        let raw = &text[line.start..line.end];
        let inside_code = verbatim
            .iter()
            .any(|(start, end)| line.start >= *start as usize && line.start < *end as usize);
        if inside_code {
            blank_run = 0;
            out.push_str(raw);
            out.push('\n');
            continue;
        }

        if raw.trim().is_empty() {
            blank_run += 1;
            if blank_run == 1 && !out.is_empty() {
                out.push('\n');
            }
            continue;
        }
        blank_run = 0;

        let expanded = expand_tabs(raw);
        let trimmed = expanded.trim_end();
        out.push_str(trimmed);
        // Two trailing spaces are a hard break, and only when a line follows.
        // A block statement or a directive line is structure, not prose, so
        // its trailing space is spacing to normalize (CM-21).
        if line.start >= front_end as usize
            && !is_structural(trimmed)
            && expanded.len() >= trimmed.len() + 2
            && all
                .get(at + 1)
                .is_some_and(|next| !text[next.start..next.end].trim().is_empty())
        {
            out.push_str("  ");
        }
        out.push('\n');
    }

    while out.ends_with("\n\n") {
        out.pop();
    }
    Ok(out)
}

/// Whether `source` is already canonical, which is what `--check` reports.
pub fn is_formatted(source: &str) -> bool {
    format(source).is_ok_and(|formatted| formatted == source)
}

/// A line that is nothing but one block statement or one directive.
fn is_structural(trimmed: &str) -> bool {
    let trimmed = trimmed.trim_start();
    trimmed.starts_with("::")
        || (trimmed.starts_with("{%") && trimmed.ends_with("%}"))
        || (trimmed.starts_with("{#") && trimmed.ends_with("#}"))
}

fn expand_tabs(line: &str) -> String {
    if !line.contains('\t') {
        return line.to_owned();
    }
    let mut out = String::with_capacity(line.len() + TAB_STOP);
    let mut column = 0usize;
    for ch in line.chars() {
        if ch == '\t' {
            let width = TAB_STOP - column % TAB_STOP;
            out.extend(std::iter::repeat_n(' ', width));
            column += width;
        } else {
            out.push(ch);
            column += 1;
        }
    }
    out
}

// ---- tag form (CLI-05 `--directives`, CM-53) ----

/// Rewrites `<Card …>` … `</Card>` to `:::card{…}` … `:::`.
///
/// Nesting decides the fence length, so the pass matches every pair first and
/// then gives each container three colons plus the depth of what it holds.
/// `plan/rfcs/0022-tag-form-directive-names.md` records how a name is spelled.
fn tag_form(text: &str) -> Result<String, Diagnostics> {
    let (document, diagnostics) = scan::scan(text, SourceId(0));
    if diagnostics
        .iter()
        .any(|d| d.is_error() && FATAL.contains(&d.code.as_str()))
    {
        return Err(diagnostics);
    }
    let verbatim: Vec<(u32, u32)> = document
        .segments
        .iter()
        .filter(|segment| matches!(segment, Segment::Code { .. }))
        .map(|segment| (segment.span().start, segment.span().end))
        .collect();

    let all = lines::split(text, scan::body_start(&document) as usize);
    let mut tags: Vec<Option<Tag>> = Vec::with_capacity(all.len());
    for line in &all {
        let inside_code = verbatim
            .iter()
            .any(|(start, end)| line.start >= *start as usize && line.start < *end as usize);
        tags.push(if inside_code {
            None
        } else {
            parse_tag(line.content)
        });
    }

    // Match opens with closes and give each container its fence length.
    let mut colons = vec![3usize; tags.len()];
    let mut stack: Vec<(usize, String)> = Vec::new();
    let mut depth = vec![0usize; tags.len()];
    for (at, tag) in tags.iter().enumerate() {
        match tag {
            Some(Tag::Open { name, .. }) => stack.push((at, name.clone())),
            Some(Tag::Close { name }) => {
                if let Some(found) = stack.iter().rposition(|(_, open)| open == name) {
                    let (open_at, _) = stack.remove(found);
                    let inner = depth[open_at];
                    colons[open_at] = 3 + inner;
                    colons[at] = 3 + inner;
                    for (parent, _) in &stack {
                        depth[*parent] = depth[*parent].max(inner + 1);
                    }
                }
            }
            _ => {}
        }
    }

    let mut out = String::with_capacity(text.len());
    out.push_str(&text[..scan::body_start(&document) as usize]);
    for (at, line) in all.iter().enumerate() {
        let lead = &text[line.start..line.content_at];
        match &tags[at] {
            Some(Tag::Open { name, props }) => {
                let fence = ":".repeat(colons[at]);
                out.push_str(&format!("{lead}{fence}{name}{props}\n"));
            }
            Some(Tag::Close { .. }) => {
                out.push_str(&format!("{lead}{}\n", ":".repeat(colons[at])));
            }
            Some(Tag::Leaf { name, props }) => {
                out.push_str(&format!("{lead}::{name}{props}\n"));
            }
            None => {
                out.push_str(&text[line.start..line.end]);
                out.push('\n');
            }
        }
    }
    while out.ends_with("\n\n") {
        out.pop();
    }
    Ok(out)
}

enum Tag {
    Open { name: String, props: String },
    Close { name: String },
    Leaf { name: String, props: String },
}

fn parse_tag(content: &str) -> Option<Tag> {
    let rest = content.strip_prefix('<')?;
    if let Some(rest) = rest.strip_prefix('/') {
        return Some(Tag::Close {
            name: directive_name(rest.strip_suffix('>')?)?,
        });
    }
    let body = rest.strip_suffix('>')?;
    let (body, leaf) = match body.strip_suffix('/') {
        Some(body) => (body, true),
        None => (body, false),
    };
    let name_len = body
        .find(|c: char| !c.is_ascii_alphanumeric())
        .unwrap_or(body.len());
    let name = directive_name(&body[..name_len])?;
    let props = attributes(body[name_len..].trim());
    if leaf {
        Some(Tag::Leaf { name, props })
    } else {
        Some(Tag::Open { name, props })
    }
}

/// `CodeGroup` becomes `code-group`; a lowercase tag is HTML and is left alone
/// (CM-53).
fn directive_name(tag: &str) -> Option<String> {
    let mut chars = tag.chars();
    if !chars.next()?.is_ascii_uppercase() {
        return None;
    }
    let mut out = String::with_capacity(tag.len() + 2);
    for (at, ch) in tag.char_indices() {
        if ch.is_ascii_uppercase() && at > 0 {
            out.push('-');
        }
        out.extend(ch.to_lowercase());
    }
    Some(out)
}

/// JSX attributes in the directive prop grammar: `a="x"` stays, `a={expr}`
/// becomes `a={{ expr }}`, and a bare `a` becomes `a=true` (CM-51).
fn attributes(text: &str) -> String {
    let mut props: Vec<String> = Vec::new();
    let bytes = text.as_bytes();
    let mut at = 0usize;
    while at < bytes.len() {
        if bytes[at].is_ascii_whitespace() {
            at += 1;
            continue;
        }
        let key_end = at
            + text[at..]
                .find(|c: char| c == '=' || c.is_whitespace())
                .unwrap_or(text.len() - at);
        let key = &text[at..key_end];
        at = key_end;
        if !text[at..].starts_with('=') {
            props.push(format!("{key}=true"));
            continue;
        }
        at += 1;
        let value = &text[at..];
        if let Some(rest) = value.strip_prefix('"') {
            let end = rest.find('"').unwrap_or(rest.len());
            props.push(format!("{key}=\"{}\"", &rest[..end]));
            at += end + 2;
        } else if value.starts_with("{{") {
            let end = value.find("}}").map_or(value.len(), |end| end + 2);
            props.push(format!("{key}={}", value[..end].trim()));
            at += end;
        } else if let Some(rest) = value.strip_prefix('{') {
            let end = rest.find('}').unwrap_or(rest.len());
            props.push(format!("{key}={{{{ {} }}}}", rest[..end].trim()));
            at += end + 2;
        } else {
            let end = value.find(char::is_whitespace).unwrap_or(value.len());
            props.push(format!("{key}={}", &value[..end]));
            at += end;
        }
    }
    if props.is_empty() {
        return String::new();
    }
    format!("{{{}}}", props.join(" "))
}

#[cfg(test)]
mod tests {
    use liyasa_core::diagnostics::code;

    use super::*;

    fn formatted(text: &str) -> String {
        format(text).expect("a formattable page")
    }

    fn assert_idempotent(text: &str) {
        let once = formatted(text);
        assert_eq!(formatted(&once), once, "formatting {text:?} is not stable");
    }

    #[test]
    fn line_endings_and_the_byte_order_mark_are_normalized() {
        assert_eq!(formatted("\u{feff}a\r\nb\r\n"), "a\nb\n");
    }

    #[test]
    fn tabs_become_spaces_outside_code() {
        assert_eq!(formatted("a\tb\n"), "a   b\n");
        // A lazy continuation is prose, not an indented code block.
        assert_eq!(
            formatted("para\n\tcontinuation\n"),
            "para\n    continuation\n"
        );
    }

    #[test]
    fn a_leading_tab_after_a_blank_line_is_code_and_keeps_its_tab() {
        let text = "para\n\n\tcode\n";
        assert_eq!(formatted(text), text);
    }

    #[test]
    fn tabs_survive_inside_code() {
        let text = "```\n\tindented\tby tabs\n```\n";
        assert_eq!(formatted(text), text);
    }

    #[test]
    fn trailing_whitespace_goes_but_a_hard_break_stays() {
        assert_eq!(formatted("a   \n"), "a\n");
        assert_eq!(formatted("a  \nb\n"), "a  \nb\n");
        assert_eq!(formatted("a \nb\n"), "a\nb\n");
    }

    #[test]
    fn blank_runs_collapse_and_the_file_ends_with_one_newline() {
        assert_eq!(formatted("a\n\n\n\nb\n\n\n"), "a\n\nb\n");
        assert_eq!(formatted("a"), "a\n");
        assert_eq!(formatted(""), "");
    }

    #[test]
    fn leading_blank_lines_go() {
        assert_eq!(formatted("\n\na\n"), "a\n");
    }

    #[test]
    fn blank_lines_inside_a_fence_are_kept() {
        let text = "```\na\n\n\n\nb\n```\n";
        assert_eq!(formatted(text), text);
    }

    #[test]
    fn front_matter_keeps_its_keys_and_order() {
        let text = "---\nzebra: 1\nalpha: 2\n---\n\nbody\n";
        assert_eq!(formatted(text), text);
    }

    #[test]
    fn a_block_statement_loses_its_trailing_space() {
        assert_eq!(
            formatted("{% for row in rows %}   \n- {{ row }}\n{% endfor %}\n"),
            "{% for row in rows %}\n- {{ row }}\n{% endfor %}\n"
        );
    }

    #[test]
    fn an_unclosed_fence_is_refused_rather_than_guessed() {
        let diagnostics = format("```\ncode\n").expect_err("an unclosed fence");
        assert_eq!(
            diagnostics.iter().map(|d| d.code).collect::<Vec<_>>(),
            [code::E0301]
        );
    }

    #[test]
    fn unparsable_front_matter_is_refused() {
        assert!(format("---\ntitle: [unclosed\n---\nbody\n").is_err());
    }

    #[test]
    fn the_check_flag_sees_the_difference() {
        assert!(is_formatted("a\nb\n"));
        assert!(!is_formatted("a\t\nb\n"));
        assert!(!is_formatted("a\r\nb\r\n"));
    }

    #[test]
    fn formatting_is_idempotent() {
        for text in [
            "",
            "\u{feff}# Title\r\n\r\n\r\nbody   \n\ttabbed\n",
            "---\ntitle: A\n---\n\n\nbody\n",
            "```bash\n\tcode\t\n```\n\n\n",
            ":::note\nbody  \n:::\n",
            "{% for row in rows %}\n- {{ row }}\n{% endfor %}\n\n",
            "| a | b |\n|---|---|\n| 1 | 2 |\n",
            "- item\n  - inner\n\n\n- next\n",
            "a  \nb\n",
            "text with `{{ x }}` in a span\n",
        ] {
            assert_idempotent(text);
        }
    }

    // ---- --directives ----

    fn converted(text: &str) -> String {
        format_with(text, &FormatOptions { directives: true }).expect("a convertible page")
    }

    #[test]
    fn a_block_tag_becomes_a_container_directive() {
        assert_eq!(
            converted("<Card title=\"Install\">\nbody\n</Card>\n"),
            ":::card{title=\"Install\"}\nbody\n:::\n"
        );
    }

    #[test]
    fn a_self_closing_tag_becomes_a_leaf_directive() {
        assert_eq!(
            converted("<Image src=\"/a.png\" alt=\"A\" />\n"),
            "::image{src=\"/a.png\" alt=\"A\"}\n"
        );
    }

    #[test]
    fn a_pascal_case_name_becomes_kebab_case() {
        assert_eq!(
            converted("<CodeGroup>\nx\n</CodeGroup>\n"),
            ":::code-group\nx\n:::\n"
        );
    }

    #[test]
    fn nesting_lengthens_the_outer_fence() {
        assert_eq!(
            converted("<Tabs>\n<Tab title=\"npm\">\nx\n</Tab>\n</Tabs>\n"),
            "::::tabs\n:::tab{title=\"npm\"}\nx\n:::\n::::\n"
        );
    }

    #[test]
    fn a_lowercase_tag_stays_html() {
        let text = "<div class=\"x\">\nbody\n</div>\n";
        assert_eq!(converted(text), text);
    }

    #[test]
    fn a_tag_inside_a_fence_is_left_alone() {
        let text = "```\n<Card title=\"x\">\n```\n";
        assert_eq!(converted(text), text);
    }

    #[test]
    fn a_jsx_expression_attribute_becomes_a_template_expression() {
        assert_eq!(
            converted("<Card href={page.url}>\nx\n</Card>\n"),
            ":::card{href={{ page.url }}}\nx\n:::\n"
        );
    }

    #[test]
    fn a_template_expression_attribute_is_left_as_written() {
        assert_eq!(
            converted("<Card href={{ page.url }}>\nx\n</Card>\n"),
            ":::card{href={{ page.url }}}\nx\n:::\n"
        );
    }

    #[test]
    fn a_bare_attribute_becomes_true() {
        assert_eq!(converted("<Card wide />\n"), "::card{wide=true}\n");
    }

    #[test]
    fn conversion_is_idempotent() {
        for text in [
            "<Card title=\"Install\">\nbody\n</Card>\n",
            "<Tabs>\n<Tab title=\"npm\">\nx\n</Tab>\n</Tabs>\n",
            "<Image src=\"/a.png\" />\n",
        ] {
            let once = converted(text);
            assert_eq!(converted(&once), once, "converting {text:?} is not stable");
            assert_eq!(formatted(&once), once);
        }
    }
}
