//! OpenAPI operations as structured documents (AST-03).
//!
//! An operation is indexed from the endpoint page the build already produced,
//! rendered through `liyasa_openapi::markdown` with the schemas included, so a
//! retrieved chunk carries the exact field names and status codes an answer
//! must cite rather than a prose summary of them. That is also the reason the
//! renderer is called rather than a second one written here: an answer that
//! cites `expires_at` because the index says so, while the page says
//! `expiresAt`, is the failure this requirement exists to prevent.
//!
//! Splitting is at the document's own second-level headings — `Parameters`,
//! `Request body`, `Responses` — and every piece keeps the operation's title
//! and `METHOD /path` line, so a fragment retrieved alone still says which
//! operation it belongs to.

use liyasa_openapi::markdown;
use liyasa_openapi::page::Page;

use crate::chunk::{Chunk, ChunkOptions, PageContext, split_markdown};
use crate::index::{ChunkKind, ChunkRecord};

/// The facts an answer cites by name, kept beside the prose so a query for a
/// status code or a parameter matches the operation that has it.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct OperationFacts {
    pub method: String,
    pub path: String,
    pub parameters: Vec<String>,
    /// Status codes as the spec writes them: `"200"`, `"default"`.
    pub responses: Vec<String>,
}

impl OperationFacts {
    pub fn of(page: &Page) -> Self {
        Self {
            method: page.method.as_str().to_owned(),
            path: page.path.clone(),
            parameters: page
                .parameters
                .iter()
                .flat_map(|section| section.fields.iter())
                .map(|field| field.name.clone())
                .collect(),
            responses: page
                .responses
                .iter()
                .map(|response| response.status.clone())
                .collect(),
        }
    }
}

/// The header every chunk of one operation repeats.
fn preamble(page: &Page, facts: &OperationFacts) -> String {
    let mut out = format!("# {}\n\n`{} {}`", page.title, facts.method, facts.path);
    if page.deprecated {
        out.push_str("\n\n**Deprecated.**");
    }
    if let Some(summary) = page.summary.as_ref().filter(|s| !s.is_empty()) {
        out.push_str(&format!("\n\n{summary}"));
    }
    out
}

/// Chunks one operation.
pub fn chunks(page: &Page, options: &ChunkOptions) -> Vec<Chunk> {
    let facts = OperationFacts::of(page);
    let rendered = markdown::render(
        page,
        &markdown::Options {
            // An answer that names a field must have been shown the field.
            include_schema: true,
            include_samples: true,
        },
    );
    // The renderer's own title and method line are in the preamble already.
    let body = strip_header(&rendered);
    split_markdown(&preamble(page, &facts), &body, "", &page.title, options)
}

/// Everything up to the first second-level heading is the renderer's header,
/// which [`preamble`] restates.
fn strip_header(rendered: &str) -> String {
    match rendered.find("\n## ") {
        Some(at) => rendered[at + 1..].to_owned(),
        None => rendered
            .lines()
            .skip_while(|line| !line.is_empty())
            .collect::<Vec<_>>()
            .join("\n"),
    }
}

/// One operation as index rows, with `route` taken from the endpoint page.
pub fn records(page: &Page, context: &PageContext, options: &ChunkOptions) -> Vec<ChunkRecord> {
    chunks(page, options)
        .iter()
        .map(|chunk| ChunkRecord::from_chunk(chunk, context, ChunkKind::Operation))
        .collect()
}
