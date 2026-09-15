//! `spec/markdown/cm-31/anchors/`.

use super::*;

#[test]
fn slugs_are_lowercase_and_hyphenated() {
    assert_eq!(slugify("Install the CLI"), "install-the-cli");
    assert_eq!(slugify("Setup"), "setup");
}

#[test]
fn punctuation_is_dropped_and_does_not_leave_a_gap() {
    assert_eq!(slugify("What's new?"), "whats-new");
    assert_eq!(slugify("A -- B"), "a-b");
    assert_eq!(slugify("  Leading and trailing  "), "leading-and-trailing");
    assert_eq!(slugify("C++ / Rust"), "c-rust");
}

#[test]
fn hyphens_and_underscores_survive() {
    assert_eq!(slugify("build-id"), "build-id");
    assert_eq!(slugify("build_id"), "build_id");
}

#[test]
fn letters_of_every_script_survive() {
    assert_eq!(slugify("安装 CLI"), "安装-cli");
    assert_eq!(slugify("Установка"), "установка");
}

/// The acceptance case of CM-31.
#[test]
fn duplicates_are_numbered_and_an_explicit_id_is_taken_as_written() {
    let mut anchors = Anchors::new();
    assert_eq!(anchors.assign("Setup", None), "setup");
    assert_eq!(anchors.assign("Setup", None), "setup-1");
    assert_eq!(anchors.assign("Anything", Some("custom")), "custom");
}

/// Adding a heading elsewhere must not move an anchor that was already
/// published: the counter is per slug, not per page.
#[test]
fn an_unrelated_heading_does_not_move_an_anchor() {
    let with_intro = {
        let mut anchors = Anchors::new();
        anchors.assign("Intro", None);
        [anchors.assign("Setup", None), anchors.assign("Setup", None)]
    };
    let without = {
        let mut anchors = Anchors::new();
        [anchors.assign("Setup", None), anchors.assign("Setup", None)]
    };
    assert_eq!(with_intro, without);
}

/// `a`, `a`, `a-1` must not all want `a-1`.
#[test]
fn a_written_suffix_does_not_collide_with_a_generated_one() {
    let mut anchors = Anchors::new();
    assert_eq!(anchors.assign("a", None), "a");
    assert_eq!(anchors.assign("a", None), "a-1");
    assert_eq!(anchors.assign("a-1", None), "a-1-1");
    assert_eq!(anchors.assign("a", None), "a-2");
}

#[test]
fn an_explicit_id_claimed_twice_is_still_unique() {
    let mut anchors = Anchors::new();
    assert_eq!(anchors.assign("A", Some("x")), "x");
    assert_eq!(anchors.assign("B", Some("x")), "x-1");
}

#[test]
fn a_heading_with_no_sluggable_text_still_gets_an_anchor() {
    let mut anchors = Anchors::new();
    assert_eq!(anchors.assign("!!!", None), "section");
    assert_eq!(anchors.assign("???", None), "section-1");
}

#[test]
fn every_anchor_on_a_page_is_unique() {
    let mut anchors = Anchors::new();
    let assigned: Vec<_> = ["a", "a", "a-1", "a", "a-1", "", "!", "A", "  a  "]
        .into_iter()
        .map(|text| anchors.assign(text, None))
        .collect();
    let mut sorted = assigned.clone();
    sorted.sort();
    sorted.dedup();
    assert_eq!(sorted.len(), assigned.len(), "collision in {assigned:?}");
}
