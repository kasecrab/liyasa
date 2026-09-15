//! Well-formed templating (CM-21).
//!
//! A block statement may straddle Markdown structure in exactly two ways and
//! no others: both tags alone on their own lines, or both tags on one line.
//! Anything between — an `{% if %}` that opens mid-sentence and closes three
//! paragraphs later, a `{% for %}` that opens inside a list item and closes
//! outside it — makes the expanded document's structure depend on the values,
//! which the Source Document cannot represent and the editor cannot show.
//!
//! This is the rule that keeps expansion output structurally predictable, so
//! it is checked on the source, before anything is expanded.

use liyasa_core::diagnostics::{Diagnostic, Diagnostics, code};
use liyasa_core::document::{Segment, SourceDocument, TemplateKind};
use liyasa_core::span::Span;

use super::lines::{self, ListStack};

/// Where a tag sits: which line, whether the line holds nothing else, and
/// which Markdown container encloses it.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
struct Place {
    line: usize,
    alone: bool,
    container: Container,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
struct Container {
    quotes: usize,
    list_column: usize,
    table: bool,
}

pub fn check(source: &str, document: &SourceDocument, diagnostics: &mut Diagnostics) {
    let places = places(source, super::scan::body_start(document) as usize);
    for segment in &document.segments {
        let Segment::Template {
            span,
            kind: TemplateKind::Statement { name, matching },
            ..
        } = segment
        else {
            continue;
        };
        let Some(close) = matching.and_then(|at| document.segments.get(at)) else {
            continue;
        };
        let (Some(open_at), Some(close_at)) = (
            place(&places, source, *span),
            place(&places, source, close.span()),
        ) else {
            continue;
        };

        if open_at.line == close_at.line {
            continue;
        }
        let message = if !open_at.alone || !close_at.alone {
            format!(
                "`{{% {name} %}}` and `{{% end{name} %}}` must either both stand alone on their \
                 own lines or both lie within one line"
            )
        } else if open_at.container != close_at.container {
            format!(
                "`{{% {name} %}}` opens in {} and closes in {}",
                describe(open_at.container),
                describe(close_at.container)
            )
        } else {
            continue;
        };
        diagnostics.push(
            Diagnostic::new(code::E0210, message)
                .at(*span)
                .label(close.span(), "closed here")
                .help(
                    "a block statement may not open in one Markdown container and close in another",
                ),
        );
    }
}

fn describe(container: Container) -> &'static str {
    match container {
        Container { table: true, .. } => "a table row",
        Container { quotes, .. } if quotes > 0 => "a blockquote",
        Container { list_column, .. } if list_column > 0 => "a list item",
        _ => "the document body",
    }
}

fn places(source: &str, body_at: usize) -> Vec<(Span, Place)> {
    let mut list = ListStack::default();
    let mut out = Vec::new();
    for (at, line) in lines::split(source, body_at).into_iter().enumerate() {
        list.close(&line);
        let container = Container {
            quotes: line.quotes,
            list_column: list.content_column(),
            table: line.content.starts_with('|'),
        };
        out.push((
            Span::new(
                liyasa_core::span::SourceId(0),
                line.start as u32,
                line.next as u32,
            ),
            Place {
                line: at,
                alone: false,
                container,
            },
        ));
        if !line.is_blank() {
            list.open(&line);
        }
    }
    out
}

fn place(places: &[(Span, Place)], source: &str, tag: Span) -> Option<Place> {
    let at = places.partition_point(|(line, _)| line.start <= tag.start);
    let (line, place) = places.get(at.checked_sub(1)?)?;
    let content = source
        .get(line.start as usize..line.end as usize)?
        .trim_matches([' ', '\t', '>', '\r', '\n']);
    let text = source.get(tag.start as usize..tag.end as usize)?;
    Some(Place {
        alone: content == text,
        ..*place
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use liyasa_core::span::SourceId;

    fn diagnose(text: &str) -> Vec<String> {
        let (_, diagnostics) = super::super::scan(text, SourceId(0));
        diagnostics
            .iter()
            .filter(|d| d.code == code::E0210)
            .map(|d| d.message.clone())
            .collect()
    }

    #[test]
    fn a_block_level_loop_is_well_formed() {
        assert!(diagnose("{% for row in rows %}\n- {{ row }}\n{% endfor %}\n").is_empty());
    }

    #[test]
    fn an_inline_conditional_is_well_formed() {
        assert!(diagnose("Plan: {% if pro %}Pro{% else %}Free{% endif %}.\n").is_empty());
    }

    #[test]
    fn a_loop_that_opens_in_a_list_item_and_closes_outside_is_rejected() {
        let found = diagnose("- item {% for row in rows %}\n  body\n{% endfor %}\n");
        assert_eq!(found.len(), 1);
        assert!(found[0].contains("both stand alone"));
    }

    #[test]
    fn a_statement_that_opens_mid_line_and_closes_alone_is_rejected() {
        let found = diagnose("text {% if x %}\nbody\n{% endif %}\n");
        assert_eq!(found.len(), 1);
        assert!(found[0].contains("both stand alone"));
    }

    #[test]
    fn a_statement_may_not_cross_a_blockquote_boundary() {
        let found = diagnose("> {% if x %}\n> body\n{% endif %}\n");
        assert_eq!(found.len(), 1);
        assert!(found[0].contains("blockquote"), "{found:?}");
    }

    #[test]
    fn a_statement_inside_one_blockquote_is_well_formed() {
        assert!(diagnose("> {% if x %}\n> body\n> {% endif %}\n").is_empty());
    }

    #[test]
    fn a_statement_may_not_cross_a_list_item_boundary() {
        let found = diagnose("- item\n  {% for row in rows %}\n  - inner\n\n{% endfor %}\n");
        assert_eq!(found.len(), 1);
        assert!(found[0].contains("list item"), "{found:?}");
    }

    #[test]
    fn a_statement_inside_one_list_item_is_well_formed() {
        assert!(
            diagnose("- item\n  {% for row in rows %}\n  - inner\n  {% endfor %}\n").is_empty()
        );
    }

    #[test]
    fn a_loop_over_table_rows_is_well_formed() {
        let text = "| a | b |\n|---|---|\n{% for row in rows %}\n| {{ row }} | x |\n{% endfor %}\n";
        assert!(diagnose(text).is_empty());
    }

    #[test]
    fn a_statement_that_opens_inside_a_table_row_is_rejected() {
        let text = "| a | {% if x %} |\n| b | c |\n{% endif %}\n";
        let found = diagnose(text);
        assert_eq!(found.len(), 1);
    }

    #[test]
    fn an_unpaired_statement_raises_no_wellformedness_error() {
        // It is already `E0202`; two codes for one mistake helps nobody.
        assert!(diagnose("{% for row in rows %}\nbody\n").is_empty());
    }

    #[test]
    fn whitespace_control_does_not_make_a_tag_look_crowded() {
        assert!(diagnose("{%- for row in rows -%}\nbody\n{%- endfor -%}\n").is_empty());
    }
}
