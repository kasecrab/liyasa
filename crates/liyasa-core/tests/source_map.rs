//! Positions, paths, and identity (PRD §7.16, §34.9).

use std::sync::Arc;

use liyasa_core::ids::BlockId;
use liyasa_core::net::{HostPattern, HostSet};
use liyasa_core::span::LineCol;
use liyasa_core::{Fingerprint, SourceMap, Span, VfsPath};

fn map_of(text: &str) -> (SourceMap, liyasa_core::SourceId) {
    let mut map = SourceMap::new();
    let id = map.intern(VfsPath::new("docs/page.md"), Arc::from(text));
    (map, id)
}

#[test]
fn line_col_is_one_based_and_counts_bytes() {
    let (map, id) = map_of("one\ntwo\nthree\n");
    let file = map.get(id);
    assert_eq!(file.line_col(0), LineCol { line: 1, col: 1 });
    assert_eq!(file.line_col(3), LineCol { line: 1, col: 4 });
    assert_eq!(file.line_col(4), LineCol { line: 2, col: 1 });
    assert_eq!(file.line_col(8), LineCol { line: 3, col: 1 });
}

#[test]
fn columns_are_utf8_bytes_not_characters() {
    // "é" is two bytes, so the character after it is at column 4.
    let (map, id) = map_of("aé b\n");
    let file = map.get(id);
    assert_eq!(file.line_col(4), LineCol { line: 1, col: 5 });
}

#[test]
fn an_offset_past_the_end_clamps_instead_of_panicking() {
    let (map, id) = map_of("short\n");
    let file = map.get(id);
    assert_eq!(file.line_col(9_999), file.line_col(6));
}

#[test]
fn line_col_and_offset_round_trip() {
    let text = "alpha\n\nbeta gamma\nδ\n";
    let (map, id) = map_of(text);
    let file = map.get(id);
    for offset in 0..=text.len() as u32 {
        let at = file.line_col(offset);
        assert_eq!(
            file.offset(at),
            Some(offset),
            "offset {offset} did not round-trip"
        );
    }
}

#[test]
fn a_file_with_no_trailing_newline_still_has_its_last_line() {
    let (map, id) = map_of("a\nb");
    let file = map.get(id);
    assert_eq!(file.line_count(), 2);
    assert_eq!(file.line_col(2), LineCol { line: 2, col: 1 });
}

#[test]
fn snippet_includes_the_requested_context() {
    let (map, id) = map_of("1\n2\n3\n4\n5\n");
    let span = Span::new(id, 4, 5); // line 3
    assert_eq!(map.snippet(span, 1), "2\n3\n4");
    assert_eq!(map.snippet(span, 0), "3");
}

#[test]
fn interning_records_the_fingerprint_of_the_text() {
    let (map, id) = map_of("content\n");
    assert_eq!(map.get(id).fingerprint, Fingerprint::of("content\n"));
}

#[test]
fn vfs_paths_normalize() {
    assert_eq!(VfsPath::new("./docs//page.md").as_str(), "docs/page.md");
    assert_eq!(VfsPath::new("docs\\page.md").as_str(), "docs/page.md");
    assert_eq!(VfsPath::new("/docs/page.md").as_str(), "docs/page.md");
    assert_eq!(
        VfsPath::new("docs/../assets/logo.svg").as_str(),
        "assets/logo.svg"
    );
    assert_eq!(VfsPath::new("../../etc/passwd").as_str(), "etc/passwd");
    assert_eq!(VfsPath::new("docs/page.md").extension(), Some("md"));
    assert_eq!(
        VfsPath::new("docs/page.md")
            .parent()
            .map(|p| p.as_str().to_owned()),
        Some("docs".to_owned())
    );
    assert_eq!(VfsPath::new("docs").join("a/../b.md").as_str(), "docs/b.md");
}

#[test]
fn fingerprints_are_unambiguous_over_parts() {
    assert_ne!(
        Fingerprint::of_parts([&b"ab"[..], &b"c"[..]]),
        Fingerprint::of_parts([&b"a"[..], &b"bc"[..]])
    );
}

#[test]
fn fingerprints_round_trip_through_their_text_form() {
    let fingerprint = Fingerprint::of("x");
    let text = fingerprint.to_string();
    assert!(text.starts_with("blake3:"));
    assert_eq!(Fingerprint::parse(&text), Some(fingerprint));
    assert_eq!(Fingerprint::parse("blake3:nope"), None);
}

#[test]
fn explicit_and_implicit_block_ids_share_a_space_without_colliding() {
    let implicit = BlockId::implicit("paragraph", "install the cli", "install", 0);
    let explicit = BlockId::explicit("install-the-cli");
    assert_ne!(implicit, explicit);
    assert_eq!(BlockId::parse(&implicit.to_string()), Some(implicit));
    assert_eq!(implicit.to_string().len(), 24);
}

#[test]
fn implicit_block_ids_depend_on_every_input() {
    let base = BlockId::implicit("paragraph", "text", "anchor", 0);
    assert_ne!(base, BlockId::implicit("heading", "text", "anchor", 0));
    assert_ne!(base, BlockId::implicit("paragraph", "other", "anchor", 0));
    assert_ne!(base, BlockId::implicit("paragraph", "text", "elsewhere", 0));
    assert_ne!(base, BlockId::implicit("paragraph", "text", "anchor", 1));
    assert_eq!(base, BlockId::implicit("paragraph", "text", "anchor", 0));
}

#[test]
fn host_patterns_match_subdomains_only_for_suffixes() {
    let exact = HostSet(vec![HostPattern::Exact("acme.com".into())]);
    assert!(exact.matches("ACME.com"));
    assert!(!exact.matches("api.acme.com"));

    let suffix = HostSet(vec![HostPattern::Suffix("acme.com".into())]);
    assert!(suffix.matches("acme.com"));
    assert!(suffix.matches("api.acme.com"));
    assert!(!suffix.matches("notacme.com"));
    assert!(!suffix.matches("acme.com.evil.test"));
}
