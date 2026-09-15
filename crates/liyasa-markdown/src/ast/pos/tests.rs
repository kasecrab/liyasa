use liyasa_core::document::Frame;

use super::*;

#[test]
fn line_and_column_to_offset() {
    let lines = Lines::of("ab\ncd\n");
    assert_eq!(lines.offset(1, 1), 0);
    assert_eq!(lines.offset(1, 3), 2);
    assert_eq!(lines.offset(2, 1), 3);
    assert_eq!(lines.offset(2, 3), 5);
}

#[test]
fn a_position_past_the_end_clamps() {
    let lines = Lines::of("ab\n");
    assert_eq!(lines.offset(9, 9), 3);
    assert_eq!(lines.offset(1, 99), 3);
    assert_eq!(lines.offset(0, 0), 0);
}

#[test]
fn a_text_with_no_trailing_newline() {
    let lines = Lines::of("ab");
    assert_eq!(lines.offset(1, 3), 2);
    assert_eq!(lines.offset(2, 1), 2);
}

#[test]
fn an_empty_text_has_one_line() {
    assert_eq!(Lines::of("").offset(1, 1), 0);
}

fn positions<'a>(text: &str, rewrite: &'a RewriteMap, expansion: &'a SpanMap) -> Positions<'a> {
    Positions::new(SourceId(7), text, rewrite, expansion)
}

#[test]
fn a_page_with_no_expansion_is_already_in_source_coordinates() {
    let rewrite = RewriteMap::default();
    let expansion = SpanMap::default();
    let positions = positions("hello\n", &rewrite, &expansion);
    let span = Span::new(SourceId(7), 0, 5);
    assert_eq!(positions.origin(span), Origin::at(span));
}

#[test]
fn an_offset_inside_an_included_run_keeps_its_place_in_the_included_file() {
    let included = SourceId(3);
    let expansion = SpanMap(vec![(
        10,
        20,
        Origin {
            span: Some(Span::new(included, 100, 110)),
            frames: vec![Frame::Include {
                file: included,
                at: Span::new(SourceId(7), 0, 5),
            }],
        },
    )]);
    let rewrite = RewriteMap::default();
    let positions = positions(&" ".repeat(30), &rewrite, &expansion);

    let origin = positions.origin(Span::new(SourceId(7), 13, 15));
    assert_eq!(origin.span, Some(Span::new(included, 103, 105)));
    assert_eq!(origin.frames.len(), 1);
}

#[test]
fn generated_content_keeps_its_frames_and_has_no_span() {
    let expansion = SpanMap(vec![(
        0,
        5,
        Origin::generated_by(Span::new(SourceId(7), 1, 2)),
    )]);
    let rewrite = RewriteMap::default();
    let positions = positions("hello", &rewrite, &expansion);
    let origin = positions.origin(Span::new(SourceId(7), 0, 5));
    assert!(origin.is_generated());
    assert_eq!(origin.frames.len(), 1);
}
