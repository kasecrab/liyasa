//! The checks the schema cannot make (CFG-90).
//!
//! Everything here reads the merged JSON rather than the typed config, because
//! every diagnostic needs the JSON Pointer of the thing it is about and the
//! pointer is what maps back to a span.

use std::collections::{BTreeMap, BTreeSet};

use liyasa_core::diagnostics::{Diagnostic, Diagnostics, Severity, code};
use liyasa_core::span::Span;
use serde_json::Value;

use crate::color::{AA_NORMAL, Color, Parsed, contrast};
use crate::json::SpanIndex;
use crate::load::Load;
use crate::pages::Pages;

/// `liyasa serve` arrives in 0.5 (CFG-01).
const SERVE_AVAILABLE: bool = false;

/// `dev` fills the gaps in navigation, `build` reports them (CFG-32).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Mode {
    Dev,
    Build,
}

pub struct Context<'a> {
    pub pages: &'a Pages,
    pub mode: Mode,
}

pub fn validate_load(load: &Load, context: &Context<'_>) -> Diagnostics {
    validate(&load.value, &load.spans, context)
}

pub fn validate(config: &Value, spans: &SpanIndex, context: &Context<'_>) -> Diagnostics {
    let mut run = Run {
        config,
        spans,
        context,
        diagnostics: Diagnostics::new(),
    };
    run.privacy();
    run.colors();
    run.dimensions();
    run.navigation();
    run.redirects();
    run.seo();
    run.diagnostics
}

struct Run<'a> {
    config: &'a Value,
    spans: &'a SpanIndex,
    context: &'a Context<'a>,
    diagnostics: Diagnostics,
}

impl Run<'_> {
    fn at(&self, pointer: &str) -> Option<Span> {
        self.spans.nearest(pointer)
    }

    fn report(&mut self, diagnostic: Diagnostic, pointer: &str) {
        let diagnostic = match self.at(pointer) {
            Some(span) => diagnostic.at(span),
            None => diagnostic,
        };
        self.diagnostics.push(diagnostic);
    }

    /// CFG-01: a private site needs a server, which 0.1 does not have.
    fn privacy(&mut self) {
        if SERVE_AVAILABLE || self.config.get("public") != Some(&Value::Bool(false)) {
            return;
        }
        self.report(
            Diagnostic::new(
                code::E0120,
                "private sites need `liyasa serve`, available from 0.5",
            )
            .help("remove `public: false`, or host the built site behind your own access control"),
            "/public",
        );
    }

    /// CFG-04: every colour must be readable, and text on primary must clear
    /// WCAG AA.
    fn colors(&mut self) {
        let Some(colors) = self.config.pointer("/theme/colors") else {
            return;
        };
        let mut parsed: BTreeMap<&str, Color> = BTreeMap::new();
        let mut backgrounds: Vec<Color> = Vec::new();
        for (key, value) in colors.as_object().into_iter().flatten() {
            let Some(text) = value.as_str() else {
                continue; // `background` is an object; its two keys are below
            };
            match Color::parse(text) {
                Parsed::Known(color) => {
                    parsed.insert(key.as_str(), color);
                }
                Parsed::Malformed => self.report(
                    Diagnostic::new(
                        code::E0132,
                        format!("`{text}` is not a colour Liyasa can read"),
                    )
                    .help("write a hex colour such as `#4F46E5`, `rgb(…)`, or `hsl(…)`"),
                    &format!("/theme/colors/{key}"),
                ),
                Parsed::Unresolved => {}
            }
        }

        for scheme in ["light", "dark"] {
            let at = format!("/theme/colors/background/{scheme}");
            let Some(value) = self.config.pointer(&at).and_then(Value::as_str) else {
                continue;
            };
            match Color::parse(value) {
                Parsed::Known(color) => {
                    backgrounds.push(color);
                }
                Parsed::Malformed => self.report(
                    Diagnostic::new(
                        code::E0132,
                        format!("`{value}` is not a colour Liyasa can read"),
                    )
                    .help("write a hex colour such as `#4F46E5`, `rgb(…)`, or `hsl(…)`"),
                    &at,
                ),
                Parsed::Unresolved => {}
            }
        }

        // Primary is a fill, and the theme picks the label on it: white, the
        // configured text colour, or the page background, whichever clears AA
        // (`--ly-color-primary-contrast`). A colour is only reported when none
        // of them does, so a dark `text` over a dark `primary` is not a finding
        // — that pair never meets on a button.
        let mut labels = vec![Color::new(255, 255, 255)];
        labels.extend(parsed.get("text").copied());
        labels.extend(backgrounds);
        for key in ["primary", "light", "dark"] {
            let Some(color) = parsed.get(key).copied() else {
                continue;
            };
            let ratio = labels
                .iter()
                .map(|label| contrast(color, *label))
                .fold(f64::NEG_INFINITY, f64::max);
            if ratio < AA_NORMAL {
                self.report(
                    Diagnostic::new(
                        code::E0107,
                        format!(
                            "text on `theme.colors.{key}` has a contrast ratio of {ratio:.2}, \
                             below the WCAG AA minimum of {AA_NORMAL}"
                        ),
                    )
                    .with_severity(Severity::Warning)
                    .help(
                        "darken the colour, or set `theme.colors.text` to a label colour that clears AA",
                    ),
                    &format!("/theme/colors/{key}"),
                );
            }
        }
    }

    /// CFG-01: versions, locales, and dimensions each need exactly one default
    /// and no repeated name.
    fn dimensions(&mut self) {
        self.axis("versions", "name", "version");
        self.axis("locales", "code", "locale");
        for (at, dimension) in self.list("dimensions") {
            let Some(name) = dimension.get("name").and_then(Value::as_str) else {
                continue;
            };
            if dimension.get("default").and_then(Value::as_str).is_none() {
                self.report(
                    Diagnostic::new(
                        code::E0108,
                        format!("dimension `{name}` declares no default value"),
                    ),
                    &at,
                );
            }
        }
        for key in ["openapi", "asyncapi", "graphql"] {
            self.unique_ids(key);
        }
    }

    /// One of `versions` or `locales`: entries are either plain names, in which
    /// case the first is the default, or objects, in which case exactly one
    /// carries `default: true`.
    fn axis(&mut self, key: &str, name_key: &str, what: &str) {
        let entries = self.list(key);
        if entries.is_empty() {
            return;
        }
        let mut seen: BTreeSet<String> = BTreeSet::new();
        let mut defaults = Vec::new();
        let mut objects = 0usize;
        for (at, entry) in &entries {
            if entry.is_object() {
                objects += 1;
                if entry.get("default") == Some(&Value::Bool(true)) {
                    defaults.push(at.clone());
                }
            }
            let Some(name) = entry
                .as_str()
                .or_else(|| entry.get(name_key).and_then(Value::as_str))
            else {
                continue;
            };
            if !seen.insert(name.to_owned()) {
                self.report(
                    Diagnostic::new(code::E0105, format!("{what} `{name}` is declared twice")),
                    at,
                );
            }
        }

        if objects == 0 {
            return; // a bare list has no way to mark a default; the first wins
        }
        match defaults.len() {
            1 => {}
            0 => self.report(
                Diagnostic::new(code::E0108, format!("no default {what} is declared"))
                    .help(format!("set `default: true` on one entry of `{key}`")),
                &format!("/{key}"),
            ),
            _ => {
                for at in &defaults[1..] {
                    self.report(
                        Diagnostic::new(
                            code::E0108,
                            format!("more than one {what} is marked as the default"),
                        ),
                        at,
                    );
                }
            }
        }
    }

    fn unique_ids(&mut self, key: &str) {
        let mut seen: BTreeSet<String> = BTreeSet::new();
        for (at, entry) in self.list(key) {
            let Some(id) = entry.get("id").and_then(Value::as_str) else {
                continue;
            };
            if !seen.insert(id.to_owned()) {
                self.report(
                    Diagnostic::new(code::E0105, format!("`{key}` declares `{id}` twice")),
                    &at,
                );
            }
        }
    }

    /// CFG-30, CFG-32, CFG-35: every node resolves, nothing is listed twice,
    /// and every page is reachable.
    fn navigation(&mut self) {
        let (root, pointer) = match self.config.get("navigation") {
            Some(Value::Array(_)) => (self.config.get("navigation"), "/navigation".to_owned()),
            Some(Value::Object(object)) => match (object.get("pages"), object.get("tabs")) {
                (Some(pages), _) => (Some(pages), "/navigation/pages".to_owned()),
                (None, Some(tabs)) => (Some(tabs), "/navigation/tabs".to_owned()),
                (None, None) => (None, "/navigation".to_owned()),
            },
            _ => (None, "/navigation".to_owned()),
        };
        let Some(Value::Array(nodes)) = root else {
            return;
        };

        let mut visited = Visited::default();
        for (at, node) in nodes.iter().enumerate() {
            self.node(node, &format!("{pointer}/{at}"), &mut visited, 0);
        }

        if self.context.mode == Mode::Dev && self.autofill() {
            return; // the missing pages land in the "Other" group instead
        }
        let unreachable: Vec<String> = self
            .context
            .pages
            .iter()
            .filter(|route| !visited.routes.contains(*route))
            .map(str::to_owned)
            .collect();
        for route in unreachable {
            let shown = if route.is_empty() { "index" } else { &route };
            self.report(
                Diagnostic::new(
                    code::W0130,
                    format!("`{shown}` is not reachable from the navigation"),
                )
                .help("add it to `navigation`, or set `navigation.autofill: true` while drafting"),
                "/navigation",
            );
        }
    }

    fn autofill(&self) -> bool {
        self.config.pointer("/navigation/autofill") == Some(&Value::Bool(true))
    }

    fn node(&mut self, node: &Value, at: &str, visited: &mut Visited, depth: u8) {
        if depth > 32 {
            return;
        }
        match node {
            Value::String(path) => self.page(path, at, visited),
            Value::Object(object) => {
                if let Some(directory) = object.get("directory").and_then(Value::as_str) {
                    self.directory(directory, at, visited);
                }
                if let Some(root) = object.get("root").and_then(Value::as_str) {
                    self.page(root, at, visited);
                }
                for (key, kind) in [
                    ("version", "versions"),
                    ("language", "locales"),
                    ("product", "dimensions"),
                    ("openapi", "openapi"),
                    ("asyncapi", "asyncapi"),
                    ("graphql", "graphql"),
                ] {
                    if let Some(value) = object.get(key).and_then(Value::as_str) {
                        self.binding(key, kind, value, at);
                    }
                }
                for children in ["pages", "items"] {
                    let Some(Value::Array(nodes)) = object.get(children) else {
                        continue;
                    };
                    for (index, child) in nodes.iter().enumerate() {
                        self.node(
                            child,
                            &format!("{at}/{children}/{index}"),
                            visited,
                            depth + 1,
                        );
                    }
                }
            }
            _ => {}
        }
    }

    fn page(&mut self, path: &str, at: &str, visited: &mut Visited) {
        if path.contains('*') {
            let matched: Vec<String> = self
                .context
                .pages
                .matching(path)
                .into_iter()
                .map(str::to_owned)
                .collect();
            if matched.is_empty() {
                self.report(
                    Diagnostic::new(code::E0104, format!("`{path}` matches no page")),
                    at,
                );
            }
            for route in matched {
                visited.routes.insert(route);
            }
            return;
        }
        if path.starts_with("http://") || path.starts_with("https://") {
            return;
        }
        if !self.context.pages.contains(path) {
            self.report(
                Diagnostic::new(
                    code::E0104,
                    format!("navigation names `{path}`, which is not a page"),
                ),
                at,
            );
            return;
        }
        let route = normalized(path);
        if !visited.routes.insert(route.clone()) {
            let shown = if route.is_empty() { "index" } else { &route };
            self.report(
                Diagnostic::new(
                    code::E0105,
                    format!("`{shown}` appears more than once in the navigation"),
                ),
                at,
            );
        }
    }

    fn directory(&mut self, directory: &str, at: &str, visited: &mut Visited) {
        if !self.context.pages.has_directory(directory) {
            self.report(
                Diagnostic::new(code::E0104, format!("`{directory}` holds no pages")),
                at,
            );
            return;
        }
        for route in self.context.pages.matching(&format!("{directory}/**")) {
            visited.routes.insert(route.to_owned());
        }
        if self.context.pages.contains(directory) {
            visited.routes.insert(normalized(directory));
        }
    }

    /// CFG-35: a subtree bound to a version, locale, product, or spec that the
    /// config never declares.
    fn binding(&mut self, key: &str, kind: &str, value: &str, at: &str) {
        let declared = self.list(kind).into_iter().any(|(_, entry)| {
            entry.as_str() == Some(value)
                || ["name", "code", "id"]
                    .iter()
                    .any(|field| entry.get(*field).and_then(Value::as_str) == Some(value))
                || entry
                    .get("values")
                    .and_then(Value::as_array)
                    .is_some_and(|values| values.iter().any(|v| v.as_str() == Some(value)))
        });
        if !declared {
            self.report(
                Diagnostic::new(
                    code::E0133,
                    format!("navigation binds a subtree to `{key}: {value}`, which `{kind}` does not declare"),
                ),
                &format!("{at}/{key}"),
            );
        }
    }

    /// CFG-01: no rule may be written twice or shadow a page, and an absolute
    /// destination needs an allowed host.
    fn redirects(&mut self) {
        let (rules, allow) = match self.config.get("redirects") {
            Some(Value::Array(_)) => (self.list("redirects"), Vec::new()),
            Some(Value::Object(object)) => {
                let allow: Vec<String> = object
                    .get("externalAllow")
                    .and_then(Value::as_array)
                    .map(|hosts| {
                        hosts
                            .iter()
                            .filter_map(Value::as_str)
                            .map(str::to_owned)
                            .collect()
                    })
                    .unwrap_or_default();
                let rules = object
                    .get("rules")
                    .and_then(Value::as_array)
                    .map(|rules| {
                        rules
                            .iter()
                            .enumerate()
                            .map(|(at, rule)| (format!("/redirects/rules/{at}"), rule.clone()))
                            .collect()
                    })
                    .unwrap_or_default();
                (rules, allow)
            }
            _ => return,
        };

        let mut sources: BTreeSet<String> = BTreeSet::new();
        for (at, rule) in rules {
            if let Some(source) = rule.get("source").and_then(Value::as_str) {
                if !sources.insert(source.to_owned()) {
                    self.report(
                        Diagnostic::new(
                            code::E0106,
                            format!("two redirects claim the source `{source}`"),
                        ),
                        &format!("{at}/source"),
                    );
                } else if !source.contains(':') && self.context.pages.contains(source) {
                    self.report(
                        Diagnostic::new(
                            code::E0106,
                            format!("the redirect source `{source}` is also a page, which would never be served"),
                        ),
                        &format!("{at}/source"),
                    );
                }
            }
            if let Some(destination) = rule.get("destination").and_then(Value::as_str) {
                self.destination(destination, &allow, &format!("{at}/destination"));
            }
        }
    }

    fn destination(&mut self, destination: &str, allow: &[String], at: &str) {
        let Some(host) = external_host(destination) else {
            return;
        };
        if host.contains(':')
            && host
                .split(':')
                .next_back()
                .is_some_and(|p| p.parse::<u16>().is_err())
            || host.starts_with(':')
        {
            self.report(
                Diagnostic::new(
                    code::E0109,
                    format!("`{destination}` puts a route parameter in its host"),
                )
                .help("a parameter may appear in the path, never in the host"),
                at,
            );
            return;
        }
        let bare = host.split(':').next().unwrap_or(host);
        if !allow.iter().any(|allowed| allowed == bare) {
            self.report(
                Diagnostic::new(
                    code::E0109,
                    format!("`{bare}` is not in `redirects.externalAllow`"),
                )
                .help("add the host to `redirects.externalAllow` to redirect off this site"),
                at,
            );
        }
    }

    /// CFG-65: without an origin, `llms.txt`, the feeds, and the Markdown
    /// directive cannot write absolute URLs.
    fn seo(&mut self) {
        if self
            .config
            .pointer("/seo/canonicalOrigin")
            .and_then(Value::as_str)
            .is_some_and(|origin| !origin.is_empty())
        {
            return;
        }
        self.report(
            Diagnostic::new(
                code::W0131,
                "`seo.canonicalOrigin` is not set; absolute URLs cannot be generated",
            )
            .help("set it to the production origin, such as `https://docs.example.com`"),
            "/seo",
        );
    }

    /// One top-level array, as `(pointer, entry)` pairs.
    fn list(&self, key: &str) -> Vec<(String, Value)> {
        self.config
            .get(key)
            .and_then(Value::as_array)
            .map(|entries| {
                entries
                    .iter()
                    .enumerate()
                    .map(|(at, entry)| (format!("/{key}/{at}"), entry.clone()))
                    .collect()
            })
            .unwrap_or_default()
    }
}

#[derive(Default)]
struct Visited {
    routes: BTreeSet<String>,
}

fn normalized(path: &str) -> String {
    let mut pages = Pages::new();
    pages.insert(path);
    pages.iter().next().unwrap_or_default().to_owned()
}

/// The host of an absolute destination, or `None` when it stays on this site.
fn external_host(destination: &str) -> Option<&str> {
    let rest = destination
        .strip_prefix("https://")
        .or_else(|| destination.strip_prefix("http://"))
        .or_else(|| destination.strip_prefix("//"))?;
    Some(rest.split(['/', '?', '#']).next().unwrap_or(rest))
}
