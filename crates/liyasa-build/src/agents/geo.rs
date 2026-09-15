//! Generative engine optimization (MCP-31).
//!
//! A generative engine quotes what it can lift cleanly: a typed record of what
//! the page is, an answer in the first sentence under each heading, an anchor
//! that still resolves next quarter, and a question-and-answer block it can
//! read as one. The checklist is what `liyasa score` prints; the JSON-LD is
//! what the theme embeds.

use liyasa_components::text;
use liyasa_core::document::{Block, BlockKind, Document, Node};
use serde_json::{Value, json};

use crate::agents::site::{PageRecord, SiteInput};

/// A first paragraph longer than this is not an answer, it is a preamble.
const ANSWER_WORDS: usize = 60;

/// The share of headings that must carry an author-written anchor.
const STABLE_ANCHOR_SHARE: f64 = 0.8;

/// Openings that delay the answer. A section that starts with one of these is
/// telling the reader what it is about to say instead of saying it.
const PREAMBLES: [&str; 8] = [
    "in this section",
    "in this guide",
    "this page describes",
    "this page explains",
    "this section describes",
    "this section explains",
    "before we begin",
    "let's take a look",
];

/// One line of the GEO checklist.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Item {
    pub id: &'static str,
    pub title: &'static str,
    pub passed: bool,
    pub detail: String,
}

#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct Checklist {
    pub items: Vec<Item>,
}

impl Checklist {
    pub fn passed(&self) -> usize {
        self.items.iter().filter(|item| item.passed).count()
    }

    /// The share of items that pass, which is what the score reports.
    pub fn score(&self) -> f64 {
        if self.items.is_empty() {
            return 1.0;
        }
        self.passed() as f64 / self.items.len() as f64
    }

    pub fn failures(&self) -> impl Iterator<Item = &Item> {
        self.items.iter().filter(|item| !item.passed)
    }

    fn push(&mut self, id: &'static str, title: &'static str, passed: bool, detail: String) {
        self.items.push(Item {
            id,
            title,
            passed,
            detail,
        });
    }
}

/// Runs the checklist over one page.
pub fn checklist(site: &SiteInput, page: &PageRecord, doc: &Document) -> Checklist {
    let mut out = Checklist::default();

    out.push(
        "structured-data",
        "The page carries JSON-LD",
        true,
        format!("`{}` is emitted for every page", article_type(page)),
    );

    let sections = sections(&doc.root);
    let slow: Vec<&str> = sections
        .iter()
        .filter(|section| !section.answers_first())
        .map(|section| section.heading.as_str())
        .collect();
    out.push(
        "answer-first",
        "Each section answers in its first paragraph",
        slow.is_empty(),
        if slow.is_empty() {
            format!("{} section(s) open with the answer", sections.len())
        } else {
            format!("these open with a preamble: {}", slow.join(", "))
        },
    );

    let (explicit, total) = anchors(&doc.root);
    let share = if total == 0 {
        1.0
    } else {
        explicit as f64 / total as f64
    };
    out.push(
        "stable-anchors",
        "Headings carry anchors that survive an edit",
        share >= STABLE_ANCHOR_SHARE,
        format!("{explicit} of {total} headings carry an explicit `{{#id}}`"),
    );

    let questions = faq_entries(&doc.root);
    out.push(
        "faq-schema",
        "Question-and-answer blocks are typed as `FAQPage`",
        true,
        if questions.is_empty() {
            "no question-and-answer block on this page".to_owned()
        } else {
            format!("{} question(s) typed as `FAQPage`", questions.len())
        },
    );

    let described = page
        .description
        .as_deref()
        .is_some_and(|text| !text.trim().is_empty());
    out.push(
        "page-summary",
        "The page states what it is in one line",
        described,
        if described {
            "`description` is set".to_owned()
        } else {
            "add a `description` to the front matter; it is the snippet an engine quotes".to_owned()
        },
    );

    let canonical = site.origin.page_url(&page.route);
    out.push(
        "canonical-url",
        "The page has one absolute canonical URL",
        canonical.starts_with("https://"),
        canonical,
    );

    out
}

/// The JSON-LD a page carries: the article record, and an `FAQPage` when the
/// page has question-and-answer content.
pub fn json_ld(site: &SiteInput, page: &PageRecord, doc: &Document) -> Vec<Value> {
    let url = site.origin.page_url(&page.route);
    let mut out = vec![json!({
        "@context": "https://schema.org",
        "@type": article_type(page),
        "headline": page.title,
        "description": page.description.clone().unwrap_or_default(),
        "url": url,
        "mainEntityOfPage": { "@type": "WebPage", "@id": url },
        "inLanguage": page.locale.as_str(),
        "isPartOf": {
            "@type": "WebSite",
            "name": site.name,
            "url": site.origin.base(),
        },
        "dateModified": page.updated.clone().unwrap_or_default(),
        "encoding": {
            "@type": "MediaObject",
            "encodingFormat": "text/markdown",
            "contentUrl": site.origin.markdown_url(&page.route),
        },
    })];

    let questions = faq_entries(&doc.root);
    if !questions.is_empty() {
        out.push(json!({
            "@context": "https://schema.org",
            "@type": "FAQPage",
            "mainEntity": questions
                .iter()
                .map(|entry| json!({
                    "@type": "Question",
                    "name": entry.question,
                    "acceptedAnswer": { "@type": "Answer", "text": entry.answer },
                }))
                .collect::<Vec<_>>(),
        }));
    }
    out
}

/// The JSON-LD as one pretty-printed document, for the theme to embed.
pub fn json_ld_text(site: &SiteInput, page: &PageRecord, doc: &Document) -> String {
    let blocks = json_ld(site, page, doc);
    let value = if blocks.len() == 1 {
        blocks.into_iter().next().unwrap_or(Value::Null)
    } else {
        Value::Array(blocks)
    };
    serde_json::to_string_pretty(&value).unwrap_or_else(|_| "{}".to_owned())
}

fn article_type(page: &PageRecord) -> &'static str {
    if page.changelog {
        "Article"
    } else {
        "TechArticle"
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct FaqEntry {
    pub question: String,
    pub answer: String,
}

struct Section {
    heading: String,
    opening: String,
}

impl Section {
    /// Whether the section says something before it says what it is about to
    /// say.
    fn answers_first(&self) -> bool {
        if self.opening.is_empty() {
            return false;
        }
        let lower = self.opening.to_ascii_lowercase();
        if PREAMBLES.iter().any(|opener| lower.starts_with(opener)) {
            return false;
        }
        first_paragraph_words(&self.opening) <= ANSWER_WORDS
    }
}

fn first_paragraph_words(text: &str) -> usize {
    text.split_whitespace().count()
}

/// Every heading with the prose that immediately follows it.
fn sections(root: &Block) -> Vec<Section> {
    let mut out: Vec<Section> = Vec::new();
    walk(root, &mut |block| match &block.kind {
        BlockKind::Heading { level, .. } if *level >= 2 => {
            out.push(Section {
                heading: text::of(&block.children).trim().to_owned(),
                opening: String::new(),
            });
        }
        BlockKind::Paragraph => {
            if let Some(section) = out.last_mut()
                && section.opening.is_empty()
            {
                section.opening = text::of(&block.children).trim().to_owned();
            }
        }
        _ => {}
    });
    out
}

/// `(headings with an author-written anchor, headings)`.
fn anchors(root: &Block) -> (usize, usize) {
    let (mut explicit, mut total) = (0, 0);
    walk(root, &mut |block| {
        if matches!(&block.kind, BlockKind::Heading { level, .. } if *level >= 2) {
            total += 1;
            if block.explicit_id.is_some() {
                explicit += 1;
            }
        }
    });
    (explicit, total)
}

/// Question-and-answer pairs: a disclosure component's title and body, or a
/// heading that asks a question with the prose that answers it.
pub fn faq_entries(root: &Block) -> Vec<FaqEntry> {
    let mut out = Vec::new();
    let mut pending: Option<String> = None;
    walk(root, &mut |block| match &block.kind {
        BlockKind::Component { name, props, .. } if is_disclosure(name) => {
            let question = props
                .get("title")
                .and_then(prop_text)
                .unwrap_or_else(|| text::of(&block.children));
            let answer = text::of(&block.children).trim().to_owned();
            if !question.trim().is_empty() && !answer.is_empty() {
                out.push(FaqEntry {
                    question: question.trim().to_owned(),
                    answer,
                });
            }
        }
        BlockKind::Heading { level, .. } if *level >= 2 => {
            let text = text::of(&block.children).trim().to_owned();
            pending = text.ends_with('?').then_some(text);
        }
        BlockKind::Paragraph => {
            if let Some(question) = pending.take() {
                let answer = text::of(&block.children).trim().to_owned();
                if !answer.is_empty() {
                    out.push(FaqEntry { question, answer });
                }
            }
        }
        _ => {}
    });
    out
}

fn is_disclosure(name: &str) -> bool {
    matches!(name, "accordion" | "disclosure" | "faq" | "details")
}

fn prop_text(value: &liyasa_core::document::PropValue) -> Option<String> {
    match value {
        liyasa_core::document::PropValue::Str(text) => Some(text.clone()),
        _ => None,
    }
}

/// Depth-first, parents before children, so a heading is seen before the
/// paragraph it introduces.
fn walk(block: &Block, visit: &mut impl FnMut(&Block)) {
    visit(block);
    for child in &block.children {
        if let Node::Block(inner) = child {
            walk(inner, visit);
        }
    }
}

#[cfg(test)]
mod tests {
    use liyasa_components::{inst, nodes};
    use liyasa_core::diagnostics::Diagnostics;
    use liyasa_core::document::{Origin, PropValue};
    use liyasa_core::ids::{BlockId, Locale, Route};

    use super::*;
    use crate::agents::site::{AgentsSettings, CanonicalOrigin, FeedsSettings};

    fn document(children: Vec<Node>) -> Document {
        Document {
            root: Block {
                id: BlockId::implicit("document", "", "", 0),
                explicit_id: None,
                kind: BlockKind::Document,
                origin: Origin::default(),
                children,
            },
            deps: Default::default(),
            diagnostics: Diagnostics::new(),
        }
    }

    fn heading_with_id(level: u8, text: &str, id: &str) -> Node {
        let Node::Block(mut block) = nodes::heading(level, text) else {
            unreachable!("heading builds a block")
        };
        block.explicit_id = Some(id.to_owned());
        Node::Block(block)
    }

    fn page() -> PageRecord {
        PageRecord {
            id: None,
            route: Route::new("/guide/install"),
            title: "Install".to_owned(),
            description: Some("Install Liyasa in under a minute.".to_owned()),
            locale: Locale::new("en"),
            version: None,
            tab: None,
            group: None,
            indexable: true,
            personalized: false,
            markdown: "# Install\n".to_owned(),
            updated: Some("2026-09-15".to_owned()),
            changelog: false,
        }
    }

    fn site() -> SiteInput {
        SiteInput {
            name: "Liyasa".to_owned(),
            summary: Some("Documentation.".to_owned()),
            origin: CanonicalOrigin::parse("https://example.com").expect("a valid origin"),
            locale: Locale::new("en"),
            version: None,
            pages: vec![page()],
            nav: Vec::new(),
            agents: AgentsSettings::default(),
            feeds: FeedsSettings::default(),
        }
    }

    fn item<'a>(checklist: &'a Checklist, id: &str) -> &'a Item {
        checklist
            .items
            .iter()
            .find(|item| item.id == id)
            .unwrap_or_else(|| panic!("{id}"))
    }

    #[test]
    fn mcp_31_the_page_record_is_typed_json_ld() {
        let doc = document(vec![nodes::paragraph("Run the installer.")]);
        let blocks = json_ld(&site(), &page(), &doc);
        assert_eq!(blocks.len(), 1);
        assert_eq!(blocks[0]["@context"], "https://schema.org");
        assert_eq!(blocks[0]["@type"], "TechArticle");
        assert_eq!(blocks[0]["url"], "https://example.com/guide/install");
        assert_eq!(
            blocks[0]["encoding"]["contentUrl"],
            "https://example.com/guide/install.md"
        );
        assert_eq!(blocks[0]["dateModified"], "2026-09-15");
    }

    #[test]
    fn mcp_31_a_changelog_page_is_an_article() {
        let mut page = page();
        page.changelog = true;
        let doc = document(vec![nodes::paragraph("Shipped.")]);
        assert_eq!(json_ld(&site(), &page, &doc)[0]["@type"], "Article");
    }

    #[test]
    fn mcp_31_a_question_heading_becomes_an_faq_entry() {
        let doc = document(vec![
            nodes::heading(2, "Does Liyasa need a database?"),
            nodes::paragraph("No. A static build writes files and nothing else."),
        ]);
        let blocks = json_ld(&site(), &page(), &doc);
        assert_eq!(blocks.len(), 2);
        assert_eq!(blocks[1]["@type"], "FAQPage");
        let entities = blocks[1]["mainEntity"].as_array().expect("entities");
        assert_eq!(entities[0]["name"], "Does Liyasa need a database?");
        assert_eq!(
            entities[0]["acceptedAnswer"]["text"],
            "No. A static build writes files and nothing else."
        );
    }

    #[test]
    fn mcp_31_a_disclosure_becomes_an_faq_entry() {
        let doc = document(vec![nodes::component(
            inst::new("accordion")
                .prop("title", PropValue::Str("Can I self-host it?".to_owned()))
                .child(nodes::paragraph("Yes; the binary is the whole runtime."))
                .build(),
        )]);
        let entries = faq_entries(&doc.root);
        assert_eq!(entries.len(), 1);
        assert_eq!(entries[0].question, "Can I self-host it?");
        assert!(entries[0].answer.contains("the whole runtime"));
    }

    #[test]
    fn mcp_31_a_page_with_no_questions_carries_no_faq_schema() {
        let doc = document(vec![
            nodes::heading(2, "Installing"),
            nodes::paragraph("Run the installer."),
        ]);
        assert_eq!(json_ld(&site(), &page(), &doc).len(), 1);
    }

    #[test]
    fn mcp_31_a_section_that_opens_with_a_preamble_fails_answer_first() {
        let doc = document(vec![
            nodes::heading(2, "Configuration"),
            nodes::paragraph("In this section we will look at the configuration keys."),
        ]);
        let checklist = checklist(&site(), &page(), &doc);
        let answer = item(&checklist, "answer-first");
        assert!(!answer.passed, "{answer:?}");
        assert!(answer.detail.contains("Configuration"), "{answer:?}");
    }

    #[test]
    fn mcp_31_a_section_that_opens_with_the_answer_passes() {
        let doc = document(vec![
            nodes::heading(2, "Configuration"),
            nodes::paragraph("Every key lives in `liyasa.json` at the project root."),
        ]);
        assert!(item(&checklist(&site(), &page(), &doc), "answer-first").passed);
    }

    #[test]
    fn mcp_31_a_long_opening_paragraph_is_not_an_answer() {
        let doc = document(vec![
            nodes::heading(2, "Configuration"),
            nodes::paragraph(&"word ".repeat(ANSWER_WORDS + 5)),
        ]);
        assert!(!item(&checklist(&site(), &page(), &doc), "answer-first").passed);
    }

    #[test]
    fn mcp_31_headings_without_explicit_anchors_fail_the_stability_check() {
        let doc = document(vec![
            nodes::heading(2, "One"),
            nodes::heading(2, "Two"),
            nodes::heading(2, "Three"),
        ]);
        let checklist = checklist(&site(), &page(), &doc);
        let anchors = item(&checklist, "stable-anchors");
        assert!(!anchors.passed, "{anchors:?}");
        assert!(anchors.detail.contains("0 of 3"), "{anchors:?}");
    }

    #[test]
    fn mcp_31_explicit_anchors_pass_the_stability_check() {
        let doc = document(vec![
            heading_with_id(2, "One", "one"),
            heading_with_id(2, "Two", "two"),
            heading_with_id(2, "Three", "three"),
        ]);
        assert!(item(&checklist(&site(), &page(), &doc), "stable-anchors").passed);
    }

    #[test]
    fn mcp_31_a_page_without_a_description_fails_the_summary_check() {
        let mut page = page();
        page.description = None;
        let doc = document(vec![nodes::paragraph("Run the installer.")]);
        assert!(!item(&checklist(&site(), &page, &doc), "page-summary").passed);
    }

    #[test]
    fn mcp_31_the_checklist_scores_between_zero_and_one() {
        let doc = document(vec![
            heading_with_id(2, "Configuration", "configuration"),
            nodes::paragraph("Every key lives in `liyasa.json`."),
        ]);
        let checklist = checklist(&site(), &page(), &doc);
        assert_eq!(checklist.score(), 1.0);
        assert_eq!(checklist.failures().count(), 0);
        assert_eq!(checklist.items.len(), 6);
    }

    #[test]
    fn mcp_31_the_json_ld_text_is_one_document_per_page() {
        let doc = document(vec![
            nodes::heading(2, "Is it free?"),
            nodes::paragraph("Yes."),
        ]);
        let text = json_ld_text(&site(), &page(), &doc);
        let value: Value = serde_json::from_str(&text).expect("valid JSON");
        assert!(value.is_array(), "{text}");
    }
}
