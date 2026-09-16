//! The position composition the marker approach rests on (PRD §7.5.1 item 2).
//!
//! `RewriteMap` is a frozen contract in `liyasa-core`, so these hold whether or
//! not the marker approach is the one that ships: leaf directives keep it.

use comrak::{Arena, Options};
use liyasa_core::markdown::RewriteMap;
use xtask::spike::markers;

const NONCE: [u8; 16] = [0x5a; 16];

fn options() -> Options<'static> {
    let mut options = Options::default();
    options.render.r#unsafe = true;
    options
}

fn check(source: &str) -> Vec<String> {
    let arena = Arena::new();
    let parsed = markers::parse(&arena, source, &options(), NONCE);
    assert!(
        !parsed.reported.is_empty(),
        "no markers reached comrak for:\n{source}"
    );
    markers::position_round_trip(&parsed, source)
}

#[test]
fn a_directive_composes_back_to_its_source_offset() {
    assert_eq!(check(":::note\nbody\n:::\n"), Vec::<String>::new());
}

#[test]
fn composition_survives_lines_that_change_length() {
    // The marker is much longer than `:::a` and much shorter than the long one,
    // so the cumulative delta changes sign across the document.
    let source =
        ":::a\nx\n:::\n\n:::verylongcomponentname{title=\"a rather long value\"}\ny\n:::\n";
    assert_eq!(check(source), Vec::<String>::new());
}

#[test]
fn composition_survives_multibyte_content_before_a_directive() {
    assert_eq!(
        check("안녕하세요 세계\n\n:::a\nx\n:::\n"),
        Vec::<String>::new()
    );
}

#[test]
fn composition_survives_a_leaf_directive() {
    assert_eq!(
        check("::img{src=\"/a.png\"}\n\npara\n"),
        Vec::<String>::new()
    );
}

#[test]
fn composition_survives_many_directives() {
    assert_eq!(
        check(":::a\nx\n:::\n:::b\ny\n:::\n:::c\nz\n:::\n"),
        Vec::<String>::new()
    );
}

#[test]
fn the_check_catches_a_map_that_is_off_by_one() {
    let source = ":::note\nbody\n:::\n";
    let arena = Arena::new();
    let mut parsed = markers::parse(&arena, source, &options(), NONCE);
    assert!(
        markers::position_round_trip(&parsed, source).is_empty(),
        "sound to begin with"
    );

    parsed.map = RewriteMap(
        parsed
            .map
            .0
            .iter()
            .map(|(at, delta)| (*at, delta + 1))
            .collect(),
    );
    let broken = markers::position_round_trip(&parsed, source);
    assert!(
        !broken.is_empty(),
        "a map shifted by one byte must not round-trip"
    );
    assert!(broken[0].contains("composed to byte"), "{broken:?}");
}

#[test]
fn an_empty_map_is_the_identity() {
    assert_eq!(RewriteMap::default().to_expanded(0), 0);
    assert_eq!(RewriteMap::default().to_expanded(1234), 1234);
}
