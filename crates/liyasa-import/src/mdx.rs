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

use std::cell::RefCell;
use std::collections::{BTreeMap, BTreeSet};

use liyasa_core::vfs::{Vfs, VfsPath};
use serde_json::json;

use crate::page::{self, Action, Components, Convert, Tag, TagKind, directive_name};
use crate::plan::Plan;
use crate::report::{PageReport, Report, Source};
use crate::tree::{self, read, route_of, site_route, strip, with_md_extension};

/// What to do with a component Liyasa does not know.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Choice {
    /// Write it as this Liyasa component instead.
    Use(String),
    /// Keep the tag and generate a user-defined component for it.
    Stub,
    /// Keep the tag and leave it for a human.
    Leave,
}

/// Answers the importer's question about an unknown component.
pub trait Mapping {
    fn choose(&self, name: &str) -> Choice;
}

/// The answer when there is nobody to ask: every unknown component gets a stub,
/// so the imported project builds on the first try.
pub struct Stubs;

impl Mapping for Stubs {
    fn choose(&self, _name: &str) -> Choice {
        Choice::Stub
    }
}

/// The answer for a caller that wants the constructs listed rather than filled
/// in, which is what a dry run against an unfamiliar tree is for.
pub struct LeaveAll;

impl Mapping for LeaveAll {
    fn choose(&self, _name: &str) -> Choice {
        Choice::Leave
    }
}

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
        seen: RefCell::new(BTreeMap::new()),
        frontmatter: BTreeMap::new(),
    };

    let mut files = Vec::new();
    tree::walk(vfs, root, &mut files);
    let mut has_config = false;

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
                plan.report.pages.push(entry);
                plan.text(to.as_str(), converted.text);
            }
            _ => {
                plan.carry(file.clone(), relative.clone());
            }
        }
    }

    for (name, use_) in convert.seen.into_inner() {
        if !use_.stub {
            continue;
        }
        let file = format!("components/{name}.jinja");
        plan.text(&file, stub(&name, &use_));
    }

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

/// How one unknown component was used across the tree.
#[derive(Debug, Default)]
struct Use {
    stub: bool,
    /// Every prop name it was given, so the stub declares them all.
    props: BTreeSet<String>,
    /// Whether it ever wrapped content.
    container: bool,
    /// The tag spelling the author wrote, for the stub's alias.
    tag: String,
}

/// A user-defined component that renders nothing yet (CMP-90).
///
/// The props are every one the tree used it with, typed as strings: the
/// importer knows the names an author wrote, not what they meant, and a wrong
/// type would be an error on a page that used to build.
fn stub(name: &str, use_: &Use) -> String {
    let mut out = String::from("{# ---\n");
    out.push_str(&format!("name: {name}\n"));
    if use_.tag != name {
        out.push_str(&format!("aliases: [\"{}\"]\n", use_.tag));
    }
    out.push_str(&format!(
        "kind: {}\n",
        if use_.container { "container" } else { "leaf" }
    ));
    if use_.props.is_empty() {
        out.push_str("props: {}\n");
    } else {
        out.push_str("props:\n");
        for prop in &use_.props {
            out.push_str(&format!("  {prop}: {{ type: string }}\n"));
        }
    }
    out.push_str("--- #}\n");
    out.push_str(&format!("{{# TODO: write the markup for `{name}`. #}}\n"));
    out.push_str(&format!("<div class=\"{name}\">\n"));
    for prop in &use_.props {
        out.push_str(&format!(
            "  <span class=\"{name}-{prop}\">{{{{ props.{prop} }}}}</span>\n"
        ));
    }
    if use_.container {
        out.push_str("  {{ content }}\n");
    }
    out.push_str("</div>\n");
    out
}

struct Mdx<'a> {
    components: &'a dyn Components,
    mapping: &'a dyn Mapping,
    seen: RefCell<BTreeMap<String, Use>>,
    frontmatter: BTreeMap<String, String>,
}

impl Convert for Mdx<'_> {
    fn known(&self, name: &str) -> bool {
        if self.components.known(name) {
            return true;
        }
        // A component the importer decided to generate is known from then on,
        // so it is not also reported as needing attention.
        self.seen
            .borrow()
            .get(&directive_name(name))
            .is_some_and(|use_| use_.stub)
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
        let spelling = directive_name(&tag.name);
        if self.components.known(&tag.name) || self.components.known(&spelling) {
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
                let mut seen = self.seen.borrow_mut();
                let use_ = seen.entry(spelling).or_default();
                use_.stub = true;
                use_.tag = tag.name.clone();
                use_.container |= tag.kind == TagKind::Open;
                for prop in &tag.props {
                    use_.props.insert(prop.name.clone());
                }
                Action::Keep
            }
            Choice::Leave => Action::Keep,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_stub_declares_every_prop_the_tree_used() {
        let use_ = Use {
            stub: true,
            props: ["plan", "highlight"]
                .into_iter()
                .map(str::to_owned)
                .collect(),
            container: true,
            tag: "PricingTable".to_owned(),
        };
        let text = stub("pricing-table", &use_);
        assert!(text.starts_with("{# ---\nname: pricing-table\n"));
        assert!(text.contains("aliases: [\"PricingTable\"]"));
        assert!(text.contains("kind: container"));
        assert!(text.contains("  highlight: { type: string }"));
        assert!(text.contains("  plan: { type: string }"));
        assert!(text.contains("{{ content }}"));
        assert!(text.contains("{{ props.plan }}"));
    }

    #[test]
    fn a_leaf_stub_has_no_content_slot() {
        let use_ = Use {
            stub: true,
            props: BTreeSet::new(),
            container: false,
            tag: "Spacer".to_owned(),
        };
        let text = stub("spacer", &use_);
        assert!(text.contains("kind: leaf"));
        assert!(text.contains("props: {}"));
        assert!(!text.contains("{{ content }}"));
    }
}
