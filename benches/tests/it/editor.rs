//! NFR-05's lower bound, and the two things that would make it meaningless.

use liyasa_benches::editor::{self, WORD_LIMIT};

#[test]
fn the_page_measured_is_inside_the_range_the_row_covers() {
    // The budget is "pages under 5,000 words". The first version of `page`
    // emitted a fixed number of chunks and produced 5,389 words for a target of
    // 4,900, because `[a link](/elsewhere)` is two whitespace tokens and
    // `**bold**` is one — so it measured a page the row does not cover and
    // would have reported it as the row's case.
    let source = editor::page(WORD_LIMIT - 100);
    let words = source.split_whitespace().count();
    assert!(
        words < WORD_LIMIT,
        "{words} words is not under the {WORD_LIMIT} the row budgets"
    );
    // And not trivially short: a 50-word page would also be under the limit and
    // would measure nothing like the slowest case the budget covers.
    assert!(
        words > WORD_LIMIT - 200,
        "{words} words is far short of the limit, so this is not the hard case"
    );
}

#[test]
fn the_page_has_the_structure_the_parser_is_being_timed_on() {
    // A page of one repeated word measures the fast path. The figure is only
    // about the editor if the inline and block parsers have real work.
    let source = editor::page(WORD_LIMIT - 100);
    assert!(
        source.contains("## Section 1"),
        "no headings: {source:.120}"
    );
    assert!(source.contains("`code`"), "no inline code");
    assert!(source.contains("**bold**"), "no strong emphasis");
    assert!(source.contains("[a link](/elsewhere)"), "no links");
    assert!(source.starts_with("---\n"), "no front matter");
}

#[test]
fn the_measurement_renders_the_page_rather_than_timing_an_empty_response() {
    // An empty render is fast. A timing that cannot tell "rendered nothing
    // quickly" from "rendered the page quickly" is the shape of every vacuous
    // check in this repository, so `measure` returns the output size and this
    // asserts on it.
    let (preview, slowest) = editor::measure(5).expect("the session opens and renders");

    assert!(
        preview.html_bytes > 10_000,
        "{} bytes of HTML for {} words is not a rendered page",
        preview.html_bytes,
        preview.words
    );
    assert!(
        preview.elapsed > std::time::Duration::ZERO,
        "a zero duration means the clock, not the renderer"
    );
    assert!(
        slowest >= preview.elapsed,
        "the slowest sample cannot be faster than the median"
    );
}
