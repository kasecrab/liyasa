//! What the server knows about the project around the file being edited.
//!
//! An editor opens one buffer; a completion in it needs the whole tree — which
//! components exist, which variables are defined, which facts have values,
//! which snippets can be included, and which routes a link may name. The
//! workspace is that index, read once through a [`Vfs`] and refreshed when a
//! file that feeds it changes.
//!
//! Everything here is best-effort by design. A project whose `liyasa.json` does
//! not parse still completes component names, and a `facts/` directory that
//! does not exist is simply no facts. A language server that refuses to answer
//! because the project is mid-edit is a language server nobody leaves running.

use std::collections::BTreeMap;

use liyasa_components::Registry;
use liyasa_core::ids::Locale;
use liyasa_core::markdown::SiteMeta;
use liyasa_core::vfs::{Vfs, VfsPath};

use liyasa_markdown::source::route::{Ignore, is_routable, route_of};

/// Directories whose contents are not pages (CM-03).
const NOT_PAGES: &[&str] = &[
    "snippets",
    "components",
    "facts",
    "theme",
    "assets",
    "public",
    "templates",
    "automations",
    "skills",
    "dist",
    ".liyasa",
    // Not CM-03's list: a docs tree can sit at the root of a repository, and
    // walking either of these is minutes of I/O for no pages at all.
    "node_modules",
    "target",
];

/// One value under `facts.*`, and the file it is written in.
#[derive(Debug, Clone, PartialEq)]
pub struct Fact {
    /// The dotted path as a page writes it: `pricing.pro.monthly_usd`.
    pub path: String,
    pub file: VfsPath,
    pub value: serde_json::Value,
}

/// One page of the content tree.
#[derive(Debug, Clone, PartialEq)]
pub struct Page {
    pub route: String,
    pub file: VfsPath,
    pub title: Option<String>,
}

#[derive(Debug, Clone, PartialEq)]
pub struct Snippet {
    pub name: String,
    pub file: VfsPath,
}

pub struct Workspace {
    pub registry: Registry,
    /// What the render pipeline needs to know about the site as a whole.
    pub site: SiteMeta,
    /// `variables` from `liyasa.json` merged with `snippets/vars.*`, flattened
    /// to the dotted paths a template writes.
    pub variables: BTreeMap<String, serde_json::Value>,
    pub facts: BTreeMap<String, Fact>,
    pub snippets: BTreeMap<String, Snippet>,
    pub pages: BTreeMap<String, Page>,
}

impl Default for Workspace {
    fn default() -> Self {
        Self {
            registry: Registry::builtins(),
            site: site_meta("", ""),
            variables: BTreeMap::new(),
            facts: BTreeMap::new(),
            snippets: BTreeMap::new(),
            pages: BTreeMap::new(),
        }
    }
}

impl std::fmt::Debug for Workspace {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("Workspace")
            .field("components", &self.registry.len())
            .field("variables", &self.variables.len())
            .field("facts", &self.facts.len())
            .field("snippets", &self.snippets.len())
            .field("pages", &self.pages.len())
            .finish()
    }
}

impl Workspace {
    /// The built-in components and nothing else, for a buffer with no project
    /// around it.
    pub fn new() -> Self {
        Self::default()
    }

    /// Reads a project. Every step is independent: a failure in one leaves the
    /// others populated.
    pub fn load(vfs: &dyn Vfs) -> Self {
        let mut workspace = Self::new();
        workspace.load_variables(vfs);
        workspace.load_facts(vfs);
        workspace.load_snippets(vfs);
        workspace.load_pages(vfs);
        workspace
    }

    fn load_variables(&mut self, vfs: &dyn Vfs) {
        if let Some(config) = read_json(vfs, &VfsPath::new("liyasa.json")) {
            self.site = site_meta(
                config
                    .get("name")
                    .and_then(|name| name.as_str())
                    .unwrap_or(""),
                config
                    .get("seo")
                    .and_then(|seo| seo.get("canonicalOrigin"))
                    .and_then(|origin| origin.as_str())
                    .unwrap_or(""),
            );
            if let Some(variables) = config.get("variables") {
                flatten("", variables, &mut self.variables);
            }
        }
        for name in ["vars.json", "vars.yaml", "vars.yml"] {
            if let Some(vars) = read_data(vfs, &VfsPath::new("snippets").join(name)) {
                flatten("", &vars, &mut self.variables);
                break;
            }
        }
    }

    /// `facts/sources.toml` names each source and the file its values live in;
    /// a `facts/<id>.json` with no row is read under its own stem, because that
    /// is what a project that has not written `sources.toml` yet looks like.
    fn load_facts(&mut self, vfs: &dyn Vfs) {
        let declared = read_text(vfs, &VfsPath::new("facts/sources.toml"))
            .and_then(|text| toml::from_str::<Sources>(&text).ok())
            .map(|sources| sources.source)
            .unwrap_or_default();

        let mut by_id: BTreeMap<String, VfsPath> = BTreeMap::new();
        for source in declared {
            let Some(path) = source.path else { continue };
            by_id.insert(source.id, VfsPath::new(path));
        }

        let facts = VfsPath::new("facts");
        for file in vfs.list(&facts).unwrap_or_default() {
            let Some(name) = file.file_name() else {
                continue;
            };
            let Some((stem, extension)) = name.rsplit_once('.') else {
                continue;
            };
            if !matches!(extension, "json" | "yaml" | "yml") || stem.ends_with(".schema") {
                continue;
            }
            by_id.entry(stem.to_owned()).or_insert(file);
        }

        for (id, file) in by_id {
            let Some(value) = read_data(vfs, &file) else {
                continue;
            };
            let mut flat = BTreeMap::new();
            flatten(&id, &value, &mut flat);
            for (path, value) in flat {
                self.facts.insert(
                    path.clone(),
                    Fact {
                        path,
                        file: file.clone(),
                        value,
                    },
                );
            }
        }
    }

    /// A snippet is named by its path under `snippets/` without its extension,
    /// which is how `{% include %}` and `snippet-from` spell it.
    fn load_snippets(&mut self, vfs: &dyn Vfs) {
        for file in walk(vfs, &VfsPath::new("snippets")) {
            let text = file.as_str();
            let Some(rest) = text.strip_prefix("snippets/") else {
                continue;
            };
            let Some((name, extension)) = rest.rsplit_once('.') else {
                continue;
            };
            if !matches!(extension, "md" | "mdx" | "markdown") {
                continue;
            }
            self.snippets.insert(
                name.to_owned(),
                Snippet {
                    name: name.to_owned(),
                    file: file.clone(),
                },
            );
        }
    }

    fn load_pages(&mut self, vfs: &dyn Vfs) {
        let ignore = read_text(vfs, &VfsPath::new(".liyasaignore"))
            .map(|text| Ignore::parse(&text))
            .unwrap_or_else(|| Ignore::parse(""));

        for file in walk(vfs, &VfsPath::new("")) {
            if !is_routable(&file, &ignore) {
                continue;
            }
            let route = route_of(&file, None).to_string();
            let title = read_text(vfs, &file).and_then(|text| title_of(&text));
            self.pages
                .insert(route.clone(), Page { route, file, title });
        }
    }
}

/// A preview is not a deploy, so an unset `seo.canonicalOrigin` is not an
/// error: absolute URLs in the preview point at a host that resolves nowhere,
/// which is the honest rendering of a site that has not chosen one.
fn site_meta(name: &str, origin: &str) -> SiteMeta {
    let canonical_origin = url::Url::parse(origin)
        .or_else(|_| url::Url::parse("https://example.invalid"))
        .unwrap_or_else(|error| unreachable!("a literal origin parses: {error}"));
    let llms_txt = canonical_origin
        .join("llms.txt")
        .unwrap_or_else(|_| canonical_origin.clone());
    SiteMeta {
        name: name.to_owned(),
        canonical_origin,
        llms_txt,
        version: None,
        locale: Locale::new("en"),
    }
}

#[derive(Debug, Default, serde::Deserialize)]
struct Sources {
    #[serde(default)]
    source: Vec<Source>,
}

#[derive(Debug, serde::Deserialize)]
struct Source {
    id: String,
    #[serde(default)]
    path: Option<String>,
}

/// Every file under `dir`, skipping the directories that hold no pages. The
/// recursion is bounded by the tree; a `Vfs` that reports a directory as its
/// own child would loop, and none of the implementations does.
fn walk(vfs: &dyn Vfs, dir: &VfsPath) -> Vec<VfsPath> {
    let mut out = Vec::new();
    let mut queue = vec![dir.clone()];
    let mut seen = 0usize;
    while let Some(next) = queue.pop() {
        seen += 1;
        if seen > 10_000 {
            break;
        }
        let Ok(entries) = vfs.list(&next) else {
            continue;
        };
        for entry in entries {
            match vfs.metadata(&entry) {
                Ok(meta) if meta.kind == liyasa_core::vfs::VfsKind::Dir => {
                    let name = entry.file_name().unwrap_or_default().to_owned();
                    if name.starts_with('.') || NOT_PAGES.contains(&name.as_str()) {
                        continue;
                    }
                    queue.push(entry);
                }
                Ok(_) => out.push(entry),
                Err(_) => {}
            }
        }
    }
    out.sort();
    out
}

/// The first ATX heading, as a page's title when its front matter has none.
fn title_of(text: &str) -> Option<String> {
    if let Some(front) = text.strip_prefix("---\n")
        && let Some(end) = front.find("\n---")
        && let Ok(value) = liyasa_core::yaml::parse_value(&front[..end], None)
        && let Some(title) = value.get("title").and_then(|t| t.as_str())
    {
        return Some(title.to_owned());
    }
    text.lines()
        .find(|line| line.starts_with("# "))
        .map(|line| line[2..].trim().to_owned())
}

fn read_text(vfs: &dyn Vfs, path: &VfsPath) -> Option<String> {
    let bytes = vfs.read(path).ok()?;
    String::from_utf8(bytes.to_vec()).ok()
}

fn read_json(vfs: &dyn Vfs, path: &VfsPath) -> Option<serde_json::Value> {
    serde_json::from_str(&read_text(vfs, path)?).ok()
}

fn read_data(vfs: &dyn Vfs, path: &VfsPath) -> Option<serde_json::Value> {
    let text = read_text(vfs, path)?;
    match path.extension() {
        Some("json") => serde_json::from_str(&text).ok(),
        _ => liyasa_core::yaml::parse_value(&text, None).ok(),
    }
}

/// `{"pro": {"price": 49}}` under the prefix `pricing` becomes
/// `pricing.pro.price = 49`, and every container on the way keeps its own row
/// so that completion can offer `pricing.pro` as well as the leaf.
fn flatten(prefix: &str, value: &serde_json::Value, out: &mut BTreeMap<String, serde_json::Value>) {
    if !prefix.is_empty() {
        out.insert(prefix.to_owned(), value.clone());
    }
    if let serde_json::Value::Object(fields) = value {
        for (key, child) in fields {
            let path = if prefix.is_empty() {
                key.clone()
            } else {
                format!("{prefix}.{key}")
            };
            flatten(&path, child, out);
        }
    }
}
