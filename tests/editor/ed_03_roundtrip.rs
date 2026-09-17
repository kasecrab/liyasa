//! ED-03 (a) and (b), over generated documents.
//!
//! > Given 1,000 generated documents; when each is opened and saved without
//! > edits; then bytes are identical; when one block is edited; then only its
//! > segment's bytes change.
//!
//! The generator assembles pages from the constructs the editor models, in
//! random order and at random nesting, rather than shrinking a failure: the
//! workspace has no `proptest` row in PRD §6.2.1's dependency table, and the
//! value here is coverage of the segmenter's shapes rather than minimal
//! counter-examples. The seed is fixed, so a failure names a document anyone
//! can regenerate.
//!
//! **Not covered here, and reported rather than asserted:** a page whose front
//! matter fails to parse loses the whole block on this round trip —
//! `scan_frontmatter` returns `None` while advancing past the block, and
//! `serialize_source` then has no owner for those bytes. It is a defect in
//! `crates/liyasa-markdown/`, which this package does not own; it was sent to
//! the coordinator with a reproduction. The generator below writes front
//! matter the schema accepts, which is the shape an editor round trip meets in
//! practice.

use liyasa_core::document::{Segment, SegmentEdit};
use liyasa_core::span::SourceId;
use liyasa_markdown::{scan, serialize_source};

/// How many documents the requirement names.
const DOCUMENTS: usize = 1_000;

/// A small deterministic generator, so a failure is reproducible from its index.
struct Rng(u64);

impl Rng {
    fn next(&mut self) -> u64 {
        // xorshift64*: three shifts and a multiply, enough for choosing between
        // a dozen constructs and short enough to read.
        let mut x = self.0;
        x ^= x >> 12;
        x ^= x << 25;
        x ^= x >> 27;
        self.0 = x;
        x.wrapping_mul(0x2545_F491_4F6C_DD1D)
    }

    fn below(&mut self, bound: usize) -> usize {
        (self.next() % bound as u64) as usize
    }
}

/// The constructs a generated page is built from.
const PROSE: &[&str] = &[
    "A paragraph of ordinary prose.\n",
    "# A heading\n",
    "## A deeper heading\n",
    "- one\n- two\n",
    "1. first\n2. second\n",
    "> a quotation\n",
    "| a | b |\n| - | - |\n| 1 | 2 |\n",
    "Prose with an \u{e9}, a \u{2603} and an emoji \u{1F388}.\n",
    "<div class=\"legacy\">raw html</div>\n",
    "Text with `inline code` and a [link](/somewhere).\n",
];

const FENCES: &[&str] = &[
    "```bash\nliyasa build\n```\n",
    "```rust {title=\"main.rs\"}\nfn main() {}\n```\n",
    "~~~\nno language\n~~~\n",
];

const TEMPLATES: &[&str] = &[
    "{{ title }}\n",
    "{{ fact(\"plan.pro.price\") }}\n",
    "{# a template comment #}\n",
    "{% for row in rows %}\n- {{ row }}\n{% endfor %}\n",
    "{% if enabled %}\nshown\n{% endif %}\n",
];

const DIRECTIVES: &[&str] = &[
    ":::note\nA note.\n:::\n",
    ":::note{title=\"Heads up\"}\nA note with a prop.\n:::\n",
    "::::tabs\n\n:::tab{title=\"One\"}\nbody\n:::\n\n::::\n",
    "::image{src=\"/assets/a.png\" alt=\"A picture\"}\n",
];

const FRONT: &[&str] = &[
    "",
    "---\ntitle: A page\n---\n",
    "---\ntitle: A page\ndescription: With a description\ndraft: true\n---\n",
    "---\ntitle: A page\nkeywords:\n  - one\n  - two\n---\n",
];

fn document(index: usize) -> String {
    let mut rng = Rng(0x5EED_0000_0000_0001 ^ index as u64);
    let mut out = String::new();
    out.push_str(FRONT[rng.below(FRONT.len())]);
    let blocks = 1 + rng.below(8);
    for _ in 0..blocks {
        let pool = match rng.below(10) {
            0..=3 => PROSE,
            4..=5 => FENCES,
            6..=7 => TEMPLATES,
            _ => DIRECTIVES,
        };
        out.push_str(pool[rng.below(pool.len())]);
        // A blank line between blocks some of the time, so the corpus holds
        // both the separated and the run-together shapes.
        if rng.below(3) != 0 {
            out.push('\n');
        }
    }
    out
}

#[test]
fn a_generated_document_saved_without_edits_keeps_every_byte() {
    for index in 0..DOCUMENTS {
        let text = document(index);
        let (parsed, _) = scan(&text, SourceId(0));
        let written = serialize_source(&text, &parsed, &[]);
        assert_eq!(written, text, "document {index} did not round trip:\n{text}");
    }
}

#[test]
fn the_segment_spans_of_a_generated_document_tile_it_exactly() {
    // The round trip above could pass while the spans overlapped, because
    // `serialize_source` concatenates them either way. This is the property
    // the editor's model depends on: every byte belongs to exactly one
    // segment, so a node always knows which bytes are its own.
    for index in 0..DOCUMENTS {
        let text = document(index);
        let (parsed, _) = scan(&text, SourceId(0));
        let body = parsed.frontmatter.as_ref().map_or(0, |front| front.span.end);
        let mut at = body;
        for (position, segment) in parsed.segments.iter().enumerate() {
            let span = segment.span();
            assert_eq!(
                span.start, at,
                "document {index} segment {position} starts at {} with {at} unaccounted for:\n{text}",
                span.start
            );
            assert!(
                span.end >= span.start,
                "document {index} segment {position} ends before it starts"
            );
            at = span.end;
        }
        assert_eq!(at as usize, text.len(), "document {index} ends with bytes past its last segment");
    }
}

#[test]
fn editing_one_segment_changes_that_segment_and_nothing_else() {
    for index in 0..DOCUMENTS {
        let text = document(index);
        let (parsed, _) = scan(&text, SourceId(0));
        if parsed.segments.is_empty() {
            continue;
        }
        let mut rng = Rng(0xBEEF_0000_0000_0001 ^ index as u64);
        let at = rng.below(parsed.segments.len());
        let span = parsed.segments[at].span();

        // A replacement that cannot appear in the source, so an edit that
        // landed in the wrong place is visible rather than coincidental.
        let replacement = format!("<<EDIT {at}>>");
        let edits = [SegmentEdit {
            segment: at,
            new_text: replacement.clone(),
        }];
        let written = serialize_source(&text, &parsed, &edits);

        let mut expected = String::new();
        expected.push_str(&text[..span.start as usize]);
        expected.push_str(&replacement);
        expected.push_str(&text[span.end as usize..]);
        assert_eq!(
            written, expected,
            "document {index}: editing segment {at} moved bytes outside its span:\n{text}"
        );
    }
}

#[test]
fn every_construct_the_editor_models_appears_in_the_generated_corpus() {
    // A generator that happened to emit only prose would pass all three tests
    // above while proving nothing about the constructs ED-01 names.
    let mut markdown = 0usize;
    let mut code = 0usize;
    let mut output = 0usize;
    let mut statement = 0usize;
    let mut comment = 0usize;
    let mut open = 0usize;
    let mut leaf = 0usize;
    let mut front = 0usize;

    for index in 0..DOCUMENTS {
        let text = document(index);
        let (parsed, _) = scan(&text, SourceId(0));
        if parsed.frontmatter.is_some() {
            front += 1;
        }
        for segment in &parsed.segments {
            match segment {
                Segment::Markdown { .. } => markdown += 1,
                Segment::Code { .. } => code += 1,
                Segment::Template { kind, .. } => match kind {
                    liyasa_core::document::TemplateKind::Output => output += 1,
                    liyasa_core::document::TemplateKind::Statement { .. } => statement += 1,
                    liyasa_core::document::TemplateKind::Comment => comment += 1,
                },
                Segment::DirectiveOpen { .. } => open += 1,
                Segment::DirectiveLeaf { .. } => leaf += 1,
                Segment::DirectiveClose { .. } => {}
            }
        }
    }

    for (name, count) in [
        ("markdown", markdown),
        ("code", code),
        ("template output", output),
        ("template statement", statement),
        ("template comment", comment),
        ("directive open", open),
        ("directive leaf", leaf),
        ("front matter", front),
    ] {
        assert!(count > 0, "the generated corpus holds no {name} segment");
    }
}
