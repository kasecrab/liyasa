//! `spec/markdown/cm-50/leaf/` and the forged-marker class of
//! `spec/markdown/cm-50/forged-marker/`.

use liyasa_core::document::PropValue;
use liyasa_core::markdown::{ComponentKind, ExpansionRecord, SpanMap};

use super::*;

const NONCE: [u8; 16] = [0x5a; 16];

fn expanded(text: &str) -> Expanded {
    Expanded {
        text: text.to_owned(),
        map: SpanMap::default(),
        record: ExpansionRecord::default(),
    }
}

fn run(text: &str) -> Rewritten {
    let out = rewrite(&expanded(text), NONCE);
    assert_eq!(
        out.text.lines().count(),
        text.lines().count(),
        "the rewrite must preserve line numbers"
    );
    out
}

fn codes(out: &Rewritten) -> Vec<&str> {
    out.diagnostics.iter().map(|d| d.code.as_str()).collect()
}

#[test]
fn a_leaf_becomes_a_marker_and_a_table_row() {
    let out = run("::image{src=\"/a.png\"}\n");
    assert_eq!(out.table.len(), 1);
    let row = out.table.get(0).expect("one row");
    assert_eq!(row.name, "image");
    assert_eq!(row.kind, ComponentKind::Leaf);
    assert_eq!(
        row.props.get("src"),
        Some(&PropValue::Str("/a.png".to_owned()))
    );
    assert_eq!(marker_id(out.text.trim(), NONCE), Some(0));
}

#[test]
fn a_leaf_without_props_is_still_a_leaf() {
    let out = run("::divider\n");
    assert_eq!(out.table.len(), 1);
    assert_eq!(out.table.get(0).expect("one row").name, "divider");
}

#[test]
fn adjacent_leaves_get_their_own_markers() {
    let out = run("::image{src=\"/a.png\"}\n::image{src=\"/b.png\"}\n");
    assert_eq!(out.table.len(), 2);
    let ids: Vec<_> = out.text.lines().map(|l| marker_id(l, NONCE)).collect();
    assert_eq!(ids, [Some(0), Some(1)]);
}

#[test]
fn a_container_fence_is_not_a_leaf() {
    let out = run(":::note\nbody\n:::\n");
    assert!(out.table.is_empty());
    assert_eq!(out.text, ":::note\nbody\n:::\n");
}

#[test]
fn a_bare_double_colon_is_not_a_leaf() {
    let out = run("::\n");
    assert!(out.table.is_empty());
    assert_eq!(out.text, "::\n");
}

#[test]
fn prose_that_starts_with_two_colons_is_not_a_leaf() {
    let out = run("::note this is prose\n");
    assert!(out.table.is_empty());
}

#[test]
fn a_leaf_keeps_its_blockquote_prefix() {
    let out = run("> ::image{src=\"/a.png\"}\n");
    assert_eq!(out.table.len(), 1);
    assert!(out.text.starts_with("> <!--ly:"));
}

#[test]
fn a_leaf_keeps_its_list_item_indentation() {
    let out = run("- item\n  ::image{src=\"/a.png\"}\n");
    assert_eq!(out.table.len(), 1);
    assert!(
        out.text
            .lines()
            .nth(1)
            .is_some_and(|line| line.starts_with("  <!--ly:"))
    );
}

#[test]
fn a_leaf_inside_a_fence_is_literal() {
    let out = run("```\n::image{src=\"/a.png\"}\n```\n");
    assert!(out.table.is_empty());
    assert_eq!(out.text, "```\n::image{src=\"/a.png\"}\n```\n");
}

#[test]
fn a_leaf_in_indented_code_is_literal() {
    let out = run("    ::image{src=\"/a.png\"}\n");
    assert!(out.table.is_empty());
}

#[test]
fn a_leaf_inside_a_container_is_rewritten() {
    let out = run(":::note\n::image{src=\"/a.png\"}\n:::\n");
    assert_eq!(out.table.len(), 1);
}

/// §7.5.1 item 2: no author-controlled text ever becomes a marker.
#[test]
fn a_literal_marker_prefix_is_escaped_and_warned() {
    let out = run("<!--ly:deadbeef:l:0-->\n");
    assert_eq!(codes(&out), ["W0319"]);
    assert!(out.table.is_empty());
    assert_eq!(marker_id(out.text.trim(), NONCE), None);
    assert!(out.text.contains("<!--&#108;y:"));
}

#[test]
fn every_literal_marker_prefix_on_a_line_is_escaped() {
    let out = run("<!--ly:a--> and <!--ly:b-->\n");
    assert_eq!(codes(&out), ["W0319", "W0319"]);
    assert!(!out.text.contains(MARKER_PREFIX));
}

#[test]
fn a_marker_from_another_build_does_not_resolve() {
    let out = run("::image{src=\"/a.png\"}\n");
    assert_eq!(marker_id(out.text.trim(), [0x00; 16]), None);
}

#[test]
fn a_malformed_prop_list_on_a_leaf_is_e0312() {
    let out = run("::image{src=\"/a.png}\n");
    assert_eq!(codes(&out), ["E0312"]);
}

#[test]
fn the_recorded_span_covers_the_directive_line() {
    let text = "before\n::image{src=\"/a.png\"}\n";
    let out = run(text);
    let span = out.table.get(0).expect("one row").span;
    assert_eq!(
        &text[span.start as usize..span.end as usize],
        "::image{src=\"/a.png\"}"
    );
}

#[test]
fn a_prop_span_points_at_what_was_written() {
    let text = "::image{src=\"/a.png\"}\n";
    let out = run(text);
    let row = out.table.get(0).expect("one row");
    let (_, span) = row
        .prop_spans
        .iter()
        .find(|(name, _)| name == "src")
        .expect("src has a span");
    assert_eq!(
        &text[span.start as usize..span.end as usize],
        "src=\"/a.png\""
    );
}

/// The map is what turns a rewritten offset back into an expanded one; every
/// line start must lead back to the start of the same line in the input.
#[test]
fn the_rewrite_map_leads_every_line_start_back() {
    let text = "before\n::image{src=\"/a.png\" alt=\"A picture\"}\nafter\n::x\nend\n";
    let out = run(text);
    let starts = |text: &str| {
        let mut at = 0;
        text.split_inclusive('\n')
            .map(move |line| {
                let start = at;
                at += line.len();
                start as u32
            })
            .collect::<Vec<_>>()
    };
    assert_eq!(
        starts(&out.text)
            .into_iter()
            .map(|at| out.map.to_expanded(at))
            .collect::<Vec<_>>(),
        starts(text)
    );
}

/// A multibyte line before a rewritten one must not shift the map.
#[test]
fn the_rewrite_map_survives_multibyte_content() {
    let text = "安装 · Установка\n::image{src=\"/a.png\"}\nend\n";
    let out = run(text);
    let second = out.text.find("<!--ly:").expect("the marker is there");
    assert_eq!(
        out.map.to_expanded(second as u32) as usize,
        text.find("::image").expect("the directive is there")
    );
}

/// The one place the leaf path still needs comrak's container algorithm and
/// does not have it: at a list item's content column plus four, the pass cannot
/// tell a directive from indented code.
#[test]
fn a_leaf_deep_inside_a_list_is_missed() {
    let out = run("- a\n  - b\n    - c\n      ::image{src=\"/a.png\"}\n");
    assert!(
        out.table.is_empty(),
        "if this starts passing, the case is no longer pending"
    );
}
