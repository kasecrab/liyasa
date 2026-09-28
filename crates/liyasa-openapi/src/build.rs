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

use liyasa_core::diagnostics::{Diagnostic, Diagnostics, code};
use liyasa_core::vfs::{Vfs, VfsPath};

use crate::config::SpecConfig;
use crate::field::Field;
use crate::nav::{self, Entry, Node, Reference};
use crate::page::{Augmentation, BuildOptions, Page, Rendered};
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
    let mut out = Surface::default();
    let (specs, problems) = crate::config::specs(config.get("openapi"));
    for problem in problems {
        out.diagnostics.push(
            Diagnostic::new(code::E0133, problem)
                .help("check the `openapi` array against schemas/liyasa.schema.json"),
        );
    }
    for spec in &specs {
        let Some(model) = processed(vfs, spec, &mut out.diagnostics) else {
            continue;
        };
        let one = one(&model, spec, authored);
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
pub fn processed(vfs: &dyn Vfs, spec: &SpecConfig, diagnostics: &mut Diagnostics) -> Option<Spec> {
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

    let mut documents = read::Documents::default();
    documents.insert(documents.root_key().to_owned(), root);
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

struct One {
    pages: Vec<SpecPage>,
    navigation: Reference,
    diagnostics: Diagnostics,
}

fn one(model: &Spec, config: &SpecConfig, authored: &[Authored]) -> One {
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
                    ..Augmentation::default()
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
                    &markdown::render(&page, &markdown::Options::default()),
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
