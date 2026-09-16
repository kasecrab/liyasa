//! User-defined component stubs for the components Liyasa cannot render
//! (MIG-03, CMP-90, RFC 2902).
//!
//! A component an importer does not know is one file to write, not one page to
//! fix: the stub declares the props the tree used it with and keeps the tag
//! where the author put it, so the imported project builds and the operator
//! fills in the markup once. The report says so at the project level, naming
//! the component, the pages that used it, and the file written for it.

use std::cell::RefCell;
use std::collections::{BTreeMap, BTreeSet};

use crate::page::{Tag, TagKind, directive_name};
use crate::report::{Attention, Kind};

/// What to do with a component Liyasa does not know.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Choice {
    /// Write it as this Liyasa component instead.
    Use(String),
    /// Keep the tag and generate a user-defined component for it.
    Stub,
    /// Keep the tag and report it on every page that used it. RFC 2902
    /// rejected this as the way an import runs; see [`LeaveAll`].
    Leave,
}

/// Answers the importer's question about an unknown component.
///
/// MIG-03 asks it interactively, which a library cannot be: the CLI implements
/// this by prompting, a test by a table, and [`Stubs`] by always generating.
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

/// Reports every use on its own page instead of generating anything.
///
/// RFC 2902 settled that this is not how an import runs: it is the alternative
/// the core lead rejected, not a switch back to earlier behaviour, and no CLI
/// flag selects it. It stays because a caller reading a tree to find out what
/// is in it wants the per-page listing, and because the tests assert that the
/// choice is honoured.
pub struct LeaveAll;

impl Mapping for LeaveAll {
    fn choose(&self, _name: &str) -> Choice {
        Choice::Leave
    }
}

/// How one unknown component was used across a project.
#[derive(Debug, Default)]
struct Use {
    /// Every prop name it was given, so the stub declares them all.
    props: BTreeSet<String>,
    /// Whether it ever wrapped content.
    container: bool,
    /// The tag spelling the author wrote, for the stub's alias.
    tag: String,
    /// How many times it appeared, for the report.
    uses: usize,
}

/// The components an import decided to generate.
#[derive(Debug, Default)]
pub struct Generated {
    seen: RefCell<BTreeMap<String, Use>>,
}

impl Generated {
    pub fn new() -> Self {
        Self::default()
    }

    /// Whether a name has already been decided, which is what keeps a generated
    /// component from also being reported as needing attention.
    pub fn knows(&self, name: &str) -> bool {
        self.seen.borrow().contains_key(&directive_name(name))
    }

    /// Records one use of a component that will be generated.
    pub fn record(&self, tag: &Tag) {
        let mut seen = self.seen.borrow_mut();
        let entry = seen.entry(directive_name(&tag.name)).or_default();
        entry.tag = tag.name.clone();
        entry.container |= tag.kind == TagKind::Open;
        entry.uses += 1;
        for prop in &tag.props {
            entry.props.insert(prop.name.clone());
        }
    }

    /// The component files to write, as `(path, contents)`.
    pub fn files(&self) -> Vec<(String, String)> {
        self.seen
            .borrow()
            .iter()
            .map(|(name, use_)| (format!("components/{name}.jinja"), stub(name, use_)))
            .collect()
    }

    /// One entry per generated component. TODO(rfc-2902): this is where a
    /// component Liyasa cannot render is counted — once, against the project,
    /// rather than once against every page that used it.
    pub fn attention(&self) -> Vec<Attention> {
        self.seen
            .borrow()
            .iter()
            .map(|(name, use_)| {
                Attention::new(Kind::CustomComponent, use_.tag.clone()).help(format!(
                    "used {} times; a stub is at `components/{name}.jinja`, \
                     and the pages that use it render once it has markup",
                    use_.uses
                ))
            })
            .collect()
    }

    pub fn is_empty(&self) -> bool {
        self.seen.borrow().is_empty()
    }
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

#[cfg(test)]
mod tests {
    use liyasa_core::span::{SourceId, Span};

    use super::*;
    use crate::page::Prop;

    fn tag(name: &str, kind: TagKind, props: &[(&str, &str)]) -> Tag {
        Tag {
            name: name.to_owned(),
            kind,
            props: props
                .iter()
                .map(|(name, value)| Prop::new(*name, format!("\"{value}\"")))
                .collect(),
            span: Span::new(SourceId(0), 0, 1),
        }
    }

    #[test]
    fn a_stub_declares_every_prop_the_tree_used() {
        let generated = Generated::new();
        generated.record(&tag("PricingTable", TagKind::Open, &[("plan", "team")]));
        generated.record(&tag("PricingTable", TagKind::Open, &[("highlight", "yes")]));

        let files = generated.files();
        assert_eq!(files.len(), 1);
        let (path, text) = &files[0];
        assert_eq!(path, "components/pricing-table.jinja");
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
        let generated = Generated::new();
        generated.record(&tag("Spacer", TagKind::SelfClosing, &[]));
        let (_, text) = generated.files().remove(0);
        assert!(text.contains("kind: leaf"));
        assert!(text.contains("props: {}"));
        assert!(!text.contains("{{ content }}"));
    }

    #[test]
    fn the_project_report_counts_a_component_once_and_says_how_often_it_appears() {
        let generated = Generated::new();
        for _ in 0..29 {
            generated.record(&tag("PropertiesTable", TagKind::Open, &[]));
        }
        let attention = generated.attention();
        assert_eq!(attention.len(), 1);
        assert_eq!(attention[0].what, "PropertiesTable");
        assert!(
            attention[0]
                .help
                .as_deref()
                .is_some_and(|help| help.contains("used 29 times"))
        );
    }

    #[test]
    fn a_dotted_name_keeps_its_own_stub_and_its_own_alias() {
        let generated = Generated::new();
        generated.record(&tag("Tree.File", TagKind::SelfClosing, &[("name", "a.rs")]));
        let (path, text) = generated.files().remove(0);
        assert_eq!(path, "components/tree-file.jinja");
        assert!(text.contains("aliases: [\"Tree.File\"]"));
    }
}
