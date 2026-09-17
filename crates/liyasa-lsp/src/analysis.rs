//! One buffer, run through the build's own page pipeline.
//!
//! This is the whole of what the server knows about a file, and every feature
//! reads it rather than re-deriving anything: diagnostics are the ones the
//! pipeline raised, completion and hover index into the Source Document, and
//! the preview is the HTML the pipeline already produced.
//!
//! It runs `liyasa_build::render::page` rather than expanding and parsing here,
//! deliberately. An editor that disagrees with the build about whether a page
//! is valid is worse than one that says nothing, and two pipelines drift.
//!
//! **Undefined names are lenient.** `Options::new` is strict, which raises
//! `E0201` for every name the context does not hold. An editor's context is not
//! a build's — `site.*`, `nav.*` and `env.*` are filled by the build engine and
//! this server cannot reach them — so strict mode here would report
//! undefined-variable errors the build never raises. `anonymous()` is the
//! lenient render §6.6.4 already defines, and a live buffer is exactly that.

use std::sync::Arc;

use liyasa_build::render::{self, Options};
use liyasa_core::diagnostics::Diagnostic;
use liyasa_core::document::{Document, SourceDocument};
use liyasa_core::markdown::HtmlMode;
use liyasa_core::source_map::SourceMap;
use liyasa_core::span::SourceId;
use liyasa_core::vfs::VfsPath;
use liyasa_markdown::source::context::Layers;
use liyasa_markdown::source::{normalize, scan};

use crate::text::Text;
use crate::workspace::Workspace;

pub struct Analysis {
    pub text: Text,
    pub source: SourceId,
    /// The Source Document of §7.16: every feature's index into the buffer.
    pub document: SourceDocument,
    /// The Rendered AST, absent when expansion failed outright — a template
    /// syntax error leaves nothing to parse, and the diagnostics say why.
    pub parsed: Option<Document>,
    /// The page as the build would serve it. Empty when there is no AST.
    pub html: String,
    pub diagnostics: Vec<Diagnostic>,
}

impl Analysis {
    /// Analyses one buffer. `path` is only the name the source map interns; a
    /// buffer the editor has never saved may pass any stable string.
    pub fn of(path: &str, raw: &str, workspace: &Workspace) -> Self {
        let normalized: Arc<str> = Arc::from(normalize(raw).into_owned());
        let mut sources = SourceMap::new();
        let source = sources.intern(VfsPath::new(path), Arc::clone(&normalized));

        let (document, scanned) = scan(&normalized, source);
        let mut diagnostics = scanned.into_vec();

        // No link resolution: the AST is rendered as written, which is what a
        // preview of one page wants and what the pipeline documents `None` as.
        let options = Options::new(&workspace.registry, &workspace.site)
            .anonymous()
            .html_mode(HtmlMode::Sanitize);
        let context = layers(workspace, &document).build();
        let page = render::page(&sources, &document, &context, &options);
        diagnostics.extend(page.diagnostics.into_vec());

        Self {
            text: Text::new(normalized.to_string()),
            source,
            document,
            parsed: page.document,
            html: page.html,
            diagnostics,
        }
    }

    /// The segment a byte offset falls in. An offset on the boundary between
    /// two segments belongs to the one that starts there, which is what an
    /// author means by typing at the start of a directive.
    pub fn segment_at(&self, offset: u32) -> Option<usize> {
        self.document
            .segments
            .iter()
            .position(|segment| {
                let span = segment.span();
                span.start <= offset && offset <= span.end
            })
            .or_else(|| {
                self.document
                    .segments
                    .iter()
                    .position(|segment| segment.span().contains(offset))
            })
    }
}

/// The layers a live buffer can fill: the site's variables, the project's
/// facts, and the page's own front matter. The rest belong to a build.
fn layers(workspace: &Workspace, document: &SourceDocument) -> Layers {
    let mut site_variables = serde_json::Map::new();
    for (path, value) in &workspace.variables {
        if !path.contains('.') {
            site_variables.insert(path.clone(), value.clone());
        }
    }

    let mut facts = serde_json::Map::new();
    for (path, fact) in &workspace.facts {
        if !path.contains('.') {
            facts.insert(path.clone(), fact.value.clone());
        }
    }

    Layers {
        site_variables: serde_json::Value::Object(site_variables),
        facts: serde_json::Value::Object(facts),
        page: front_matter(document),
        ..Layers::default()
    }
}

/// The scanner has already parsed the front matter and kept the value; there is
/// nothing here to parse a second time.
fn front_matter(document: &SourceDocument) -> serde_json::Value {
    document
        .frontmatter
        .as_ref()
        .map_or(serde_json::Value::Null, |front| front.value.clone())
}
