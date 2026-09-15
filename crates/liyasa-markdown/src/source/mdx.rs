//! `.mdx` compatibility mode (CM-04).
//!
//! An imported MDX file is translated, not interpreted: the tag form the
//! formatter already understands becomes directive form, MDX comments become
//! template comments, and everything that has no Liyasa equivalent — an
//! `import`, an `export`, a JSX expression container holding JavaScript — is
//! reported rather than silently dropped. A migration that quietly loses a
//! component is worse than one that stops and names it.
//!
//! The caller passes the component names Liyasa knows, because the registry
//! lives in `liyasa-components`; this crate only translates syntax.

use liyasa_core::diagnostics::{Diagnostic, Diagnostics, code};
use liyasa_core::document::Segment;
use liyasa_core::span::{SourceId, Span};

use super::format::{FormatOptions, format_with};
use super::scan;

/// Translates an MDX page into Liyasa Markdown.
///
/// `known` is every component name Liyasa can render, in directive spelling
/// (`card`, `code-group`). A name that is not in it is `E0313` with the closest
/// one as the suggestion.
pub fn from_mdx(source: &str, known: &[&str]) -> (String, Diagnostics) {
    let mut diagnostics = Diagnostics::new();
    let text = comments(source);
    let translated = match format_with(&text, &FormatOptions { directives: true }) {
        Ok(translated) => translated,
        Err(reported) => return (text, reported),
    };

    let (document, _) = scan::scan(&translated, SourceId(0));
    let code_ranges: Vec<(u32, u32)> = document
        .segments
        .iter()
        .filter(|segment| matches!(segment, Segment::Code { .. }))
        .map(|segment| (segment.span().start, segment.span().end))
        .collect();

    for segment in &document.segments {
        let (span, name) = match segment {
            Segment::DirectiveOpen { span, name, .. }
            | Segment::DirectiveLeaf { span, name, .. } => (*span, name),
            _ => continue,
        };
        if known.contains(&name.as_str()) {
            continue;
        }
        let mut diagnostic =
            Diagnostic::new(code::E0313, format!("`{name}` is not a Liyasa component")).at(span);
        if let Some(closest) = closest(name, known) {
            diagnostic = diagnostic.help(format!("did you mean `{closest}`?"));
        }
        diagnostics.push(diagnostic);
    }

    let mut at = 0usize;
    for raw in translated.split_inclusive('\n') {
        let start = at;
        at += raw.len();
        let line = raw.trim();
        let inside_code = code_ranges
            .iter()
            .any(|(from, to)| start >= *from as usize && start < *to as usize);
        if inside_code {
            continue;
        }
        let Some(construct) = unsupported(line) else {
            continue;
        };
        diagnostics.push(
            Diagnostic::new(
                code::E0202,
                format!("MDX `{construct}` has no Liyasa equivalent"),
            )
            .at(Span::new(
                SourceId(0),
                start as u32,
                (start + line.len()) as u32,
            ))
            .help(match construct {
                "import" | "export" => {
                    "move shared content into `snippets/` and include it with `{% snippet %}`"
                }
                _ => "write the expression as `{{ … }}`, which Liyasa expands before parsing",
            }),
        );
    }
    (translated, diagnostics)
}

/// MDX's `{/* … */}` becomes a template comment; both are erased before the
/// Markdown parser sees them.
fn comments(source: &str) -> String {
    let mut out = String::with_capacity(source.len());
    let mut rest = source;
    while let Some(at) = rest.find("{/*") {
        out.push_str(&rest[..at]);
        let body = &rest[at + 3..];
        match body.find("*/}") {
            Some(end) => {
                out.push_str("{#");
                out.push_str(&body[..end]);
                out.push_str("#}");
                rest = &body[end + 3..];
            }
            None => {
                out.push_str(&rest[at..]);
                rest = "";
            }
        }
    }
    out.push_str(rest);
    out
}

/// The MDX constructs that do not translate.
fn unsupported(line: &str) -> Option<&'static str> {
    if line.starts_with("import ") || line == "import" {
        return Some("import");
    }
    if line.starts_with("export ") || line.starts_with("export{") {
        return Some("export");
    }
    // A line that is one JSX expression container: `{items.map(…)}`. A Liyasa
    // `{{ … }}` or `{% … %}` is not one, and neither is `{#`.
    if line.starts_with('{')
        && line.ends_with('}')
        && !line.starts_with("{{")
        && !line.starts_with("{%")
        && !line.starts_with("{#")
        && line.len() > 2
    {
        return Some("expression container");
    }
    None
}

fn closest<'a>(name: &str, known: &[&'a str]) -> Option<&'a str> {
    known
        .iter()
        .map(|candidate| (distance(name, candidate), *candidate))
        .filter(|(distance, candidate)| *distance <= 2 || *distance * 3 <= candidate.len())
        .min_by_key(|(distance, _)| *distance)
        .map(|(_, candidate)| candidate)
}

fn distance(a: &str, b: &str) -> usize {
    let b: Vec<char> = b.chars().collect();
    let mut previous: Vec<usize> = (0..=b.len()).collect();
    let mut current = vec![0usize; b.len() + 1];
    for (row, left) in a.chars().enumerate() {
        current[0] = row + 1;
        for (column, right) in b.iter().enumerate() {
            let cost = usize::from(left != *right);
            current[column + 1] = (previous[column] + cost)
                .min(previous[column + 1] + 1)
                .min(current[column] + 1);
        }
        std::mem::swap(&mut previous, &mut current);
    }
    previous[b.len()]
}

#[cfg(test)]
mod tests {
    use super::*;

    const KNOWN: &[&str] = &["card", "tabs", "tab", "image", "callout"];

    fn translate(source: &str) -> (String, Vec<&'static str>) {
        let (text, diagnostics) = from_mdx(source, KNOWN);
        (text, diagnostics.iter().map(|d| d.code.as_str()).collect())
    }

    #[test]
    fn a_known_component_is_translated() {
        let (text, codes) = translate("<Card title=\"Install\">\nbody\n</Card>\n");
        assert_eq!(text, ":::card{title=\"Install\"}\nbody\n:::\n");
        assert!(codes.is_empty());
    }

    #[test]
    fn an_unknown_component_is_named_with_a_suggestion() {
        let (_, codes) = translate("<Callot>\nbody\n</Callot>\n");
        assert_eq!(codes, ["E0313"]);
        let (_, diagnostics) = from_mdx("<Callot>\nbody\n</Callot>\n", KNOWN);
        assert_eq!(
            diagnostics.iter().next().and_then(|d| d.help.as_deref()),
            Some("did you mean `callout`?")
        );
    }

    #[test]
    fn an_import_is_reported_with_the_snippet_alternative() {
        let (_, diagnostics) = from_mdx("import Note from './note'\n\nbody\n", KNOWN);
        let reported = diagnostics.iter().next().expect("a diagnostic");
        assert_eq!(reported.code, code::E0202);
        assert!(reported.message.contains("import"));
        assert!(
            reported
                .help
                .as_deref()
                .is_some_and(|h| h.contains("snippet"))
        );
    }

    #[test]
    fn an_export_is_reported() {
        let (_, codes) = translate("export const meta = {}\n\nbody\n");
        assert_eq!(codes, ["E0202"]);
    }

    #[test]
    fn a_jsx_expression_container_is_reported() {
        let (_, codes) = translate("{items.map(i => i.name)}\n");
        assert_eq!(codes, ["E0202"]);
    }

    #[test]
    fn a_liyasa_expression_is_not_an_mdx_container() {
        let (_, codes) = translate("{{ page.title }}\n\n{% if x %}y{% endif %}\n");
        assert!(codes.is_empty(), "{codes:?}");
    }

    #[test]
    fn an_mdx_comment_becomes_a_template_comment() {
        let (text, codes) = translate("{/* a note */}\n\nbody\n");
        assert_eq!(text, "{# a note #}\n\nbody\n");
        assert!(codes.is_empty());
    }

    #[test]
    fn an_import_inside_a_fence_is_sample_code() {
        let (_, codes) = translate("```js\nimport x from 'y'\n```\n");
        assert!(codes.is_empty(), "{codes:?}");
    }

    #[test]
    fn a_lowercase_tag_stays_html_and_is_not_a_component() {
        let (text, codes) = translate("<div>\nbody\n</div>\n");
        assert_eq!(text, "<div>\nbody\n</div>\n");
        assert!(codes.is_empty());
    }

    #[test]
    fn nested_components_keep_their_fences() {
        let (text, codes) = translate("<Tabs>\n<Tab title=\"npm\">\nx\n</Tab>\n</Tabs>\n");
        assert_eq!(text, "::::tabs\n:::tab{title=\"npm\"}\nx\n:::\n::::\n");
        assert!(codes.is_empty());
    }

    #[test]
    fn every_offending_component_is_listed() {
        let (_, codes) = translate("<Nope />\n\n<AlsoNope />\n");
        assert_eq!(codes, ["E0313", "E0313"]);
    }
}
