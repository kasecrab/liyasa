use super::*;

#[test]
fn a_plain_line_is_all_content() {
    let split = split("text");
    assert_eq!(split.lead, "");
    assert_eq!(split.content, "text");
    assert_eq!(split.indent, 0);
}

#[test]
fn indentation_is_counted_and_stripped() {
    let split = split("    ::image");
    assert_eq!(split.lead, "    ");
    assert_eq!(split.content, "::image");
    assert_eq!(split.indent, 4);
}

#[test]
fn a_tab_advances_to_the_next_stop() {
    assert_eq!(split("\ttext").indent, 4);
    assert_eq!(split("  \ttext").indent, 4);
}

#[test]
fn blockquote_markers_reset_the_indent() {
    let split = split("  > > ::image");
    assert_eq!(split.content, "::image");
    assert_eq!(split.indent, 1);
}

#[test]
fn fences_mask_their_body_and_both_delimiters() {
    let mut fences = Fences::default();
    assert!(fences.step("```", 0));
    assert!(fences.step("::image", 0));
    assert!(fences.step("```", 0));
    assert!(!fences.step("::image", 0));
}

#[test]
fn a_longer_fence_is_not_closed_by_a_shorter_one() {
    let mut fences = Fences::default();
    assert!(fences.step("````", 0));
    assert!(fences.step("```", 0));
    assert!(fences.is_open());
}

#[test]
fn tildes_and_backticks_do_not_close_each_other() {
    let mut fences = Fences::default();
    assert!(fences.step("~~~", 0));
    assert!(fences.step("```", 0));
    assert!(fences.is_open());
}

/// `` `{% raw %}` `` is a code span on one line, not a fence.
#[test]
fn a_code_span_is_not_a_fence() {
    let mut fences = Fences::default();
    assert!(!fences.step("``` not a fence ```", 0));
    assert!(!fences.is_open());
}

#[test]
fn an_info_string_does_not_open_a_span() {
    let mut fences = Fences::default();
    assert!(fences.step("```rust", 0));
    assert!(fences.is_open());
}

#[test]
fn four_spaces_is_indented_code() {
    let mut fences = Fences::default();
    assert!(fences.step("::image", 4));
    assert!(!fences.step("::image", 3));
}
