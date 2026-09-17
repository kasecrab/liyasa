//! The chunker over pages the real parser produced, not hand-built nodes.

use crate::page::parse;
use liyasa_ai::chunk::{Chunk, ChunkOptions, chunk, estimate_tokens};

fn chunks(source: &str) -> Vec<Chunk> {
    chunk(&parse(source), "Test page", &ChunkOptions::default())
}

/// Enough prose to force a section over the ceiling, as paragraphs the parser
/// sees as separate blocks.
fn prose(paragraphs: usize) -> String {
    let sentence = "The server resolves the host header to a project and a deployment before \
                    any authentication runs. ";
    (0..paragraphs)
        .map(|n| format!("Paragraph {n}. {}", sentence.repeat(4)))
        .collect::<Vec<_>>()
        .join("\n\n")
}

#[test]
fn a_chunk_carries_the_anchor_of_the_heading_it_sits_under() {
    let chunks = chunks(
        "# Page\n\nLead text.\n\n## First heading\n\nUnder first.\n\n### Second heading\n\nUnder second.\n",
    );
    let anchors: Vec<&str> = chunks.iter().map(|c| c.anchor.as_str()).collect();
    assert_eq!(anchors, ["", "first-heading", "second-heading"]);
    assert_eq!(chunks[0].section, "Test page");
    assert_eq!(chunks[1].section, "First heading");
}

#[test]
fn a_fourth_level_heading_stays_inside_the_section_above_it() {
    let chunks = chunks("## Third\n\nOne.\n\n#### Fourth\n\nTwo.\n");
    assert_eq!(chunks.len(), 1);
    assert!(chunks[0].text.contains("Two."));
}

#[test]
fn every_chunk_repeats_its_heading() {
    let source = format!("## Configuration\n\n{}\n", prose(12));
    let chunks = chunks(&source);
    assert!(
        chunks.len() > 1,
        "the section should split: {}",
        chunks.len()
    );
    for c in &chunks {
        assert!(
            c.text.starts_with("## Configuration"),
            "a chunk retrieved alone must say where it is:\n{}",
            &c.text[..40.min(c.text.len())]
        );
    }
}

#[test]
fn a_code_block_larger_than_the_ceiling_is_kept_whole() {
    let body: String = (0..400)
        .map(|n| format!("let value_{n} = compute(input_{n}, options_{n});\n"))
        .collect();
    let source = format!("## Example\n\n```rust\n{body}```\n");
    let chunks = chunks(&source);
    assert!(
        estimate_tokens(&body) > 800,
        "the fixture must exceed the ceiling"
    );

    let holding: Vec<&Chunk> = chunks
        .iter()
        .filter(|c| c.text.contains("value_0 "))
        .collect();
    assert_eq!(
        holding.len(),
        1,
        "the fence must not be split or duplicated"
    );
    assert!(holding[0].text.contains("value_399 "));
}

#[test]
fn a_table_is_kept_whole() {
    let rows: String = (0..150)
        .map(|n| {
            format!(
                "| key_{n} | a fairly wordy description of what this key does | default_{n} |\n"
            )
        })
        .collect();
    let source = format!("## Keys\n\n| Key | Description | Default |\n|---|---|---|\n{rows}\n");
    let chunks = chunks(&source);
    let holding: Vec<&Chunk> = chunks
        .iter()
        .filter(|c| c.text.contains("key_0 "))
        .collect();
    assert_eq!(holding.len(), 1);
    assert!(holding[0].text.contains("key_149 "));
}

#[test]
fn consecutive_chunks_of_a_section_overlap() {
    let source = format!("## Configuration\n\n{}\n", prose(12));
    let chunks = chunks(&source);
    assert!(chunks.len() > 1);
    for pair in chunks.windows(2) {
        let earlier: Vec<&str> = pair[0]
            .text
            .lines()
            .filter(|l| l.starts_with("Paragraph "))
            .collect();
        let later: Vec<&str> = pair[1]
            .text
            .lines()
            .filter(|l| l.starts_with("Paragraph "))
            .collect();
        let shared = earlier.iter().filter(|l| later.contains(l)).count();
        assert!(shared >= 1, "no overlap between two chunks of one section");
    }
}

#[test]
fn a_prose_chunk_stays_under_the_ceiling() {
    let source = format!("## Configuration\n\n{}\n", prose(20));
    for c in chunks(&source) {
        assert!(
            c.tokens as usize <= ChunkOptions::default().max_tokens,
            "{} tokens in a prose chunk",
            c.tokens
        );
    }
}

#[test]
fn a_paragraph_larger_than_the_ceiling_is_split_on_sentences() {
    let sentence = "This single paragraph runs on well past the ceiling without a blank line. ";
    let source = format!("## Long\n\n{}\n", sentence.repeat(60));
    let chunks = chunks(&source);
    assert!(chunks.len() > 1);
    for c in &chunks {
        assert!(c.tokens as usize <= ChunkOptions::default().max_tokens);
        // Split on a sentence end, not mid-word.
        let body = c.text.trim_end();
        assert!(
            body.ends_with('.'),
            "cut mid-sentence: {:?}",
            &body[body.len() - 20..]
        );
    }
}

#[test]
fn the_hash_changes_only_when_the_text_does() {
    let first = chunks("## A\n\nOne paragraph.\n");
    let again = chunks("## A\n\nOne paragraph.\n");
    let edited = chunks("## A\n\nOne paragraph, edited.\n");
    assert_eq!(first[0].content_hash, again[0].content_hash);
    assert_ne!(first[0].content_hash, edited[0].content_hash);
    assert!(first[0].content_hash.starts_with("blake3:"));
}

#[test]
fn an_empty_page_produces_no_chunks() {
    assert!(chunks("").is_empty());
    assert!(chunks("## Heading with nothing under it\n").is_empty());
}

#[test]
fn an_ordinary_paragraph_costs_more_than_the_overlap_budget() {
    // Why `tail` carries the last block even when the budget says no: with the
    // budget applied strictly, a page of ordinary paragraphs would never
    // overlap at all.
    let one = prose(1);
    assert!(
        estimate_tokens(&one) > liyasa_ai::chunk::OVERLAP_TOKENS,
        "{} tokens in one paragraph against a budget of {}",
        estimate_tokens(&one),
        liyasa_ai::chunk::OVERLAP_TOKENS
    );
}
