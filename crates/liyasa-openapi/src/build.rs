//! What a spec contributes to a build (API-03, API-04, API-10, API-14, API-15).
//!
//! Defect 151: this crate could read, normalize, overlay and filter a spec, and
//! no build called any of it, so a site that declared `openapi` had its config
//! checked and generated nothing. [`crate::download`] fixed the first half —
//! the processed document at `/openapi/<id>.json`. This is the second: the
//! pages.
//!
//! A page here is a **whole Markdown source document**, front matter and all,
//! and that is the design decision worth knowing about. The engine's pipeline
//! starts from files in the content tree, and everything a page gets on the way
//! through — HTML, the Markdown twin, the search document, the manifest row,
//! the sitemap entry, the link check, the access chain — it gets by being one
//! of those files. A synthetic page that arrived as finished HTML would have to
//! be given each of those separately, and would drift from an authored page
//! every time one of them changed. So a generated page is spelled the way an
//! author would spell it, and the pipeline cannot tell the difference.
//!
//! The front matter carries `openapi: "<id> <METHOD> <path>"`, which the engine
//! already reads: it is what makes a page an endpoint in the search index. A
//! page an author wrote with that key is not duplicated — it *is* that
//! operation's page, keeps its own route, and its body renders above the
//! parameters (API-04).
//!
//! See `plan/rfcs/0806-generated-pages-enter-the-pipeline.md` for the call site
//! this needs, which is WP-06's.
// TODO(rfc-0806): nothing calls this yet. The pages exist and are tested; the
// build that writes them is one call in `liyasa-build`'s engine.

use liyasa_core::diagnostics::{Diagnostic, Diagnostics, code};
use liyasa_core::vfs::{Vfs, VfsPath};

use crate::config::SpecConfig;
use crate::field::Field;
use crate::model::ParameterIn;
use crate::nav::{self, Entry, Node, Reference};
use crate::page::{Augmentation, BuildOptions, Page, Rendered, Section};
use crate::schemas::{self, SchemaPage};
use crate::source::Location;
use crate::visibility::Audience;
use crate::{Spec, markdown, normalize, read, tree, version};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PageKind {
    Operation,
    Schema,
}

/// One page a spec contributes, as the engine wants it.
#[derive(Debug, Clone)]
pub struct SpecPage {
    pub spec: String,
    pub kind: PageKind,
    /// The route, with a leading slash.
    pub route: String,
    pub title: String,
    /// `GET /widgets/{id}` for an operation, the component name for a schema.
    pub selector: String,
    /// A complete Markdown document: `---` front matter, then the body.
    pub source: String,
    pub deprecated: bool,
    /// The navigation group this page belongs under, when it has one.
    pub group: Option<String>,
}

/// A page already in the content tree whose front matter names an operation
/// (API-04).
#[derive(Debug, Clone)]
pub struct Authored {
    pub route: String,
    /// The front matter's value: `"api GET /widgets/{id}"`.
    pub selector: String,
    /// The page's own Markdown body, which renders above the parameters.
    pub body: String,
}

/// The `:::slot{name="after-params"}` blocks one authored page defines (API-04).
///
/// Beside [`Authored`] rather than a field on it, deliberately: `Authored` is
/// constructed by a struct literal in `liyasa-build`'s engine, so a fourth field
/// would stop that crate compiling the moment this one gained it, and the fix
/// would be in a file this package may not write. An added type and an added
/// entry point break nothing, and the two collapse into one when the call site
/// is ready.
#[derive(Debug, Clone, Default)]
pub struct AuthoredSlots {
    /// The authored page's route, matching [`Authored::route`].
    pub route: String,
    /// Slot name to Markdown. A name outside `page::SLOTS` is dropped; saying so
    /// belongs to whoever parsed the block, with the list of names that exist.
    pub slots: Vec<(String, String)>,
}

#[derive(Debug, Default)]
pub struct Surface {
    pub pages: Vec<SpecPage>,
    /// One per spec, for the `{ openapi: "id" }` navigation node to expand to.
    pub navigations: Vec<Reference>,
    pub diagnostics: Diagnostics,
}

/// Every page and navigation entry the site's specs contribute.
///
/// Mirrors [`crate::download`]'s inputs so one call at the same point in the
/// engine serves both.
pub fn surface(vfs: &dyn Vfs, config: &serde_json::Value, authored: &[Authored]) -> Surface {
    surface_with_slots(vfs, config, authored, &[])
}

/// [`surface`], plus the slots each authored page fills (API-04).
pub fn surface_with_slots(
    vfs: &dyn Vfs,
    config: &serde_json::Value,
    authored: &[Authored],
    slots: &[AuthoredSlots],
) -> Surface {
    let mut out = Surface::default();
    let (specs, problems) = crate::config::specs(config.get("openapi"));
    for problem in problems {
        out.diagnostics.push(
            Diagnostic::new(code::E0133, problem)
                .help("check the `openapi` array against schemas/liyasa.schema.json"),
        );
    }
    for spec in &specs {
        let siblings: Vec<&str> = specs
            .iter()
            .filter(|other| other.id != spec.id)
            .map(|other| other.source.as_str())
            .collect();
        let Some(model) = processed(vfs, spec, &siblings, &mut out.diagnostics) else {
            continue;
        };
        let one = one(&model, spec, authored, slots);
        out.pages.extend(one.pages);
        out.navigations.push(one.navigation);
        out.diagnostics.extend(one.diagnostics.as_slice().to_vec());
    }
    out.pages.sort_by(|a, b| a.route.cmp(&b.route));
    out
}

/// Loads one spec, applies its overlays, normalizes it, and filters it to what
/// anyone may see.
///
/// `Audience::public()` for the same reason the download uses it: these pages
/// are written to disk and a static host hands them to every visitor, so they
/// may hold only what every visitor may have. Per-reader filtering is API-52
/// and belongs at request time.
pub fn processed(
    vfs: &dyn Vfs,
    spec: &SpecConfig,
    // The other declared specs' sources, which a `$ref` may not reach (API-07).
    siblings: &[&str],
    diagnostics: &mut Diagnostics,
) -> Option<Spec> {
    let path = match Location::parse(&spec.source) {
        Location::File(path) => path,
        // Which URLs a build may fetch depends on the deploy branch's config,
        // not on the branch being built (CFG-95). Saying so beats a silent
        // omission, which looks exactly like a spec that produced nothing.
        Location::Remote(url) => {
            diagnostics.push(
                Diagnostic::new(
                    code::W0131,
                    format!("`{url}` is remote, so `{}` generates no pages", spec.id),
                )
                .help("point `openapi[].source` at a file in the project to render it"),
            );
            return None;
        }
    };

    let bytes = match vfs.read(&path) {
        Ok(bytes) => bytes,
        Err(error) => {
            diagnostics.push(
                Diagnostic::new(
                    code::E0002,
                    format!("`{}` could not be read: {error:?}", spec.source),
                )
                .help("check `openapi[].source` against the files in the project"),
            );
            return None;
        }
    };

    let mut root = match tree::parse(&bytes, &path.to_string()) {
        Ok(value) => value,
        Err(error) => {
            diagnostics.push(*error);
            return None;
        }
    };

    for overlay in overlays(spec) {
        apply_overlay(vfs, spec, &overlay, &mut root, diagnostics);
    }

    // Overlays edit the document the author wrote, so they land before
    // normalization; everything downstream then sees one shape (API-01).
    let dialect = match version::detect(&root) {
        Ok(dialect) => dialect,
        Err(error) => {
            diagnostics.push(*error);
            return None;
        }
    };
    diagnostics.extend(normalize::to_3_1(&mut root, &dialect).as_slice().to_vec());

    let documents = local_documents(vfs, &path.to_string(), root, siblings, diagnostics);
    let mut reader = read::Reader::new(&documents);
    let mut model = reader.spec(&spec.id, dialect);
    diagnostics.extend(reader.into_diagnostics().as_slice().to_vec());

    crate::visibility::filter(&mut model, &Audience::public());
    Some(model)
}

fn apply_overlay(
    vfs: &dyn Vfs,
    spec: &SpecConfig,
    overlay: &str,
    root: &mut tree::Value,
    diagnostics: &mut Diagnostics,
) {
    let Ok(bytes) = vfs.read(&VfsPath::new(overlay)) else {
        // A discovered overlay that is not there is the ordinary case: the
        // convention is `<spec>.overlay.yaml` and most specs have none. Only a
        // CONFIGURED one that is missing is worth saying.
        if spec.overlays.iter().any(|named| named == overlay) {
            diagnostics.push(
                Diagnostic::new(
                    code::E0002,
                    format!("overlay `{overlay}` could not be read"),
                )
                .help("check `openapi[].overlays` against the files in the project"),
            );
        }
        return;
    };
    match tree::parse(&bytes, overlay) {
        Ok(document) => match crate::overlay::apply(root, &document, overlay) {
            Ok(applied) => diagnostics.extend(applied.as_slice().to_vec()),
            Err(error) => diagnostics.push(*error),
        },
        Err(error) => diagnostics.push(*error),
    }
}

/// Configured overlays, then the `<spec>.overlay.yaml` convention (API-06).
fn overlays(spec: &SpecConfig) -> Vec<String> {
    let mut out = spec.overlays.clone();
    if let Some(discovered) = spec.discovered_overlay()
        && !out.contains(&discovered)
    {
        out.push(discovered);
    }
    out
}

/// How many documents one spec may reach, mirroring [`crate::source`]'s cap.
const MAX_DOCUMENTS: usize = 512;

/// Every local document this spec reaches, so a spec split across a directory
/// of files renders (API-02).
///
/// A sync twin of [`crate::source::Fetcher::documents`], which is async because
/// it also fetches URLs. A page is built without an `HttpClient`, so a remote
/// `$ref` is refused here rather than fetched.
///
/// **A `$ref` into another declared spec is refused on purpose** (API-07):
/// cross-spec resolution is off by default, and a sibling spec's source happens
/// to be a readable local file, so without this check adding the traversal would
/// have silently turned that default off.
fn local_documents(
    vfs: &dyn Vfs,
    root_key: &str,
    root: tree::Value,
    siblings: &[&str],
    diagnostics: &mut Diagnostics,
) -> read::Documents {
    let mut documents = read::Documents::new(root_key.to_owned(), root.clone());
    let mut queue: Vec<(String, tree::Value)> = vec![(root_key.to_owned(), root)];
    let mut seen: Vec<String> = vec![root_key.to_owned()];

    while let Some((base, document)) = queue.pop() {
        for reference in crate::source::external_refs(&document) {
            let key = crate::refs::join(&base, &reference);
            if seen.contains(&key) {
                continue;
            }
            seen.push(key.clone());
            if seen.len() > MAX_DOCUMENTS {
                diagnostics.push(Diagnostic::new(
                    code::E0502,
                    format!("this spec reaches more than {MAX_DOCUMENTS} documents"),
                ));
                return documents;
            }
            if let Location::Remote(url) = Location::parse(&key) {
                diagnostics.push(
                    Diagnostic::new(
                        code::E0502,
                        format!("`{url}` is remote, so its `$ref` is not resolved here"),
                    )
                    .help("vendor the document beside the spec"),
                );
                continue;
            }
            if siblings.iter().any(|source| same_document(source, &key)) {
                diagnostics.push(
                    Diagnostic::new(
                        code::E0502,
                        format!("`{reference}` points at another declared spec"),
                    )
                    .help(
                        "cross-spec `$ref` is off by default; give the shared part its own \
                         document that both specs reference",
                    ),
                );
                continue;
            }
            let Ok(bytes) = vfs.read(&VfsPath::new(&key)) else {
                diagnostics.push(
                    Diagnostic::new(code::E0502, format!("`{reference}` could not be read"))
                        .help(format!("nothing is at `{key}` in the project")),
                );
                continue;
            };
            match tree::parse(&bytes, &key) {
                Ok(parsed) => {
                    documents.insert(key.clone(), parsed.clone());
                    queue.push((key, parsed));
                }
                Err(error) => diagnostics.push(*error),
            }
        }
    }
    documents
}

/// Whether two paths name one document, normalized so `./a.yaml` and `a.yaml`
/// are not two specs.
fn same_document(left: &str, right: &str) -> bool {
    VfsPath::new(left).to_string() == VfsPath::new(right).to_string()
}

struct One {
    pages: Vec<SpecPage>,
    navigation: Reference,
    diagnostics: Diagnostics,
}

fn one(model: &Spec, config: &SpecConfig, authored: &[Authored], slots: &[AuthoredSlots]) -> One {
    let node = Node {
        openapi: config.id.clone(),
        group_by: Some(config.group_by),
        operations: Some(config.operations.clone()),
        ..Node::default()
    };
    let (mut navigation, diagnostics) = nav::generate(model, config, &node);

    // An authored page is that operation's page. Its route replaces the
    // generated one everywhere, navigation included, so the operation has one
    // route rather than two (API-04).
    let mut overrides: Vec<(String, &Authored)> = Vec::new();
    for page in authored {
        let Some((id, selector)) = crate::page::parse_front_matter(&page.selector) else {
            continue;
        };
        if id != config.id {
            continue;
        }
        overrides.push((selector.to_owned(), page));
    }
    for group in &mut navigation.groups {
        for entry in &mut group.pages {
            if let Some((_, authored)) = overrides
                .iter()
                .find(|(selector, _)| *selector == entry.selector)
            {
                entry.route = authored.route.clone();
            }
        }
    }

    let registry = crate::codegen::Registry::new();
    let languages: Vec<String> = crate::codegen::DEFAULT_LANGUAGES
        .iter()
        .map(|name| (*name).to_owned())
        .collect();

    let mut pages = Vec::new();
    for group in &navigation.groups {
        for entry in &group.pages {
            // `x-liyasa.href` sends the navigation entry somewhere else, so
            // there is no page of ours to generate for it.
            if entry.href.is_some() {
                continue;
            }
            let Some(operation) = model.operation(entry.method, &entry.path) else {
                continue;
            };
            let augmentation = overrides
                .iter()
                .find(|(selector, _)| *selector == entry.selector)
                .map(|(_, authored)| Augmentation {
                    intro: Some(Rendered {
                        html: String::new(),
                        markdown: authored.body.clone(),
                    }),
                    slots: slots
                        .iter()
                        .find(|filled| filled.route == authored.route)
                        .map(|filled| filled.slots.as_slice())
                        .unwrap_or_default()
                        .iter()
                        .filter(|(name, _)| Augmentation::is_slot(name))
                        .map(|(name, markdown)| {
                            (
                                name.clone(),
                                Rendered {
                                    html: String::new(),
                                    markdown: markdown.clone(),
                                },
                            )
                        })
                        .collect(),
                })
                .unwrap_or_default();
            let page = Page::build(
                model,
                &operation,
                &registry,
                &BuildOptions {
                    route: entry.route.clone(),
                    languages: languages.clone(),
                    ..BuildOptions::default()
                },
            );
            let page = Page {
                augmentation,
                ..page
            };
            pages.push(SpecPage {
                spec: config.id.clone(),
                kind: PageKind::Operation,
                route: entry.route.clone(),
                title: page.title.clone(),
                selector: entry.selector.clone(),
                source: document(
                    &front_matter(&page.title, operation_key(&config.id, &entry.selector)),
                    &operation_body(
                        &page,
                        &config.id,
                        operation.operation.operation_id.as_deref(),
                    ),
                ),
                deprecated: entry.deprecated,
                group: Some(group.name.clone()),
            });
        }
    }

    if config.schema_pages {
        let entries: Vec<Entry> = navigation.entries().cloned().collect();
        let base = navigation.base.clone();
        for schema in schemas::pages(model, &base, &entries) {
            pages.push(SpecPage {
                spec: config.id.clone(),
                kind: PageKind::Schema,
                route: schema.route.clone(),
                title: schema.title.clone(),
                selector: schema.name.clone(),
                source: document(&front_matter(&schema.title, None), &schema_body(&schema)),
                deprecated: false,
                group: None,
            });
        }
    }

    One {
        pages,
        navigation,
        diagnostics,
    }
}

fn operation_key(id: &str, selector: &str) -> Option<String> {
    Some(format!("{id} {selector}"))
}

/// The front matter of a generated page, serialized rather than formatted, so a
/// title holding a colon or a quote cannot break the document.
fn front_matter(title: &str, openapi: Option<String>) -> tree::Value {
    let mut map = tree::Map::new();
    map.insert(tree::Value::from("title"), tree::Value::from(title));
    if let Some(openapi) = openapi {
        map.insert(
            tree::Value::from("openapi"),
            tree::Value::from(openapi.as_str()),
        );
    }
    tree::Value::Mapping(map)
}

fn document(front: &tree::Value, body: &str) -> String {
    let front = tree::to_yaml(front).unwrap_or_else(|_| String::from("{}\n"));
    format!("---\n{}\n---\n\n{}", front.trim_end_matches('\n'), body)
}

/// A schema page's body (API-15).
fn schema_body(page: &SchemaPage) -> String {
    let mut out = String::new();
    out.push_str(&format!("# {}\n\n", page.title));
    if let Some(description) = &page.description {
        out.push_str(&format!("{description}\n\n"));
    }
    out.push_str(&format!("{}\n\n", page.usage_summary()));
    if !page.used_by.is_empty() {
        for usage in &page.used_by {
            let place = match &usage.route {
                Some(route) => format!("[`{}`]({route})", usage.selector),
                None => format!("`{}`", usage.selector),
            };
            out.push_str(&format!("- {place} — {}\n", usage.place));
        }
        out.push('\n');
    }
    out.push_str("## Fields\n\n");
    // A schema's own row says only its name and type; a reader wants the
    // properties under it. A schema that has none — a named enum, a scalar
    // alias — has nothing underneath, so it is its own single row.
    let fields: &[Field] = if page.field.children.is_empty() {
        std::slice::from_ref(&page.field)
    } else {
        &page.field.children
    };
    out.push_str(&markdown::field_table("Field", fields));
    out
}

// ---- the body of a generated operation page ----

/// One operation as the components a manual page would use (API-10, API-11).
///
/// `liyasa-components` was built expecting this: `endpoint` takes `spec` and
/// `operation` so the header can be pulled from the document, and the doc
/// comment on `reject_body_location` says in as many words that "the generator
/// emits them as response-field rows under a Body heading". So a generated page
/// is spelled the way an author spells one, gets the method pill, the per-row
/// anchors and the table merge for free, and the Markdown twin (API-14) comes
/// from each component's own `render_markdown` rather than from a second
/// rendering of the page.
fn operation_body(page: &Page, spec: &str, operation_id: Option<&str>) -> String {
    let mut out = String::new();

    // `endpoint.method` is `one_of(&["get", "post", ...])` — the LOWERCASE set.
    // The component upper-cases it for the pill; the prop is the spec's value.
    let mut props = vec![
        ("method", Prop::Text(page.method.lowercase().to_owned())),
        ("path", Prop::Text(page.path.clone())),
        ("spec", Prop::Text(spec.to_owned())),
    ];
    if let Some(id) = operation_id {
        props.push(("operation", Prop::Text(id.to_owned())));
    }
    // The prose goes INSIDE the endpoint container, not after it. Two reasons,
    // and the second is the one that forces it.
    //
    // `Endpoint::html` and `Endpoint::markdown` both end with
    // `ctx.children(&inst.children)`, so the component was built to hold the
    // description and emitting it as a sibling paragraph was always slightly
    // wrong.
    //
    // And an EMPTY container is `E0317`: `ast/build.rs` cannot tell `:::endpoint`
    // with no body from a leaf `::endpoint` except by the span it occupies, so a
    // container written empty is read as a leaf and the kind mismatch is
    // reported. `endpoint` is declared `kind = Container`, so it must always
    // have a body — which is why the fallback below is not padding but the
    // requirement. WP-06 measured this through the real pipeline; a test that
    // hands `render::from_expanded` a default `SpanMap` cannot see it, because
    // the span-keyed `written` map has nothing to line up with.
    let mut body = String::new();
    if page.deprecated {
        let note = page
            .deprecated_note
            .clone()
            .unwrap_or_else(|| "This operation is deprecated.".to_owned());
        body.push_str(&format!("**Deprecated.** {note}\n\n"));
    }
    if let Some(description) = &page.description {
        body.push_str(description);
    } else if let Some(summary) = &page.summary {
        body.push_str(summary);
    } else {
        // Nothing was written about this operation, and the container still
        // needs a body. Its own name is the only honest thing to put there.
        body.push_str(&page.title);
    }
    directive(&mut out, "endpoint", &props, &body);
    if let Some(intro) = &page.augmentation.intro
        && !intro.markdown.is_empty()
    {
        out.push_str(&format!("{}\n\n", intro.markdown));
    }

    if !page.servers.is_empty() {
        out.push_str("## Servers\n\n");
        for server in &page.servers {
            match &server.description {
                Some(text) => out.push_str(&format!("- `{}` — {text}\n", server.url)),
                None => out.push_str(&format!("- `{}`\n", server.url)),
            }
        }
        out.push('\n');
    }

    // Each entry is one ALTERNATIVE, not a requirement: a spec listing two
    // schemes under `security` accepts either, and a reader told "both" would
    // go looking for a second credential they do not need.
    if !page.auth.is_empty() {
        out.push_str("## Authentication\n\n");
        if page.auth.len() > 1 {
            out.push_str("Any one of:\n\n");
        }
        for option in &page.auth {
            let mut row = format!("- `{}` ({})", option.scheme, option.kind);
            if let Some(description) = &option.description {
                row.push_str(&format!(" — {description}"));
            }
            if !option.scopes.is_empty() {
                row.push_str(&format!(" (scopes: `{}`)", option.scopes.join("`, `")));
            }
            out.push_str(&format!("{row}\n"));
        }
        out.push('\n');
    }

    for section in &page.parameters {
        if section.fields.is_empty() {
            continue;
        }
        out.push_str(&format!("## {}\n\n", section.title));
        for field in &section.fields {
            rows(&mut out, field, "", Some(location_of(section)));
        }
    }
    slot(&mut out, page, "after-params");

    slot(&mut out, page, "before-request");
    if let Some(body) = &page.body {
        out.push_str("## Body\n\n");
        if let Some(description) = &body.description {
            out.push_str(&format!("{description}\n\n"));
        }
        for media in &body.media_types {
            if body.media_types.len() > 1 {
                out.push_str(&format!("### `{}`\n\n", media.media_type));
            }
            for field in &media.fields {
                rows(&mut out, field, "", None);
            }
            if let Some(example) = &media.example {
                fence(&mut out, &media.media_type, example);
            }
        }
    }

    slot(&mut out, page, "before-responses");
    if !page.responses.is_empty() {
        out.push_str("## Responses\n\n");
        for response in &page.responses {
            out.push_str(&format!("### {}\n\n", response.status));
            if !response.description.is_empty() {
                out.push_str(&format!("{}\n\n", response.description));
            }
            for media in &response.media_types {
                for field in &media.fields {
                    rows(&mut out, field, "", None);
                }
                if let Some(example) = &media.example {
                    fence(&mut out, &media.media_type, example);
                }
            }
            if !response.links.is_empty() {
                out.push_str("Links:\n\n");
                for link in &response.links {
                    let described = match (&link.operation, &link.description) {
                        (Some(operation), Some(text)) => {
                            format!("`{}` to `{operation}` — {text}", link.name)
                        }
                        (Some(operation), None) => format!("`{}` to `{operation}`", link.name),
                        (None, Some(text)) => format!("`{}` — {text}", link.name),
                        (None, None) => format!("`{}`", link.name),
                    };
                    out.push_str(&format!("- {described}\n"));
                }
                out.push('\n');
            }
        }
    }
    slot(&mut out, page, "after-responses");

    if !page.callbacks.is_empty() {
        out.push_str("## Callbacks\n\n");
        for callback in &page.callbacks {
            let described = match &callback.summary {
                Some(summary) => format!(
                    "`{}`: `{} {}` — {summary}",
                    callback.name, callback.method, callback.expression
                ),
                None => format!(
                    "`{}`: `{} {}`",
                    callback.name, callback.method, callback.expression
                ),
            };
            out.push_str(&format!("- {described}\n"));
        }
        out.push('\n');
    }

    out
}

/// A slot an authored page filled, at one of `page::SLOTS`'s points (API-04).
///
/// `rail-top` and `rail-bottom` are deliberately not here: they are the right
/// rail's, not the Markdown body's, so they travel in the `Augmentation` for the
/// reader runtime rather than being written into the source.
fn slot(out: &mut String, page: &Page, name: &str) {
    if let Some(content) = page.augmentation.slots.get(name)
        && !content.markdown.trim().is_empty()
    {
        out.push_str(content.markdown.trim());
        out.push_str("\n\n");
    }
}

fn location_of(section: &Section) -> &'static str {
    match section.location {
        ParameterIn::Path => "path",
        ParameterIn::Query => "query",
        ParameterIn::Header => "header",
        ParameterIn::Cookie => "cookie",
    }
}

/// One field and everything under it.
///
/// Nested fields are flattened with a dotted name rather than nested
/// directives: a container inside a container needs a longer fence, and a
/// reader reading a table wants `shipping.postcode` on its own row anyway. It
/// is what [`markdown`]'s own table does.
fn rows(out: &mut String, field: &Field, prefix: &str, location: Option<&str>) {
    let name = if prefix.is_empty() {
        field.name.clone()
    } else {
        format!("{prefix}.{}", field.name)
    };

    let mut props: Vec<(&str, Prop)> = vec![("name", Prop::Text(name.clone()))];
    if let Some(location) = location {
        props.push(("in", Prop::Text(location.to_owned())));
    }
    if !field.type_label.is_empty() {
        props.push(("type", Prop::Text(field.type_label.clone())));
    }
    // `required` and `deprecated` are `PropType::Bool`. A bare flag is `true`
    // (RFC 0304), which is what the schema wants; `required="true"` is E0315.
    if field.required {
        props.push(("required", Prop::Flag));
    }
    if field.deprecated {
        props.push(("deprecated", Prop::Flag));
    }
    if let Some(default) = &field.default {
        props.push(("default", Prop::Text(crate::example::as_text(default))));
    }
    if let Some(example) = &field.example {
        props.push(("example", Prop::Text(crate::example::as_text(example))));
    }
    if !field.enumeration.is_empty() {
        // A list member holding a quote or a comma cannot be spelled, so the
        // whole list goes or none of it does.
        let members: Vec<String> = field
            .enumeration
            .iter()
            .map(crate::example::as_text)
            .collect();
        if members
            .iter()
            .all(|member| safe(member) && !member.contains(','))
        {
            props.push(("enum", Prop::List(members)));
        }
    }

    let mut content = String::new();
    if let Some(description) = &field.description {
        content.push_str(description);
    }
    if !field.constraints.is_empty() {
        if !content.is_empty() {
            content.push_str("\n\n");
        }
        content.push_str(&field.constraints.join(", "));
    }
    if field.truncated {
        if !content.is_empty() {
            content.push_str("\n\n");
        }
        match &field.schema_name {
            Some(schema) => content.push_str(&format!("See `{schema}`.")),
            None => content.push_str("Nested further; expand to see the rest."),
        }
    }

    let kind = if location.is_some() {
        "param"
    } else {
        "response-field"
    };
    directive(out, kind, &props, &content);

    for child in &field.children {
        rows(out, child, &name, location);
    }
    for variant in &field.variants {
        rows(out, &variant.field, &name, location);
    }
}

/// One prop value, spelled the way the component's declared type wants it.
///
/// The distinction is not cosmetic. `directives::props::scalar` reads a bare
/// `true` as `PropValue::Bool` and a quoted `"true"` as `PropValue::Str`, and
/// the components declare `required` and `deprecated` as `PropType::Bool`. So
/// `required="true"` is `E0315` — "expects a boolean, and this is a string" —
/// and the row is dropped. Dispatching on the value's SHAPE instead would break
/// the other way round: `default` is `PropType::Str`, so a schema whose default
/// really is the text `true` must stay quoted.
enum Prop {
    Text(String),
    Flag,
    /// `[a, b]`, for a `PropType::List`.
    List(Vec<String>),
}

/// A container directive with its props and its content.
fn directive(out: &mut String, name: &str, props: &[(&str, Prop)], content: &str) {
    out.push_str(":::");
    out.push_str(name);
    let written: Vec<String> = props
        .iter()
        .filter_map(|(key, value)| match value {
            Prop::Text(text) if safe(text) => Some(format!("{key}=\"{text}\"")),
            Prop::Text(_) => None,
            // `key=true`, not a bare `key`. `directives/props.rs` accepts the
            // bare form and cites RFC 0304 for it, but `source/scan.rs` has a
            // second prop parser that rejects it with `E0312` and then orphans
            // the closing fence with `E0311` — two implementations of one syntax
            // in one crate, disagreeing. `key=true` is accepted by both, and
            // `key="true"` is NOT: that is `E0315`, because the schema declares
            // a boolean and a quoted value is a string. Measured at both stages,
            // not reasoned about.
            Prop::Flag => Some(format!("{key}=true")),
            Prop::List(members) => Some(format!("{key}=[{}]", members.join(", "))),
        })
        .collect();
    if !written.is_empty() {
        out.push_str(&format!("{{{}}}", written.join(" ")));
    }
    out.push('\n');
    if !content.trim().is_empty() {
        out.push_str(content.trim());
        out.push('\n');
    }
    out.push_str(":::\n\n");
}

/// Whether a value can be a directive prop at all.
///
/// `directives::props::value` ends a quoted string at the first `"` and has no
/// escape, so a value holding one would truncate the prop and swallow the rest
/// of the line. Such a value is dropped: the row loses a hint, where emitting
/// it would lose the document.
///
/// A brace is fine. The props block is delimited by the LAST `}` on the line
/// (`props.rs` checks `ends_with('}')`) and the scan reads a quoted value
/// through to its closing quote without looking inside it — which is what makes
/// `path="/widgets/{id}"` spellable, and every path has braces in it.
fn safe(value: &str) -> bool {
    !value.is_empty() && !value.contains('"') && !value.contains('\n')
}

fn fence(out: &mut String, media_type: &str, text: &str) {
    let language = if media_type.contains("json") {
        "json"
    } else if media_type.contains("xml") {
        "xml"
    } else {
        ""
    };
    out.push_str(&format!("```{language}\n{}\n```\n\n", text.trim_end()));
}
