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
use liyasa_core::vfs::{Vfs, VfsPath};
use serde_json::Value;

use crate::page::{self, Action, Components, Convert, Tag, TagKind};
use crate::plan::Plan;
use crate::report::{PageReport, Redirect, Report, Source};
use crate::tree::{self, read, route_of, site_route, strip, with_md_extension};

/// How to import.
pub struct Options<'a> {
    /// What Liyasa can render, supplied by the caller because the registry
    /// lives in `liyasa-components` (PRD §34.7).
    pub components: &'a dyn Components,
    /// Write components in the directive form rather than the tag form. MIG-01
    /// offers the conversion and does not impose it.
    pub directives: bool,
}

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
    tree::walk(vfs, root, &mut files);
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
                    route_of(relative.as_str()),
                    site_route(to.as_str()),
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
                    plan.text(relative.as_str(), tree::extensions(&text, "x-mint"));
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

    tree::dangling(&converted.pages, &mut plan);
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
