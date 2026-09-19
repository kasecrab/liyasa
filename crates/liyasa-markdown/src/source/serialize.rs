//! Writing a Source Document back out (§34.9 `serialize_source`).
//!
//! Byte-preserving outside the edits: every segment the caller did not edit is
//! copied from the source, so an editor round trip cannot reformat a page the
//! author did not touch. That is the whole point of the Source Document.
//!
//! The text is a parameter because a `SourceDocument` holds spans, not bytes;
//! `plan/rfcs/0201-source-text-for-expansion.md` records why.

use liyasa_core::document::{SegmentEdit, SourceDocument};

pub fn serialize_source(text: &str, document: &SourceDocument, edits: &[SegmentEdit]) -> String {
    let mut out = String::with_capacity(text.len());
    if let Some(front) = &document.frontmatter {
        out.push_str(&text[front.span.start as usize..front.span.end as usize]);
    }
    for (at, segment) in document.segments.iter().enumerate() {
        match edits.iter().find(|edit| edit.segment == at) {
            Some(edit) => out.push_str(&edit.new_text),
            None => {
                let span = segment.span();
                out.push_str(&text[span.start as usize..span.end as usize]);
            }
        }
    }
    out
}

#[cfg(test)]
mod tests {
    use liyasa_core::span::SourceId;

    use super::*;
    use crate::source::scan;

    fn round_trip(text: &str) -> String {
        let (document, _) = scan::scan(text, SourceId(0));
        serialize_source(text, &document, &[])
    }

    #[test]
    fn an_untouched_document_is_reproduced_byte_for_byte() {
        for text in [
            "",
            "# Title\n\nbody\n",
            "---\ntitle: A\n---\n\nbody\n",
            // Front matter the YAML reader rejects is still front matter.
            "---\nid: not-a-ulid\ntitle: Limits\n---\n\nbody\n",
            "```bash\necho {{ x }}\n```\n",
            ":::note\nbody\n:::\n",
            "{% for row in rows %}\n- {{ row }}\n{% endfor %}\n",
            "trailing text with no newline",
        ] {
            assert_eq!(round_trip(text), text);
        }
    }

    /// The diagnostic is about one value; the deletion was of everything
    /// around it, and nothing reported that.
    #[test]
    fn a_page_whose_front_matter_does_not_parse_keeps_it() {
        let text = "---\nid: not-a-ulid\ntitle: Limits\n---\n\nbody\n";
        let (document, diagnostics) = scan::scan(text, SourceId(0));
        assert!(
            diagnostics.iter().any(|d| d.code.as_str() == "E0102"),
            "the fixture is supposed to be unparseable YAML"
        );
        assert_eq!(serialize_source(text, &document, &[]), text);
    }

    #[test]
    fn an_edit_replaces_one_segment_and_nothing_else() {
        let text = "before\n{{ x }}\nafter\n";
        let (document, _) = scan::scan(text, SourceId(0));
        let at = document
            .segments
            .iter()
            .position(|segment| matches!(segment, liyasa_core::document::Segment::Template { .. }))
            .expect("a template segment");
        let edits = [SegmentEdit {
            segment: at,
            new_text: "{{ y }}".to_owned(),
        }];
        assert_eq!(
            serialize_source(text, &document, &edits),
            "before\n{{ y }}\nafter\n"
        );
    }

    #[test]
    fn several_edits_apply_in_segment_order() {
        let text = "a\n\nb\n";
        let (document, _) = scan::scan(text, SourceId(0));
        let edits: Vec<SegmentEdit> = (0..document.segments.len())
            .map(|segment| SegmentEdit {
                segment,
                new_text: "X".to_owned(),
            })
            .collect();
        assert_eq!(serialize_source(text, &document, &edits), "X");
    }

    #[test]
    fn front_matter_survives_an_edit_to_the_body() {
        let text = "---\ntitle: A\n---\nbody\n";
        let (document, _) = scan::scan(text, SourceId(0));
        let edits = [SegmentEdit {
            segment: 0,
            new_text: "new body\n".to_owned(),
        }];
        assert_eq!(
            serialize_source(text, &document, &edits),
            "---\ntitle: A\n---\nnew body\n"
        );
    }
}
