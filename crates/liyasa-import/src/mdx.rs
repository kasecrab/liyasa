//! The generic MDX importer (MIG-03).
//!
//! Anything that is a tree of `.mdx` files can come through here: there is no
//! config to read and no product's conventions to honour, so the only real
//! question is what to do with a component nobody recognizes. MIG-03 answers it
//! interactively, which a library cannot be, so the question is asked through
//! [`Mapping`] — the CLI implements it by prompting, a test by a table, and the
//! default by writing a component stub.
//!
//! Writing a stub is the answer that leaves a project that builds. The tag
//! stays where the author put it, `components/<name>.jinja` declares the props
//! it was used with, and the operator fills in the markup once instead of
//! editing every page that used it (CMP-90).

use std::collections::BTreeMap;

use liyasa_core::vfs::{Vfs, VfsPath};
use serde_json::json;

use crate::page::{self, Action, Components, Convert, Tag, TagKind, directive_name};
use crate::plan::Plan;
use crate::report::{PageReport, Report, Source};
use crate::stubs::{Choice, Generated, Mapping};
use crate::tree::{self, read, route_of, site_route, strip, with_md_extension};

pub struct Options<'a> {
    /// What Liyasa can render, supplied by the caller (PRD §34.7).
    pub components: &'a dyn Components,
    /// What to do with everything else.
    pub mapping: &'a dyn Mapping,
    pub directives: bool,
    /// The site name for the generated config, when the tree has none.
    pub name: &'a str,
}

/// Reads a tree of MDX and plans a Liyasa project. Nothing is written.
pub fn import(vfs: &dyn Vfs, root: &VfsPath, options: &Options<'_>) -> Plan {
    let mut plan = Plan::new(Report::new(Source::Mdx));
    let convert = Mdx {
        components: options.components,
        mapping: options.mapping,
        generated: Generated::new(),
        frontmatter: BTreeMap::new(),
    };

    let mut files = Vec::new();
    tree::walk(vfs, root, &mut files);
    let mut has_config = false;
    let mut partials: Vec<(VfsPath, String, String)> = Vec::new();

    for file in &files {
        let relative = strip(root, file);
        if relative.as_str() == "liyasa.json" {
            has_config = true;
            plan.carry(file.clone(), relative.clone());
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
                for (specifier, name) in &converted.snippets {
                    partials.push((relative.clone(), specifier.clone(), name.clone()));
                }
                plan.report.pages.push(entry);
                plan.text(to.as_str(), converted.text);
            }
            _ => {
                plan.carry(file.clone(), relative.clone());
            }
        }
    }

    tree::move_partials(&partials, &mut plan);
    for (path, text) in convert.generated.files() {
        plan.text(&path, text);
    }
    plan.report.attention.extend(convert.generated.attention());

    if !has_config {
        let config = json!({
            "$schema": format!("{}liyasa.json", liyasa_core::site::SCHEMA_URL_BASE),
            "name": options.name,
            "navigation": { "autofill": true },
        });
        match serde_json::to_string_pretty(&config) {
            Ok(mut text) => {
                text.push('\n');
                plan.text("liyasa.json", text);
            }
            Err(error) => plan.report.diagnostics.push(liyasa_core::Diagnostic::new(
                liyasa_core::diagnostics::code::E1104,
                format!("the generated config could not be written: {error}"),
            )),
        }
    }
    let report = plan.report.to_markdown();
    plan.text("migration-report.md", report);
    plan
}

struct Mdx<'a> {
    components: &'a dyn Components,
    mapping: &'a dyn Mapping,
    generated: Generated,
    frontmatter: BTreeMap<String, String>,
}

impl Convert for Mdx<'_> {
    fn known(&self, name: &str) -> bool {
        if self.components.known(name) {
            return true;
        }
        // A component the importer decided to generate is known from then on,
        // so it is not also reported as needing attention.
        self.generated.knows(name)
    }

    fn suggest(&self, name: &str) -> Option<String> {
        self.components.suggest(name)
    }

    fn frontmatter(&self) -> &BTreeMap<String, String> {
        &self.frontmatter
    }

    fn boilerplate(&self, specifier: &str) -> bool {
        specifier.starts_with("@theme/") || specifier.starts_with("@docusaurus/")
    }

    fn element(&self, tag: &Tag) -> Action {
        if self.components.known(&tag.name) || self.components.known(&directive_name(&tag.name)) {
            return Action::Keep;
        }

        // A closing tag needs no decision: whatever the opening tag became,
        // its name is the one the walk already wrote.
        if tag.kind == TagKind::Close {
            return Action::Keep;
        }

        match self.mapping.choose(&tag.name) {
            Choice::Use(replacement) => Action::Rewrite {
                name: replacement,
                props: tag.props.clone(),
            },
            Choice::Stub => {
                self.generated.record(tag);
                Action::Keep
            }
            Choice::Leave => Action::Keep,
        }
    }
}
