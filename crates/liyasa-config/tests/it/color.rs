//! Colour parsing and the WCAG contrast check behind `E0107` (CFG-04).

use liyasa_config::color::{Color, Parsed, contrast};

fn rgb(text: &str) -> (u8, u8, u8) {
    match Color::parse(text) {
        Parsed::Known(color) => (color.red, color.green, color.blue),
        other => panic!("`{text}` did not parse: {other:?}"),
    }
}

#[test]
fn hex_in_every_length() {
    assert_eq!(rgb("#4F46E5"), (0x4F, 0x46, 0xE5));
    assert_eq!(rgb("#fff"), (255, 255, 255));
    assert_eq!(rgb("#1234"), (0x11, 0x22, 0x33));
    assert_eq!(rgb("#0a0b0c80"), (0x0a, 0x0b, 0x0c));
}

#[test]
fn functional_notations() {
    assert_eq!(rgb("rgb(79, 70, 229)"), (79, 70, 229));
    assert_eq!(rgb("rgb(79 70 229 / 50%)"), (79, 70, 229));
    assert_eq!(rgb("rgba(0,0,0,0.5)"), (0, 0, 0));
    assert_eq!(rgb("hsl(0, 100%, 50%)"), (255, 0, 0));
    assert_eq!(rgb("hsl(120 100% 25%)"), (0, 128, 0));
    assert_eq!(rgb("rgb(50%, 0%, 0%)"), (128, 0, 0));
}

#[test]
fn the_named_colours_liyasa_knows() {
    assert_eq!(rgb("white"), (255, 255, 255));
    assert_eq!(rgb("Black"), (0, 0, 0));
    assert_eq!(rgb("red"), (255, 0, 0));
}

#[test]
fn a_value_that_is_not_a_colour_is_reported() {
    assert!(matches!(Color::parse("#ggg"), Parsed::Malformed));
    assert!(matches!(Color::parse("#12345"), Parsed::Malformed));
    assert!(matches!(Color::parse("rgb(1, 2)"), Parsed::Malformed));
    assert!(matches!(Color::parse("hsl()"), Parsed::Malformed));
}

#[test]
fn a_value_liyasa_simply_does_not_resolve_is_left_alone() {
    // A named colour outside the short table, a custom property, and the
    // keywords: none of these is an error, they are just not checkable here.
    assert!(matches!(Color::parse("rebeccapurple"), Parsed::Unresolved));
    assert!(matches!(Color::parse("var(--brand)"), Parsed::Unresolved));
    assert!(matches!(Color::parse("currentColor"), Parsed::Unresolved));
    assert!(matches!(Color::parse("transparent"), Parsed::Unresolved));
}

#[test]
fn contrast_matches_the_wcag_reference_values() {
    let white = Color::new(255, 255, 255);
    let black = Color::new(0, 0, 0);
    assert!((contrast(white, black) - 21.0).abs() < 0.01);
    assert!((contrast(white, white) - 1.0).abs() < 0.001);

    // #4F46E5 on white: the indigo the §34.2 example uses.
    let indigo = Color::new(0x4F, 0x46, 0xE5);
    let ratio = contrast(indigo, white);
    assert!((ratio - 6.288).abs() < 0.005, "ratio was {ratio}");
    assert!(ratio >= 4.5, "the example's primary passes AA");
}

#[test]
fn a_pale_primary_fails_aa_against_white() {
    let ratio = contrast(Color::new(0x81, 0x8C, 0xF8), Color::new(255, 255, 255));
    assert!(ratio < 4.5, "ratio was {ratio}");
}
