//! Chunking a page for the vector index (AST-01).
//!
//! Boundaries are provider-independent by construction: the token count is the
//! neutral estimate of RFC 1800, so re-routing `ai.models.embeddings` re-embeds
//! the corpus without re-chunking it, which is what lets AST-05 swap an index
//! rather than rebuild one.
//!
//! Sections are cut where `liyasa-search` cuts them — at H2 and H3 — so the
//! anchor a citation deep-links to is a section the search index also knows.

use liyasa_core::document::{Block, BlockKind, Document, Inline, Node};
use liyasa_core::ids::Fingerprint;

/// The window AST-01 names.
pub const MIN_TOKENS: usize = 200;
pub const MAX_TOKENS: usize = 800;
/// RFC 1802: AST-01 says "with overlap" and no size.
pub const OVERLAP_TOKENS: usize = 80;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct ChunkOptions {
    pub min_tokens: usize,
    pub max_tokens: usize,
    pub overlap_tokens: usize,
}

impl Default for ChunkOptions {
    fn default() -> Self {
        Self {
            min_tokens: MIN_TOKENS,
            max_tokens: MAX_TOKENS,
            overlap_tokens: OVERLAP_TOKENS,
        }
    }
}

/// One chunk of one section, before it is embedded.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Chunk {
    /// The heading this chunk sits under, `""` for the lead section.
    pub anchor: String,
    /// The heading's text, or the page title for the lead section.
    pub section: String,
    /// Which chunk of that section, from zero.
    pub ordinal: u32,
    pub text: String,
    pub tokens: u32,
    /// `blake3:…` of `text`. Only a chunk whose hash changed is re-embedded.
    pub content_hash: String,
}

/// A vendor-neutral token estimate (RFC 1800).
///
/// Used by the chunker and by the re-index cost estimate, and deliberately one
/// function so a per-provider tokenizer replaces it in one place.
// TODO(rfc-1800): takes the `ModelRef` once a tokenizer crate is in §6.2.1.
pub fn estimate_tokens(text: &str) -> usize {
    let mut total = 0;
    for word in text.split_whitespace() {
        let mut bytes = 0;
        let mut punctuation = 0;
        let mut cjk = 0;
        let mut previous_punctuation = false;
        for c in word.chars() {
            if is_cjk(c) {
                cjk += 1;
                previous_punctuation = false;
                continue;
            }
            bytes += c.len_utf8();
            let punct = c.is_ascii_punctuation();
            if punct && !previous_punctuation {
                punctuation += 1;
            }
            previous_punctuation = punct;
        }
        total += cjk;
        if bytes > 0 {
            total += bytes.div_ceil(4).max(1) + punctuation;
        }
    }
    total
}

/// The ranges every tokenizer in the §6.7 set splits per character rather than
/// per few bytes: CJK ideographs, kana, and Hangul.
fn is_cjk(c: char) -> bool {
    matches!(c as u32,
        0x3040..=0x30FF      // kana
        | 0x3400..=0x4DBF    // CJK extension A
        | 0x4E00..=0x9FFF    // CJK unified
        | 0xAC00..=0xD7AF    // Hangul syllables
        | 0xF900..=0xFAFF    // compatibility ideographs
        | 0x20000..=0x2FA1F) // extensions B and up
}

/// Chunks a page's Rendered AST.
///
/// `title` is the page title, used to name the lead section the way
/// `liyasa-search` names it.
pub fn chunk(document: &Document, title: &str, options: &ChunkOptions) -> Vec<Chunk> {
    let mut out = Vec::new();
    for section in sections(document, title) {
        // The heading is repeated into every chunk of the section, so its cost
        // comes out of the ceiling before the section is split rather than
        // after. Adding it afterwards put a chunk one token over the ceiling it
        // had just been packed to fit.
        let budget = ChunkOptions {
            max_tokens: options
                .max_tokens
                .saturating_sub(section.heading_tokens())
                .max(1),
            ..*options
        };
        let units = section.units(&budget);
        for (ordinal, text) in pack(&units, &budget).into_iter().enumerate() {
            let text = match section.heading.as_ref() {
                Some(heading) => format!("{heading}\n\n{text}"),
                None => text,
            };
            let tokens = estimate_tokens(&text) as u32;
            out.push(Chunk {
                anchor: section.anchor.clone(),
                section: section.title.clone(),
                ordinal: ordinal as u32,
                content_hash: format!("{}{}", Fingerprint::PREFIX, Fingerprint::of(&text).to_hex()),
                text,
                tokens,
            });
        }
    }
    out
}

struct Section<'a> {
    anchor: String,
    title: String,
    /// The heading line, repeated at the top of every chunk of this section so
    /// a chunk retrieved on its own still says where it is.
    heading: Option<String>,
    blocks: Vec<&'a Block>,
}

impl Section<'_> {
    fn heading_tokens(&self) -> usize {
        match self.heading.as_ref() {
            // Two newlines join it to the body; whitespace costs nothing.
            Some(heading) => estimate_tokens(heading),
            None => 0,
        }
    }

    fn units(&self, options: &ChunkOptions) -> Vec<Unit> {
        let mut out = Vec::new();
        for block in &self.blocks {
            let text = render_block(block);
            if text.trim().is_empty() {
                continue;
            }
            let atomic = matches!(
                block.kind,
                BlockKind::CodeBlock { .. } | BlockKind::Table { .. }
            );
            let tokens = estimate_tokens(&text);
            if !atomic && tokens > options.max_tokens {
                out.extend(split_prose(&text, options.max_tokens));
            } else {
                out.push(Unit {
                    text,
                    tokens,
                    atomic,
                });
            }
        }
        out
    }
}

#[derive(Debug, Clone)]
struct Unit {
    text: String,
    tokens: usize,
    /// A code block or a table: never split, whatever its size (AST-01).
    atomic: bool,
}

/// Greedily fills chunks, then seeds the next one with the tail of the last.
fn pack(units: &[Unit], options: &ChunkOptions) -> Vec<String> {
    let mut chunks: Vec<Vec<&Unit>> = Vec::new();
    let mut current: Vec<&Unit> = Vec::new();
    let mut tokens = 0;

    for unit in units {
        if !current.is_empty() && tokens + unit.tokens > options.max_tokens {
            let overlap = tail(&current, options.overlap_tokens, options.max_tokens);
            chunks.push(std::mem::take(&mut current));
            tokens = overlap.iter().map(|u| u.tokens).sum();
            current = overlap;
            // The overlap plus the incoming unit may already be over the
            // ceiling; the overlap is what gives, never the unit.
            while !current.is_empty() && tokens + unit.tokens > options.max_tokens {
                tokens -= current.remove(0).tokens;
            }
        }
        tokens += unit.tokens;
        current.push(unit);
    }
    if !current.is_empty() {
        chunks.push(current);
    }

    // A trailing remnant is folded back when it fits, so a section does not end
    // with a 30-token chunk that carries no context of its own.
    if chunks.len() >= 2 {
        let last = chunks.len() - 1;
        let remnant: usize = chunks[last].iter().map(|u| u.tokens).sum();
        let previous: usize = chunks[last - 1].iter().map(|u| u.tokens).sum();
        if remnant < options.min_tokens && remnant + previous <= options.max_tokens {
            let tail = chunks.remove(last);
            let previous = chunks.len() - 1;
            for unit in tail {
                if !chunks[previous].iter().any(|u| std::ptr::eq(*u, unit)) {
                    chunks[previous].push(unit);
                }
            }
        }
    }

    chunks
        .into_iter()
        .map(|units| {
            units
                .iter()
                .map(|u| u.text.as_str())
                .collect::<Vec<_>>()
                .join("\n\n")
        })
        .collect()
}

/// The trailing units that fit in `budget`, in order. An atomic unit is never
/// duplicated into the overlap: a code block would be embedded twice and both
/// copies would rank for the same query.
///
/// A block is the unit of overlap, and a documentation paragraph routinely
/// costs more than the overlap budget on its own — measured at 117 tokens
/// against a budget of 80 — so a budget applied strictly would mean no overlap
/// at all on ordinary pages. When nothing fits, the last block is carried
/// anyway, provided it is prose and no larger than half the ceiling.
fn tail<'a>(units: &[&'a Unit], budget: usize, ceiling: usize) -> Vec<&'a Unit> {
    let mut out: Vec<&Unit> = Vec::new();
    let mut tokens = 0;
    for unit in units.iter().rev() {
        if unit.atomic || tokens + unit.tokens > budget {
            break;
        }
        tokens += unit.tokens;
        out.push(unit);
    }
    if out.is_empty()
        && let Some(last) = units.last()
        && !last.atomic
        && last.tokens * 2 <= ceiling
    {
        out.push(last);
    }
    out.reverse();
    out
}

/// Splits a paragraph too large for one chunk. Only prose reaches here; a code
/// block and a table are kept whole however large they are.
fn split_prose(text: &str, max_tokens: usize) -> Vec<Unit> {
    let mut out = Vec::new();
    let mut current = String::new();
    let mut tokens = 0;
    for sentence in sentences(text) {
        let cost = estimate_tokens(sentence);
        if !current.is_empty() && tokens + cost > max_tokens {
            out.push(Unit {
                tokens,
                text: std::mem::take(&mut current),
                atomic: false,
            });
            tokens = 0;
        }
        current.push_str(sentence);
        tokens += cost;
    }
    if !current.trim().is_empty() {
        out.push(Unit {
            tokens,
            text: current,
            atomic: false,
        });
    }
    out
}

/// Sentence ends, keeping the delimiter and the space after it so the pieces
/// concatenate back to the input.
fn sentences(text: &str) -> Vec<&str> {
    let bytes = text.as_bytes();
    let mut out = Vec::new();
    let mut start = 0;
    let mut at = 0;
    while at < bytes.len() {
        if matches!(bytes[at], b'.' | b'!' | b'?') {
            let mut end = at + 1;
            while end < bytes.len() && bytes[end].is_ascii_whitespace() {
                end += 1;
            }
            if end > at + 1 || end == bytes.len() {
                out.push(&text[start..end]);
                start = end;
                at = end;
                continue;
            }
        }
        at += 1;
    }
    if start < text.len() {
        out.push(&text[start..]);
    }
    out
}

/// H2 and H3 open a section; H4 and below stay inside the one above, the rule
/// `liyasa_search::section` uses.
fn sections<'a>(document: &'a Document, title: &str) -> Vec<Section<'a>> {
    let mut out = vec![Section {
        anchor: String::new(),
        title: title.to_owned(),
        heading: None,
        blocks: Vec::new(),
    }];
    for node in &document.root.children {
        let Node::Block(block) = node else { continue };
        if let BlockKind::Heading { level, anchor } = &block.kind
            && matches!(level, 2 | 3)
        {
            let text = inline_text(&block.children);
            out.push(Section {
                anchor: anchor.clone(),
                heading: Some(format!("{} {text}", "#".repeat(*level as usize))),
                title: text,
                blocks: Vec::new(),
            });
            continue;
        }
        out.last_mut()
            .expect("the lead section is never popped")
            .blocks
            .push(block);
    }
    out.retain(|s| !s.blocks.is_empty());
    out
}

/// One block as Markdown, through the serializer an agent is served (§11.7),
/// so a chunk reads as the page reads.
fn render_block(block: &Block) -> String {
    let root = Block {
        id: block.id,
        explicit_id: None,
        kind: BlockKind::Document,
        origin: Default::default(),
        children: vec![Node::Block(block.clone())],
    };
    liyasa_markdown::render::markdown::render(&root)
        .trim_end()
        .to_owned()
}

fn inline_text(nodes: &[Node]) -> String {
    let mut out = String::new();
    for node in nodes {
        match node {
            Node::Inline(inline) => push_inline(inline, &mut out),
            Node::Block(block) => out.push_str(&inline_text(&block.children)),
        }
    }
    out.trim().to_owned()
}

fn push_inline(inline: &Inline, out: &mut String) {
    match inline {
        Inline::Text(text) | Inline::Code(text) | Inline::Math(text) => out.push_str(text),
        Inline::Emph(children)
        | Inline::Strong(children)
        | Inline::Strike(children)
        | Inline::Link { children, .. }
        | Inline::InlineComponent { children, .. } => {
            for child in children {
                push_inline(child, out);
            }
        }
        Inline::SoftBreak | Inline::HardBreak => out.push(' '),
        Inline::Image { alt, .. } => out.push_str(alt),
        Inline::HtmlInline(_) | Inline::FootnoteRef(_) | Inline::TemplateInline { .. } => {}
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn prose_costs_about_a_token_every_four_bytes() {
        assert_eq!(estimate_tokens("hello"), 2);
        assert_eq!(estimate_tokens(""), 0);
        assert_eq!(estimate_tokens("a b c d"), 4);
    }

    #[test]
    fn punctuation_inside_a_word_costs_extra() {
        // `get_user` is eight bytes, so two tokens, plus one for the run.
        assert_eq!(estimate_tokens("get_user"), 3);
        // A run counts once, not per character.
        assert_eq!(estimate_tokens("a::b"), estimate_tokens("a:b"));
    }

    #[test]
    fn cjk_costs_a_token_a_character_not_a_token_every_four_bytes() {
        // Three characters, nine bytes. The byte rule would say three; the
        // point is that it is not undercounted, and kana counts the same.
        assert_eq!(estimate_tokens("日本語"), 3);
        assert_eq!(estimate_tokens("ひらがな"), 4);
    }

    #[test]
    fn sentences_concatenate_back_to_the_input() {
        let text = "One. Two! Three? Four";
        assert_eq!(sentences(text).concat(), text);
        assert_eq!(sentences(text).len(), 4);
    }

    #[test]
    fn a_decimal_point_does_not_end_a_sentence() {
        let text = "Version 1.5 ships. Next.";
        assert_eq!(sentences(text).len(), 2);
    }
}
