//! The Mintlify importer (MIG-01).
//!
//! Liyasa's component library and navigation schema were both modelled on
//! Mintlify's, so most of a Mintlify project is already a Liyasa project: the
//! importer's job is the config, the three constructs MDX has and Liyasa does
//! not (§7.1), and saying out loud what it could not do.
//!
//! Files that are not pages are carried at the path they had. Moving images
//! into `assets/` would be tidier and would break every `![](/images/…)` in the
//! project, which is the opposite of what MIG-01's quality bar measures.

pub mod config;
pub mod nav;

use std::collections::BTreeMap;

use liyasa_core::diagnostics::{Diagnostic, code};
use liyasa_core::vfs::{Vfs, VfsKind, VfsPath};
use serde_json::Value;

use crate::page::{self, Action, Components, Convert, Tag, TagKind};
use crate::plan::Plan;
use crate::report::{Attention, Kind, PageReport, Redirect, Report, Source};

/// How to import.
pub struct Options<'a> {
    /// What Liyasa can render, supplied by the caller because the registry
    /// lives in `liyasa-components` (PRD §34.7).
    pub components: &'a dyn Components,
    /// Write components in the directive form rather than the tag form. MIG-01
    /// offers the conversion and does not impose it.
    pub directives: bool,
}

/// Directories that belong to a toolchain rather than to the documentation.
const SKIP: &[&str] = &[
    ".git",
    ".github",
    "node_modules",
    ".mintlify",
    ".vercel",
    "dist",
];

/// Reads a Mintlify project and plans a Liyasa one. Nothing is written.
pub fn import(vfs: &dyn Vfs, root: &VfsPath, options: &Options<'_>) -> Plan {
    let mut plan = Plan::new(Report::new(Source::Mintlify));

    let Some((config_path, text)) = read_config(vfs, root) else {
        plan.report.diagnostics.push(
            Diagnostic::new(
                code::E1101,
                format!("no `docs.json` or `mint.json` under `{root}`"),
            )
            .help("point the importer at the directory that holds the Mintlify config"),
        );
        return plan;
    };

    let source: Value = match serde_json::from_str(&text) {
        Ok(value) => value,
        Err(error) => {
            plan.report.diagnostics.push(Diagnostic::new(
                code::E1104,
                format!("`{config_path}` is not valid JSON: {error}"),
            ));
            return plan;
        }
    };

    let mut converted = config::convert(&source);
    plan.report.attention.append(&mut converted.attention);

    let convert = Mintlify {
        components: options.components,
        frontmatter: BTreeMap::new(),
    };
    let mut files = Vec::new();
    walk(vfs, root, &mut files);
    let mut carried_snippets = Vec::new();

    for file in &files {
        let relative = strip(root, file);
        if relative.as_str() == "docs.json" || relative.as_str() == "mint.json" {
            continue;
        }
        if relative.as_str() == ".mintignore" {
            plan.carry(file.clone(), VfsPath::new(".liyasaignore"));
            continue;
        }

        match relative.extension() {
            Some("mdx" | "md") => {
                let Some(text) = read(vfs, file, &mut plan.report) else {
                    continue;
                };
                let converted = page::convert(
                    &text,
                    &page::Options {
                        convert: &convert,
                        directives: options.directives,
                    },
                );
                let to = with_md_extension(&relative);
                let mut entry = PageReport::new(
                    relative.clone(),
                    to.clone(),
                    route_of(&relative),
                    route_of(&to),
                );
                entry.attention = converted.attention;
                if entry.moved() {
                    plan.report.redirects.push(Redirect {
                        source: entry.old_route.clone(),
                        destination: entry.route.clone(),
                    });
                }
                for (specifier, name) in &converted.snippets {
                    carried_snippets.push((specifier.clone(), name.clone()));
                }
                plan.report.pages.push(entry);
                plan.text(to.as_str(), converted.text);
            }
            Some("json" | "yaml" | "yml") => match read(vfs, file, &mut plan.report) {
                Some(text) if text.contains("x-mint") => {
                    plan.text(relative.as_str(), extensions(&text));
                    plan.report.carried.push(relative.clone());
                }
                _ => {
                    plan.carry(file.clone(), relative.clone());
                }
            },
            _ => {
                plan.carry(file.clone(), relative.clone());
            }
        }
    }

    dangling(&converted.pages, &mut plan);
    redirects(&mut converted.value, &plan.report.redirects);
    match serde_json::to_string_pretty(&converted.value) {
        Ok(mut text) => {
            text.push('\n');
            plan.text("liyasa.json", text);
        }
        Err(error) => plan.report.diagnostics.push(Diagnostic::new(
            code::E1104,
            format!("the converted config could not be written: {error}"),
        )),
    }
    let report = plan.report.to_markdown();
    plan.text("migration-report.md", report);
    plan
}

/// Mintlify's own component table. Almost every name is one Liyasa already has,
/// because §9's library was modelled on it; the three that are not are here.
struct Mintlify<'a> {
    components: &'a dyn Components,
    frontmatter: BTreeMap<String, String>,
}

impl Convert for Mintlify<'_> {
    fn known(&self, name: &str) -> bool {
        self.components.known(name)
    }

    fn suggest(&self, name: &str) -> Option<String> {
        self.components.suggest(name)
    }

    fn frontmatter(&self) -> &BTreeMap<String, String> {
        // Mintlify's front matter keys are §7.6's: `title`, `description`,
        // `sidebarTitle`, `icon`, `mode`, `url`, `openapi`, `keywords`.
        &self.frontmatter
    }

    fn element(&self, tag: &Tag) -> Action {
        match tag.name.as_str() {
            // `<Snippet file="a/b.mdx" />` is Mintlify's include (CM-70).
            "Snippet" => match tag.text("file") {
                Some(file) => {
                    let name = page::snippet_name(file);
                    let props: Vec<page::Prop> = tag
                        .props
                        .iter()
                        .filter(|prop| prop.name != "file")
                        .cloned()
                        .collect();
                    Action::Replace(snippet(&name, &props))
                }
                None => Action::Keep,
            },
            // `<Latex>x^2</Latex>` is math, which Liyasa writes as `$…$` (CM-33).
            "Latex" => match tag.kind {
                TagKind::SelfClosing => Action::Unwrap,
                _ => Action::Replace("$".to_owned()),
            },
            _ => Action::Keep,
        }
    }
}

fn snippet(name: &str, props: &[page::Prop]) -> String {
    let mut out = format!("{{% snippet \"{name}\"");
    for prop in props {
        match prop.text() {
            Some(text) => out.push_str(&format!(" {}=\"{}\"", prop.name, text)),
            None => match &prop.value {
                Some(value) => out.push_str(&format!(" {}={}", prop.name, value)),
                None => out.push_str(&format!(" {}=true", prop.name)),
            },
        }
    }
    out.push_str(" %}");
    out
}

/// `docs.json`, or the legacy `mint.json`.
fn read_config(vfs: &dyn Vfs, root: &VfsPath) -> Option<(VfsPath, String)> {
    for name in ["docs.json", "mint.json"] {
        let path = root.join(name);
        if let Ok(bytes) = vfs.read(&path)
            && let Ok(text) = String::from_utf8(bytes.to_vec())
        {
            return Some((path, text));
        }
    }
    None
}

fn read(vfs: &dyn Vfs, path: &VfsPath, report: &mut Report) -> Option<String> {
    match vfs.read(path) {
        Ok(bytes) => match String::from_utf8(bytes.to_vec()) {
            Ok(text) => Some(text),
            Err(_) => {
                report.diagnostics.push(Diagnostic::new(
                    code::E1102,
                    format!("`{path}` is not UTF-8"),
                ));
                None
            }
        },
        Err(error) => {
            report.diagnostics.push(Diagnostic::new(
                code::E1102,
                format!("cannot read `{path}`: {error}"),
            ));
            None
        }
    }
}

fn walk(vfs: &dyn Vfs, dir: &VfsPath, out: &mut Vec<VfsPath>) {
    let Ok(entries) = vfs.list(dir) else {
        return;
    };
    for entry in entries {
        if entry.file_name().is_some_and(|name| SKIP.contains(&name)) {
            continue;
        }
        match vfs.metadata(&entry) {
            Ok(meta) if meta.kind == VfsKind::Dir => walk(vfs, &entry, out),
            Ok(_) => out.push(entry),
            Err(_) => {}
        }
    }
}

fn strip(root: &VfsPath, path: &VfsPath) -> VfsPath {
    let prefix = root.as_str();
    if prefix.is_empty() {
        return path.clone();
    }
    match path.as_str().strip_prefix(&format!("{prefix}/")) {
        Some(rest) => VfsPath::new(rest),
        None => path.clone(),
    }
}

fn with_md_extension(path: &VfsPath) -> VfsPath {
    match path.as_str().strip_suffix(".mdx") {
        Some(stem) => VfsPath::new(format!("{stem}.md")),
        None => path.clone(),
    }
}

/// The route a page path serves, which is the same rule on both sides (CM-02),
/// so a page that did not move produces no redirect.
fn route_of(path: &VfsPath) -> String {
    let text = path.as_str();
    let stem = text
        .strip_suffix(".mdx")
        .or_else(|| text.strip_suffix(".md"))
        .unwrap_or(text);
    let route = stem
        .strip_suffix("index")
        .map_or(stem, |head| head.strip_suffix('/').unwrap_or(head));
    if route.is_empty() {
        return "/".to_owned();
    }
    format!("/{route}")
}

/// Navigation entries that name a page the project does not have.
fn dangling(named: &std::collections::BTreeSet<String>, plan: &mut Plan) {
    let have: std::collections::BTreeSet<String> = plan
        .report
        .pages
        .iter()
        .map(|page| {
            page.to
                .as_str()
                .strip_suffix(".md")
                .unwrap_or(page.to.as_str())
                .to_owned()
        })
        .collect();
    for entry in named {
        let stem = entry.trim_start_matches('/');
        if stem.starts_with("http") || have.contains(stem) {
            continue;
        }
        plan.report.attention.push(
            Attention::new(Kind::DanglingPage, entry.clone())
                .help("the navigation names it and the project has no such page"),
        );
    }
}

/// Merges the redirects generated from moved pages into the config's own.
fn redirects(config: &mut Value, generated: &[Redirect]) {
    if generated.is_empty() {
        return;
    }
    let Some(map) = config.as_object_mut() else {
        return;
    };
    let mut rules: Vec<Value> = match map.get("redirects") {
        Some(Value::Array(existing)) => existing.clone(),
        Some(Value::Object(object)) => match object.get("rules") {
            Some(Value::Array(existing)) => existing.clone(),
            _ => Vec::new(),
        },
        _ => Vec::new(),
    };
    for redirect in generated {
        rules.push(serde_json::json!({
            "source": redirect.source,
            "destination": redirect.destination,
        }));
    }
    map.insert("redirects".to_owned(), Value::Array(rules));
}

/// `x-mint` becomes `x-liyasa` (API-05).
///
/// The rewrite is textual and matches the key syntax rather than the text, so a
/// spec that uses the extension keeps its key order, its comments, and its
/// formatting; re-serializing a 20 000-line OpenAPI document to change two keys
/// is not a trade a migration should make.
pub fn extensions(text: &str) -> String {
    let mut out = String::with_capacity(text.len());
    for line in text.split_inclusive('\n') {
        let trimmed = line.trim_start();
        let indent = line.len() - trimmed.len();
        if let Some(rest) = trimmed.strip_prefix("\"x-mint\"") {
            out.push_str(&line[..indent]);
            out.push_str("\"x-liyasa\"");
            out.push_str(rest);
        } else if let Some(rest) = trimmed.strip_prefix("x-mint:") {
            out.push_str(&line[..indent]);
            out.push_str("x-liyasa:");
            out.push_str(rest);
        } else {
            out.push_str(line);
        }
    }
    out
}
