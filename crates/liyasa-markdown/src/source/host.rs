//! The filters and functions whose answer comes from the build (CM-14, CM-15).
//!
//! `link`, `asset`, `page`, `pages`, `openapi`, `region_available`, and `now`
//! read the content tree, the asset manifest, the loaded specs, and the build
//! clock. This crate may not reach any of them: it does no I/O, so that it
//! builds for `wasm32-unknown-unknown` (§6.2). What it can do is hold the
//! answers. The build fills a [`Host`] once and installs it on the environment
//! it hands to [`expand`](super::expand); the semantics of each name — what it
//! resolves, what it raises when it cannot — stay here with the rest of CM-14
//! and CM-15 rather than being written again by every caller.
//!
//! [`environment`](super::expand::environment) installs an empty `Host`, so a
//! page that calls one of these in a build that supplied nothing is answered
//! with the code for the thing that was missing rather than with
//! `E0203 unknown filter`.

use std::collections::BTreeMap;
use std::sync::Arc;

use liyasa_core::diagnostics::code;
use minijinja::value::{Kwargs, Value};
use minijinja::{Environment, Error, ErrorKind};
use serde::{Deserialize, Serialize};

/// One page of the content tree, as a template sees it.
#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
pub struct PageEntry {
    /// The page ID `link` and `page` resolve.
    pub id: String,
    /// The route `seo.trailingSlash` has already been applied to (CM-02).
    pub route: String,
    /// The page's front matter, merged into the object a template receives so
    /// that `page("install").title` reads the front matter's `title`.
    #[serde(default)]
    pub data: serde_json::Value,
}

impl PageEntry {
    fn value(&self) -> Value {
        let mut fields = match &self.data {
            serde_json::Value::Object(fields) => fields.clone(),
            _ => serde_json::Map::new(),
        };
        fields.insert("id".to_owned(), serde_json::Value::String(self.id.clone()));
        fields.insert(
            "route".to_owned(),
            serde_json::Value::String(self.route.clone()),
        );
        Value::from_serialize(serde_json::Value::Object(fields))
    }
}

/// What the build knows and a page may ask it for.
///
/// Every field is empty by default and an empty field is not an error until a
/// page reads it, so a build that has not wired a surface up yet still renders
/// every page that does not use it.
#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
pub struct Host {
    /// The content tree in the order `pages()` lists it.
    #[serde(default)]
    pub pages: Vec<PageEntry>,
    /// Asset path to the URL that carries its cache-busting hash.
    #[serde(default)]
    pub assets: BTreeMap<String, String>,
    /// Spec name to its operations, by operation ID.
    #[serde(default)]
    pub openapi: BTreeMap<String, BTreeMap<String, serde_json::Value>>,
    /// The region this build is for, when it has a region dimension.
    #[serde(default)]
    pub region: Option<String>,
    /// Feature name to the regions it is available in.
    #[serde(default)]
    pub features: BTreeMap<String, Vec<String>>,
    /// The build clock of §6.6.2, RFC 3339. Never the wall clock: `now()` has
    /// no answer at all if the build did not set one.
    #[serde(default)]
    pub now: Option<String>,
}

impl Host {
    fn page(&self, id: &str) -> Option<&PageEntry> {
        self.pages.iter().find(|page| page.id == id)
    }
}

/// Where the host's `page` lookup stays reachable after CM-12's `page.*` has
/// shadowed the global of the same name.
///
/// CM-12 spells the front matter `page.title` and CM-15 spells the call
/// `page("id")`; the context wins the name, so the object expansion puts there
/// answers the attribute itself and delegates the call to this.
pub const PAGE_LOOKUP: &str = "__liyasa_page";

/// Installs the seven names a [`Host`] answers.
pub fn install(env: &mut Environment<'_>, host: Arc<Host>) {
    let at = host.clone();
    env.add_filter("link", move |id: &str| link(&at, id));
    let at = host.clone();
    env.add_filter("asset", move |path: &str| asset(&at, path));
    let lookup = Value::from_object(PageLookup(host.clone()));
    env.add_global("page", lookup.clone());
    env.add_global(PAGE_LOOKUP, lookup);
    let at = host.clone();
    env.add_function("pages", move |glob: &str, kwargs: Kwargs| {
        pages(&at, glob, kwargs)
    });
    let at = host.clone();
    env.add_function("openapi", move |spec: &str, operation: &str| {
        openapi(&at, spec, operation)
    });
    let at = host.clone();
    env.add_function("region_available", move |feature: &str| {
        region_available(&at, feature)
    });
    env.add_function("now", move || now(&host));
}

/// `page(id)`, reachable under its own name and under [`PAGE_LOOKUP`].
#[derive(Debug)]
struct PageLookup(Arc<Host>);

impl minijinja::value::Object for PageLookup {
    fn call(
        self: &Arc<Self>,
        _state: &minijinja::State<'_, '_>,
        args: &[Value],
    ) -> Result<Value, Error> {
        let [id] = args else {
            return Err(tagged(code::E0213, "`page` takes one page ID"));
        };
        let Some(id) = id.as_str() else {
            return Err(tagged(code::E0213, "`page` takes one page ID"));
        };
        page(&self.0, id)
    }
}

/// The page's own front matter, which also answers the `page(id)` call.
///
/// CM-12 puts the front matter in the context under `page`, where it shadows
/// CM-15's function of the same name. One object answers to both, the way
/// [`EnvAccessor`](super::filters::EnvAccessor) does for `env`.
#[derive(Debug)]
pub struct PageAccessor(pub Value);

impl minijinja::value::Object for PageAccessor {
    fn get_value(self: &Arc<Self>, key: &Value) -> Option<Value> {
        self.0.get_item(key).ok().filter(|f| !f.is_undefined())
    }

    fn enumerate(self: &Arc<Self>) -> minijinja::value::Enumerator {
        match self.0.try_iter() {
            Ok(keys) => minijinja::value::Enumerator::Values(keys.collect()),
            Err(_) => minijinja::value::Enumerator::NonEnumerable,
        }
    }

    fn call(
        self: &Arc<Self>,
        state: &minijinja::State<'_, '_>,
        args: &[Value],
    ) -> Result<Value, Error> {
        let Some(lookup) = state.lookup(PAGE_LOOKUP) else {
            return Err(tagged(
                code::E0213,
                "`page()` needs a content tree, and this build installed none",
            ));
        };
        lookup.call(state, args)
    }
}

fn tagged(code: liyasa_core::Code, message: impl std::fmt::Display) -> Error {
    Error::new(ErrorKind::InvalidOperation, format!("{code}: {message}"))
}

/// The closest ID to one that was not found, so `E0213` can suggest it (CM-18).
fn nearest<'a>(want: &str, known: impl Iterator<Item = &'a str>) -> Option<&'a str> {
    known
        .map(|id| (distance(want, id), id))
        .filter(|(at, _)| *at * 3 <= want.len().max(1))
        .min_by_key(|(at, id)| (*at, id.len()))
        .map(|(_, id)| id)
}

/// Levenshtein distance, two rows at a time.
fn distance(a: &str, b: &str) -> usize {
    let b: Vec<char> = b.chars().collect();
    let mut previous: Vec<usize> = (0..=b.len()).collect();
    let mut row = vec![0usize; b.len() + 1];
    for (at, left) in a.chars().enumerate() {
        row[0] = at + 1;
        for (to, right) in b.iter().enumerate() {
            let substitute = previous[to] + usize::from(left != *right);
            row[to + 1] = substitute.min(previous[to + 1] + 1).min(row[to] + 1);
        }
        std::mem::swap(&mut previous, &mut row);
    }
    previous[b.len()]
}

fn unknown_page(host: &Host, id: &str) -> Error {
    match nearest(id, host.pages.iter().map(|page| page.id.as_str())) {
        Some(near) => tagged(
            code::E0213,
            format!("no page `{id}`; did you mean `{near}`?"),
        ),
        None => tagged(code::E0213, format!("no page `{id}`")),
    }
}

// ---- CM-14 ----

/// `{{ "guides/install" | link }}` resolves a page ID to its route.
fn link(host: &Host, id: &str) -> Result<Value, Error> {
    // A fragment travels with the ID so that a cross-page anchor is one
    // expression rather than a concatenation the author has to get right.
    let (page_id, fragment) = match id.split_once('#') {
        Some((page_id, fragment)) => (page_id, Some(fragment)),
        None => (id, None),
    };
    let page = host
        .page(page_id)
        .ok_or_else(|| unknown_page(host, page_id))?;
    Ok(Value::from(match fragment {
        Some(fragment) => format!("{}#{fragment}", page.route),
        None => page.route.clone(),
    }))
}

/// `{{ "logo.svg" | asset }}` resolves an asset path to its cache-busted URL.
fn asset(host: &Host, path: &str) -> Result<Value, Error> {
    let wanted = path.trim_start_matches('/');
    match host.assets.get(wanted) {
        Some(url) => Ok(Value::from(url.clone())),
        None => Err(
            match nearest(wanted, host.assets.keys().map(String::as_str)) {
                Some(near) => tagged(
                    code::E0214,
                    format!("no asset `{path}`; did you mean `{near}`?"),
                ),
                None => tagged(code::E0214, format!("no asset `{path}`")),
            },
        ),
    }
}

// ---- CM-15 ----

fn page(host: &Host, id: &str) -> Result<Value, Error> {
    host.page(id)
        .map(PageEntry::value)
        .ok_or_else(|| unknown_page(host, id))
}

/// `pages("guides/*", draft=false)` lists the content tree in its own order.
///
/// A keyword filter compares against the page's front matter, so `pages()` is
/// the one query a listing page needs and not a query language.
fn pages(host: &Host, glob: &str, kwargs: Kwargs) -> Result<Value, Error> {
    let mut wanted: Vec<(String, Value)> = Vec::new();
    for key in kwargs.args() {
        wanted.push((key.to_owned(), kwargs.get(key)?));
    }
    kwargs.assert_all_used()?;

    let mut found = Vec::new();
    for entry in &host.pages {
        if !matches(glob, &entry.id) {
            continue;
        }
        let value = entry.value();
        let keep = wanted.iter().all(|(key, want)| {
            value
                .get_attr(key)
                .map(|found| found == *want)
                .unwrap_or(false)
        });
        if keep {
            found.push(value);
        }
    }
    Ok(Value::from(found))
}

/// `*` matches within one path segment, `**` across segments, `?` one
/// character. Everything else is literal.
fn matches(glob: &str, id: &str) -> bool {
    let pattern: Vec<char> = glob.chars().collect();
    let text: Vec<char> = id.chars().collect();
    walk(&pattern, &text)
}

fn walk(pattern: &[char], text: &[char]) -> bool {
    match pattern.first() {
        None => text.is_empty(),
        Some('*') => {
            let (rest, crosses) = match pattern.get(1) {
                Some('*') => (&pattern[2..], true),
                _ => (&pattern[1..], false),
            };
            // A `**` that is followed by a separator also matches nothing at
            // all, so `guides/**/x` finds `guides/x`.
            if crosses && rest.first() == Some(&'/') && walk(&rest[1..], text) {
                return true;
            }
            for at in 0..=text.len() {
                if !crosses && text[..at].contains(&'/') {
                    break;
                }
                if walk(rest, &text[at..]) {
                    return true;
                }
            }
            false
        }
        Some('?') => !text.is_empty() && text[0] != '/' && walk(&pattern[1..], &text[1..]),
        Some(ch) => text.first() == Some(ch) && walk(&pattern[1..], &text[1..]),
    }
}

fn openapi(host: &Host, spec: &str, operation: &str) -> Result<Value, Error> {
    let Some(operations) = host.openapi.get(spec) else {
        return Err(
            match nearest(spec, host.openapi.keys().map(String::as_str)) {
                Some(near) => tagged(
                    code::E0215,
                    format!("no OpenAPI spec `{spec}`; did you mean `{near}`?"),
                ),
                None => tagged(code::E0215, format!("no OpenAPI spec `{spec}`")),
            },
        );
    };
    match operations.get(operation) {
        Some(found) => Ok(Value::from_serialize(found)),
        None => Err(
            match nearest(operation, operations.keys().map(String::as_str)) {
                Some(near) => tagged(
                    code::E0215,
                    format!("spec `{spec}` has no operation `{operation}`; did you mean `{near}`?"),
                ),
                None => tagged(
                    code::E0215,
                    format!("spec `{spec}` has no operation `{operation}`"),
                ),
            },
        ),
    }
}

fn region_available(host: &Host, feature: &str) -> Result<Value, Error> {
    let Some(region) = &host.region else {
        return Err(tagged(
            code::E0216,
            format!("`region_available(\"{feature}\")` needs a region, and this build has none"),
        ));
    };
    let Some(regions) = host.features.get(feature) else {
        return Err(
            match nearest(feature, host.features.keys().map(String::as_str)) {
                Some(near) => tagged(
                    code::E0216,
                    format!("no feature `{feature}`; did you mean `{near}`?"),
                ),
                None => tagged(code::E0216, format!("no feature `{feature}`")),
            },
        );
    };
    Ok(Value::from(regions.iter().any(|at| at == region)))
}

/// The build clock of §6.6.2, never the wall clock: a build with no clock has
/// no answer, because answering with the wall clock would make two builds of
/// the same commit differ.
fn now(host: &Host) -> Result<Value, Error> {
    match &host.now {
        Some(clock) => Ok(Value::from(clock.clone())),
        None => Err(tagged(
            code::E0216,
            "`now()` is the build clock, and this build did not set one",
        )),
    }
}

#[cfg(test)]
mod tests;
