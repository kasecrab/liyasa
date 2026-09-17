//! Content variations (CM-111).
//!
//! A variation is the lighter half of a dimension. A dimension produces routes
//! and a variant per value; a variation produces **one** HTML page carrying
//! every option, with a switcher that picks between them in the browser. That
//! is why nothing here appears in [`Variant`](liyasa_core::build::Variant) and
//! why there is no server side: a site that only wants "cloud or self-hosted"
//! should not pay for a route per option, a cache key per option, or a server.
//!
//! The consequence is a rule this module exists to hold: **every option's text
//! is in the delivered HTML.** A variation is a convenience, never a gate. Use
//! `:::region` or a group gate for content a reader must not see — those
//! withhold the bytes; this one ships all of them and hides the rest with an
//! attribute.

use std::collections::{BTreeMap, BTreeSet};

use liyasa_core::diagnostics::{Diagnostic, code};
use liyasa_core::document::{PropValue, Segment, SourceDocument};
use serde_json::Value;

use super::config::VariationDecl;

/// The directive name CM-111 spells: `:::variation{deployment="cloud"}`.
pub const DIRECTIVE: &str = "variation";

/// Marks the wrapper of one option in the rendered page.
pub const DATA_ATTRIBUTE: &str = "data-liyasa";

/// Where a reader's choice is kept between pages. Per browser, not per request:
/// nothing about a variation reaches the server.
pub const STORAGE_KEY: &str = "liyasa.variation";

/// One entry of a variation's switcher.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Choice {
    pub option: String,
    pub label: String,
    pub current: bool,
}

/// One variation's switcher, page-level or site-level (CM-111).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Switcher {
    pub name: String,
    pub label: String,
    pub choices: Vec<Choice>,
}

#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct Variations {
    decls: Vec<VariationDecl>,
    /// Options learned from content, for a variation whose config declares
    /// none (RFC 2700).
    discovered: BTreeMap<String, BTreeSet<String>>,
}

impl Variations {
    pub fn from_value(value: &Value) -> Self {
        Self {
            decls: VariationDecl::from_value(value),
            discovered: BTreeMap::new(),
        }
    }

    pub fn is_empty(&self) -> bool {
        self.decls.is_empty()
    }

    pub fn names(&self) -> Vec<&str> {
        self.decls.iter().map(|decl| decl.name.as_str()).collect()
    }

    pub fn declares(&self, name: &str) -> bool {
        self.decls.iter().any(|decl| decl.name == name)
    }

    /// Folds what a page's directives named into the option sets.
    ///
    /// Only a variation the config declares is learned from: an unknown
    /// variation name is a typo to report, not a variation to invent.
    pub fn absorb(&mut self, found: &BTreeMap<String, BTreeSet<String>>) {
        for (name, options) in found {
            if !self.declares(name) {
                continue;
            }
            self.discovered
                .entry(name.clone())
                .or_default()
                .extend(options.iter().cloned());
        }
    }

    /// The options of one variation: what the config declared, or what the
    /// content used.
    // TODO(rfc-2700): `schemas/liyasa.schema.json` has no `variations[].options`
    // and CM-111's own example declares one, so the content is the fallback
    // source rather than the only one.
    pub fn options(&self, name: &str) -> Vec<String> {
        let Some(decl) = self.decls.iter().find(|decl| decl.name == name) else {
            return Vec::new();
        };
        if !decl.options.is_empty() {
            return decl.options.clone();
        }
        self.discovered
            .get(name)
            .map(|found| found.iter().cloned().collect())
            .unwrap_or_default()
    }

    /// The option a reader sees before they touch the switcher: the first one
    /// declared, which is the only ordering a config gives.
    pub fn default_option(&self, name: &str) -> Option<String> {
        self.options(name).first().cloned()
    }

    /// The switchers a page shows, given the variations its content uses.
    ///
    /// A variation the page never mentions gets no switcher: a page that says
    /// the same thing for every deployment should not ask the reader to pick
    /// one. A variation with one option gets none either — there is nothing to
    /// switch between.
    pub fn switchers(
        &self,
        used: &BTreeSet<String>,
        chosen: &BTreeMap<String, String>,
    ) -> Vec<Switcher> {
        self.decls
            .iter()
            .filter(|decl| used.contains(&decl.name))
            .filter_map(|decl| {
                let options = self.options(&decl.name);
                if options.len() < 2 {
                    return None;
                }
                let current = chosen
                    .get(&decl.name)
                    .cloned()
                    .or_else(|| options.first().cloned());
                Some(Switcher {
                    name: decl.name.clone(),
                    label: decl.label.clone(),
                    choices: options
                        .iter()
                        .map(|option| Choice {
                            current: current.as_deref() == Some(option.as_str()),
                            option: option.clone(),
                            label: option.clone(),
                        })
                        .collect(),
                })
            })
            .collect()
    }

    /// `W0726`: a directive named an option the config does not declare.
    ///
    /// Silent otherwise, because a variation with no declared options learns
    /// them from exactly these directives.
    pub fn undeclared(
        &self,
        found: &BTreeMap<String, BTreeSet<String>>,
        at: &str,
    ) -> Vec<Diagnostic> {
        let mut out = Vec::new();
        for (name, options) in found {
            let Some(decl) = self.decls.iter().find(|decl| decl.name == *name) else {
                out.push(
                    Diagnostic::new(
                        code::W0726,
                        format!(
                            "`{at}` uses variation `{name}`, which `variations` does not declare"
                        ),
                    )
                    .help(match self.names().is_empty() {
                        true => "declare it under `variations` in `liyasa.json`".to_owned(),
                        false => format!("declared variations are {}", quoted(&self.names())),
                    }),
                );
                continue;
            };
            if decl.options.is_empty() {
                continue;
            }
            for option in options {
                if !decl.options.iter().any(|one| one == option) {
                    out.push(
                        Diagnostic::new(
                            code::W0726,
                            format!(
                                "`{at}` uses `{name}=\"{option}\"`, which is not one of its \
                                 declared options"
                            ),
                        )
                        .help(format!(
                            "`{name}` declares {}",
                            quoted(&decl.options.iter().map(String::as_str).collect::<Vec<_>>())
                        )),
                    );
                }
            }
        }
        out
    }
}

fn quoted(names: &[&str]) -> String {
    names
        .iter()
        .map(|name| format!("`{name}`"))
        .collect::<Vec<_>>()
        .join(", ")
}

/// Every `:::variation{name="option"}` a page opens, as variation name to the
/// options it names.
///
/// Read from the scanner's own directive segments rather than from the text, so
/// a `:::variation` inside a fenced code block is documentation rather than a
/// declaration, and so this sees exactly what the parser will.
///
/// It runs before the registry, which is the point: `variation` has no
/// component yet, and the options a page uses are known whether or not one
/// exists to render them.
pub fn scan(document: &SourceDocument) -> BTreeMap<String, BTreeSet<String>> {
    let mut out: BTreeMap<String, BTreeSet<String>> = BTreeMap::new();
    for segment in &document.segments {
        let (name, props) = match segment {
            Segment::DirectiveOpen { name, props, .. } => (name, props),
            Segment::DirectiveLeaf { name, props, .. } => (name, props),
            _ => continue,
        };
        if name != DIRECTIVE {
            continue;
        }
        for (key, value) in &props.0 {
            let PropValue::Str(option) = value else {
                continue;
            };
            out.entry(key.clone()).or_default().insert(option.clone());
        }
    }
    out
}

/// The attributes one option's wrapper carries in the pre-rendered page.
///
/// Every option is present; `hidden` is what the switcher toggles. The
/// attribute rather than a class, so a reader with no JavaScript and no CSS
/// still sees one option rather than all of them at once.
pub fn attributes(name: &str, option: &str, shown: bool) -> Vec<(String, String)> {
    let mut out = vec![
        (DATA_ATTRIBUTE.to_owned(), DIRECTIVE.to_owned()),
        ("data-variation".to_owned(), name.to_owned()),
        ("data-option".to_owned(), option.to_owned()),
    ];
    if !shown {
        out.push(("hidden".to_owned(), String::new()));
    }
    out
}

#[cfg(test)]
mod tests {
    use liyasa_core::span::SourceId;

    use super::*;

    fn value(json: &str) -> Value {
        serde_json::from_str(json).expect("the fixture is JSON")
    }

    fn declared() -> Variations {
        Variations::from_value(&value(
            r#"{"variations":[{"name":"deployment","label":"Deployment",
                 "options":["cloud","self-hosted"]}]}"#,
        ))
    }

    fn scanned(text: &str) -> BTreeMap<String, BTreeSet<String>> {
        let (document, diagnostics) = liyasa_markdown::scan(text, SourceId(0));
        assert!(!diagnostics.has_errors(), "{diagnostics:?}");
        scan(&document)
    }

    fn set(names: &[&str]) -> BTreeSet<String> {
        names.iter().map(|n| (*n).to_owned()).collect()
    }

    #[test]
    fn a_directive_declares_the_option_it_names() {
        let found = scanned(
            ":::variation{deployment=\"cloud\"}\nRun `liyasa deploy`.\n:::\n\n\
             :::variation{deployment=\"self-hosted\"}\nRun the container.\n:::\n",
        );
        assert_eq!(found["deployment"], set(&["cloud", "self-hosted"]));
    }

    #[test]
    fn a_directive_inside_a_code_fence_is_documentation() {
        let found = scanned("```md\n:::variation{deployment=\"cloud\"}\n:::\n```\n");
        assert!(found.is_empty(), "{found:?}");
    }

    #[test]
    fn another_directive_is_not_a_variation() {
        let found = scanned(":::region{only=\"us\"}\nUS only.\n:::\n");
        assert!(found.is_empty(), "{found:?}");
    }

    #[test]
    fn a_declared_option_list_is_authoritative() {
        let mut variations = declared();
        variations.absorb(&BTreeMap::from([(
            "deployment".to_owned(),
            set(&["cloud", "hybrid"]),
        )]));
        assert_eq!(
            variations.options("deployment"),
            vec!["cloud".to_owned(), "self-hosted".to_owned()],
            "the config wins and keeps its own order"
        );
    }

    #[test]
    fn a_variation_with_no_declared_options_learns_them_from_the_content() {
        let mut variations =
            Variations::from_value(&value(r#"{"variations":[{"name":"deployment"}]}"#));
        assert!(variations.options("deployment").is_empty());
        variations.absorb(&scanned(
            ":::variation{deployment=\"self-hosted\"}\nx\n:::\n\
             :::variation{deployment=\"cloud\"}\ny\n:::\n",
        ));
        assert_eq!(
            variations.options("deployment"),
            vec!["cloud".to_owned(), "self-hosted".to_owned()],
            "a learned set is ordered, so two builds agree"
        );
        assert_eq!(
            variations.default_option("deployment").as_deref(),
            Some("cloud")
        );
    }

    #[test]
    fn a_variation_nothing_declares_is_never_learned() {
        let mut variations = declared();
        variations.absorb(&BTreeMap::from([("edition".to_owned(), set(&["pro"]))]));
        assert!(variations.options("edition").is_empty());
        assert!(!variations.declares("edition"));
    }

    #[test]
    fn an_undeclared_variation_is_reported_against_the_page() {
        let found = declared().undeclared(
            &BTreeMap::from([("edition".to_owned(), set(&["pro"]))]),
            "/pricing",
        );
        assert_eq!(found.len(), 1);
        assert_eq!(found[0].code.as_str(), "W0726");
        assert!(found[0].message.contains("`edition`"));
        assert_eq!(
            found[0].help.as_deref(),
            Some("declared variations are `deployment`")
        );
    }

    #[test]
    fn an_option_outside_a_declared_list_is_reported() {
        let found = declared().undeclared(
            &BTreeMap::from([("deployment".to_owned(), set(&["cloud", "hybrid"]))]),
            "/install",
        );
        assert_eq!(found.len(), 1);
        assert!(found[0].message.contains("hybrid"), "{}", found[0].message);
    }

    #[test]
    fn an_option_is_not_reported_when_the_config_declares_no_list() {
        let variations =
            Variations::from_value(&value(r#"{"variations":[{"name":"deployment"}]}"#));
        assert!(
            variations
                .undeclared(
                    &BTreeMap::from([("deployment".to_owned(), set(&["anything"]))]),
                    "/install"
                )
                .is_empty(),
            "the content is the source of the option list when the config has none"
        );
    }

    #[test]
    fn a_page_gets_a_switcher_only_for_the_variations_it_uses() {
        let variations = declared();
        assert!(
            variations
                .switchers(&BTreeSet::new(), &BTreeMap::new())
                .is_empty(),
            "a page that says the same thing for every option asks nothing"
        );
        let switchers = variations.switchers(&set(&["deployment"]), &BTreeMap::new());
        assert_eq!(switchers.len(), 1);
        assert_eq!(switchers[0].label, "Deployment");
        assert_eq!(switchers[0].choices.len(), 2);
        assert!(
            switchers[0].choices[0].current,
            "the first option is shown first"
        );
        assert!(!switchers[0].choices[1].current);
    }

    #[test]
    fn a_page_level_choice_overrides_the_default() {
        let switchers = declared().switchers(
            &set(&["deployment"]),
            &BTreeMap::from([("deployment".to_owned(), "self-hosted".to_owned())]),
        );
        assert!(!switchers[0].choices[0].current);
        assert!(switchers[0].choices[1].current);
    }

    #[test]
    fn one_option_is_not_a_choice() {
        let mut variations =
            Variations::from_value(&value(r#"{"variations":[{"name":"deployment"}]}"#));
        variations.absorb(&BTreeMap::from([(
            "deployment".to_owned(),
            set(&["cloud"]),
        )]));
        assert!(
            variations
                .switchers(&set(&["deployment"]), &BTreeMap::new())
                .is_empty(),
            "there is nothing to switch between"
        );
    }

    /// CM-111's "pre-rendered into a single HTML page with all options": every
    /// option is in the bytes, and the switcher changes which one is shown.
    #[test]
    fn every_option_is_in_the_page_and_the_hidden_ones_are_marked() {
        let shown = attributes("deployment", "cloud", true);
        assert!(shown.iter().all(|(name, _)| name != "hidden"));
        assert!(shown.contains(&("data-option".to_owned(), "cloud".to_owned())));

        let hidden = attributes("deployment", "self-hosted", false);
        assert!(
            hidden.contains(&("hidden".to_owned(), String::new())),
            "the attribute, so a reader with no CSS sees one option rather than both"
        );
    }
}
