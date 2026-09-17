//! Positions in, byte offsets out. The encoding matters: an editor counting
//! UTF-16 units and a span counting bytes disagree about every line with a
//! character outside the basic multilingual plane in it.

use liyasa_lsp::protocol::{Position, PositionEncoding::*};
use liyasa_lsp::text::Text;

#[test]
fn offsets_and_positions_are_inverse_on_ascii() {
    let text = Text::new("one\ntwo\nthree\n");
    for (line, character, offset) in [(0, 0, 0), (0, 3, 3), (1, 0, 4), (2, 5, 13), (3, 0, 14)] {
        let at = Position::new(line, character);
        assert_eq!(text.offset_of(at, Utf16), offset, "{at:?}");
        assert_eq!(text.position_of(offset, Utf16), at, "offset {offset}");
    }
}

#[test]
fn a_two_byte_character_is_one_utf16_unit_and_two_bytes() {
    let text = Text::new("é end");
    assert_eq!(text.offset_of(Position::new(0, 1), Utf16), 2);
    assert_eq!(text.offset_of(Position::new(0, 2), Utf8), 2);
    assert_eq!(text.position_of(2, Utf16).character, 1);
    assert_eq!(text.position_of(2, Utf8).character, 2);
}

#[test]
fn an_astral_character_is_two_utf16_units_and_four_bytes() {
    // A directive whose prop value is an emoji is ordinary documentation.
    let text = Text::new("::icon{name=\"🚀\"}");
    let rocket = text.as_str().find('🚀').expect("the emoji is in the line") as u32;
    let after = text.position_of(rocket + 4, Utf16);
    let before = text.position_of(rocket, Utf16);
    assert_eq!(after.character - before.character, 2, "one surrogate pair");
    assert_eq!(text.offset_of(after, Utf16), rocket + 4, "and back again");

    let after_32 = text.position_of(rocket + 4, Utf32);
    let before_32 = text.position_of(rocket, Utf32);
    assert_eq!(
        after_32.character - before_32.character,
        1,
        "one code point"
    );
}

#[test]
fn crlf_lines_end_before_the_carriage_return() {
    let text = Text::new("one\r\ntwo\r\n");
    assert_eq!(text.line(0), "one");
    assert_eq!(text.line(1), "two");
    assert_eq!(text.offset_of(Position::new(1, 0), Utf16), 5);
}

#[test]
fn a_trailing_newline_opens_a_last_empty_line() {
    let text = Text::new("one\n");
    assert_eq!(text.line_count(), 2);
    assert_eq!(text.line(1), "");
    assert_eq!(text.offset_of(Position::new(1, 0), Utf16), 4);
}

#[test]
fn a_document_without_a_trailing_newline_has_no_extra_line() {
    let text = Text::new("one");
    assert_eq!(text.line_count(), 1);
    assert_eq!(text.offset_of(Position::new(0, 3), Utf16), 3);
}

#[test]
fn a_position_past_the_end_clamps_instead_of_panicking() {
    // The client's view of the buffer can be one keystroke ahead of ours.
    let text = Text::new("short\n");
    assert_eq!(text.offset_of(Position::new(99, 0), Utf16), text.len());
    assert_eq!(text.offset_of(Position::new(0, 99), Utf16), 5);
    assert_eq!(text.position_of(9_999, Utf16).line, 1);
}

#[test]
fn an_offset_inside_a_character_rounds_down_to_its_start() {
    let text = Text::new("é");
    assert_eq!(text.position_of(1, Utf16), Position::new(0, 0));
}

#[test]
fn an_empty_document_has_one_line_and_one_position() {
    let text = Text::new("");
    assert!(text.is_empty());
    assert_eq!(text.line_count(), 1);
    assert_eq!(text.offset_of(Position::new(0, 0), Utf16), 0);
    assert_eq!(text.position_of(0, Utf16), Position::new(0, 0));
}

#[test]
fn a_range_never_ends_before_it_starts() {
    let text = Text::new("one\ntwo\n");
    let range = text.range_of(6, 2, Utf16);
    assert_eq!(range.start, range.end, "an inverted span collapses");
}
