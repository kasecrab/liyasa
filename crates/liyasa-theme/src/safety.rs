//! The theme's last line of defence over rendered content (CMP-102).
//!
//! The sanitizer (§7.5.1 item 3) is what decides which HTML survives, and the
//! CSP is the layer behind it. This is the third: whatever reaches a template
//! as page content has its scripts removed and its inline handlers dropped
//! before the theme marks it safe, so a gap in either of the first two layers
//! does not become an executed script.

use liyasa_core::diagnostics::{Diagnostic, Diagnostics, code};

/// Content with every script element, inline event handler, and `javascript:`
/// URL removed, plus one diagnostic per removal.
pub fn strip_scripts(html: &str) -> (String, Diagnostics) {
    let mut diagnostics = Diagnostics::new();
    let mut out = String::with_capacity(html.len());
    let mut rest = html;

    while let Some(at) = find_element_start(rest, "script") {
        out.push_str(&rest[..at]);
        diagnostics.push(removed("a `<script>` element"));
        rest = skip_element(&rest[at..], "script").unwrap_or_default();
    }
    out.push_str(rest);

    let (out, handlers) = strip_handlers(&out);
    for _ in 0..handlers {
        diagnostics.push(removed("an inline event handler"));
    }

    let (out, urls) = strip_javascript_urls(&out);
    for _ in 0..urls {
        diagnostics.push(removed("a `javascript:` URL"));
    }

    (out, diagnostics)
}

fn removed(what: &str) -> Diagnostic {
    Diagnostic::new(
        code::E0304,
        format!("{what} was removed from rendered content; the theme never emits one"),
    )
    .help("inline scripts in content are never executed (CMP-102)")
}

fn find_element_start(html: &str, name: &str) -> Option<usize> {
    let lower = html.to_ascii_lowercase();
    let mut from = 0;
    while let Some(at) = lower[from..].find(&format!("<{name}")) {
        let at = from + at;
        let after = lower[at + name.len() + 1..].chars().next();
        if matches!(after, Some(c) if c.is_whitespace() || c == '>' || c == '/') || after.is_none()
        {
            return Some(at);
        }
        from = at + 1;
    }
    None
}

/// Everything after the element's closing tag, or `None` when it never closes,
/// in which case the rest of the document is dropped with it.
fn skip_element<'a>(html: &'a str, name: &str) -> Option<&'a str> {
    let lower = html.to_ascii_lowercase();
    let close = format!("</{name}");
    let at = lower.find(&close)?;
    let end = html[at..].find('>')? + at + 1;
    Some(&html[end..])
}

/// Drops every `on…="…"` attribute.
///
/// The rule is the name's prefix, not a list of known handlers: the list grows
/// with every HTML release, and an attribute this drops by mistake costs a
/// styling hook, while one it keeps by mistake costs a reader.
fn strip_handlers(html: &str) -> (String, usize) {
    let mut out = String::with_capacity(html.len());
    let mut removed = 0;
    let mut rest = html;

    while let Some(start) = rest.find('<') {
        out.push_str(&rest[..start]);
        let Some(end) = rest[start..].find('>') else {
            out.push_str(&rest[start..]);
            return (out, removed);
        };
        let tag = &rest[start..start + end + 1];
        let (cleaned, count) = strip_handlers_in_tag(tag);
        removed += count;
        out.push_str(&cleaned);
        rest = &rest[start + end + 1..];
    }
    out.push_str(rest);
    (out, removed)
}

fn strip_handlers_in_tag(tag: &str) -> (String, usize) {
    let lower = tag.to_ascii_lowercase();
    if !lower.contains(" on") {
        return (tag.to_owned(), 0);
    }
    let mut out = String::with_capacity(tag.len());
    let mut removed = 0;
    let bytes: Vec<char> = tag.chars().collect();
    let mut at = 0;
    while at < bytes.len() {
        let is_handler = bytes[at].is_whitespace()
            && bytes
                .get(at + 1..at + 3)
                .is_some_and(|pair| pair.iter().collect::<String>().eq_ignore_ascii_case("on"));
        if !is_handler {
            out.push(bytes[at]);
            at += 1;
            continue;
        }
        // Skip the attribute: name, `=`, and a quoted or bare value.
        let mut cursor = at + 1;
        while cursor < bytes.len()
            && bytes[cursor] != '='
            && !bytes[cursor].is_whitespace()
            && bytes[cursor] != '>'
        {
            cursor += 1;
        }
        if cursor < bytes.len() && bytes[cursor] == '=' {
            cursor += 1;
            match bytes.get(cursor) {
                Some('"') | Some('\'') => {
                    let quote = bytes[cursor];
                    cursor += 1;
                    while cursor < bytes.len() && bytes[cursor] != quote {
                        cursor += 1;
                    }
                    cursor += 1;
                }
                _ => {
                    while cursor < bytes.len()
                        && !bytes[cursor].is_whitespace()
                        && bytes[cursor] != '>'
                    {
                        cursor += 1;
                    }
                }
            }
            removed += 1;
        }
        at = cursor;
    }
    (out, removed)
}

fn strip_javascript_urls(html: &str) -> (String, usize) {
    let mut out = html.to_owned();
    let mut removed = 0;
    loop {
        let lower = out.to_ascii_lowercase();
        let Some(at) = ["=\"javascript:", "='javascript:", "=javascript:"]
            .iter()
            .filter_map(|pattern| lower.find(pattern).map(|at| (at, pattern.len())))
            .min_by_key(|(at, _)| *at)
        else {
            return (out, removed);
        };
        let (start, pattern_len) = at;
        let value_start = start + pattern_len - "javascript:".len();
        let end = out[value_start..]
            .find(['"', '\'', ' ', '>'])
            .map_or(out.len(), |offset| value_start + offset);
        out.replace_range(value_start..end, "#");
        removed += 1;
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_script_element_and_its_body_are_removed() {
        let (html, diagnostics) =
            strip_scripts("<p>before</p><script>alert(1)</script><p>after</p>");
        assert_eq!(html, "<p>before</p><p>after</p>");
        assert_eq!(diagnostics.len(), 1);
        assert_eq!(diagnostics.as_slice()[0].code.as_str(), "E0304");
    }

    #[test]
    fn a_script_with_attributes_or_odd_case_is_still_removed() {
        let (html, _) = strip_scripts("<SCRIPT type=\"module\" src=\"x.js\"></SCRIPT>ok");
        assert_eq!(html, "ok");
        let (html, _) = strip_scripts("<script\n>x</script>ok");
        assert_eq!(html, "ok");
    }

    #[test]
    fn an_unclosed_script_takes_the_rest_of_the_content_with_it() {
        let (html, diagnostics) = strip_scripts("<p>ok</p><script>never closed");
        assert_eq!(html, "<p>ok</p>");
        assert!(!diagnostics.is_empty());
    }

    #[test]
    fn inline_handlers_are_dropped_and_the_element_kept() {
        let (html, diagnostics) =
            strip_scripts("<img src=\"a.png\" onerror=\"steal()\" alt=\"a\">");
        assert_eq!(html, "<img src=\"a.png\" alt=\"a\">");
        assert_eq!(diagnostics.len(), 1);
    }

    #[test]
    fn a_javascript_url_becomes_a_dead_link() {
        let (html, diagnostics) = strip_scripts("<a href=\"javascript:alert(1)\">x</a>");
        assert_eq!(html, "<a href=\"#\">x</a>");
        assert_eq!(diagnostics.len(), 1);
    }

    #[test]
    fn ordinary_content_is_left_exactly_as_it_was() {
        let content = "<h2 id=\"one\">One</h2>\n<p>A <code>script</code> tag is written <em>like this</em>: &lt;script&gt;.</p>";
        let (html, diagnostics) = strip_scripts(content);
        assert_eq!(html, content);
        assert!(diagnostics.is_empty());
    }

    #[test]
    fn the_prefix_is_the_rule_and_it_errs_toward_removal() {
        // `data-once` is kept: the name does not begin with `on`.
        let kept = "<div data-once=\"1\" data-only=\"2\">x</div>";
        let (html, diagnostics) = strip_scripts(kept);
        assert_eq!(html, kept);
        assert!(diagnostics.is_empty());

        // `only` is removed although it is not an event handler, because the
        // alternative is a list that goes stale as HTML grows.
        let (html, diagnostics) = strip_scripts("<div only=\"2\">x</div>");
        assert_eq!(html, "<div>x</div>");
        assert_eq!(diagnostics.len(), 1);
    }
}
