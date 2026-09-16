//! `docs.json`, and the legacy `mint.json`, as `liyasa.json` (MIG-01).
//!
//! The conversion is an allow list, not a copy: §8's schema sets
//! `additionalProperties: false`, so a key carried across on the hope that it
//! fits would fail `liyasa validate` (E0110) at the end of a migration rather
//! than during it. Every source key is either mapped here or named in the
//! report, and the leftovers are computed rather than listed, so a key Mintlify
//! adds tomorrow is reported instead of silently dropped.

use std::collections::BTreeSet;

use serde_json::{Map, Value, json};

use super::nav;
use crate::report::{Attention, Kind};

/// What one `docs.json` became.
#[derive(Debug, Default)]
pub struct Config {
    pub value: Value,
    /// Spec files the config named, for the `x-mint` rewrite.
    pub openapi: Vec<String>,
    /// Pages the navigation named, for the dangling-entry check.
    pub pages: BTreeSet<String>,
    pub attention: Vec<Attention>,
}

/// Every Liyasa icon library, because `theme.icons.library` is an enum and an
/// unknown value would fail validation.
const ICON_LIBRARIES: &[&str] = &["lucide", "phosphor", "tabler", "fontawesome"];

pub fn convert(source: &Value) -> Config {
    let mut out = Config::default();
    let Some(map) = source.as_object() else {
        out.attention.push(
            Attention::new(Kind::ConfigKey, "the whole file")
                .help("the configuration is not a JSON object"),
        );
        return out;
    };

    let mut site = Map::new();
    let mut seen: BTreeSet<&str> = BTreeSet::new();
    let mut take = |key: &'static str| -> Option<&Value> {
        seen.insert(key);
        map.get(key)
    };

    set(&mut site, "$schema", json!(schema_url()));
    if let Some(name) = take("name") {
        set(&mut site, "name", name.clone());
    }
    if let Some(description) = take("description") {
        set(&mut site, "description", description.clone());
    }

    // Colours and the preset. TODO(rfc-2900): Mintlify's preset names are brand
    // names for palettes with no Liyasa counterpart, so the palette carries and
    // the preset stays at its default.
    if let Some(Value::Object(colors)) = take("colors") {
        for (from, to) in [
            ("primary", "theme.colors.primary"),
            ("light", "theme.colors.light"),
            ("dark", "theme.colors.dark"),
            ("background", "theme.colors.background"),
        ] {
            if let Some(value) = colors.get(from) {
                set(&mut site, to, value.clone());
            }
        }
        for key in colors
            .keys()
            .filter(|key| !["primary", "light", "dark", "background"].contains(&key.as_str()))
        {
            out.attention.push(
                Attention::new(Kind::ConfigKey, format!("colors.{key}"))
                    .help("§8.2's palette has no counterpart for it"),
            );
        }
    }
    if let Some(theme) = take("theme") {
        out.attention.push(
            Attention::new(Kind::ConfigKey, format!("theme: {theme}")).help(
                "Liyasa's presets are their own designs (CFG-03); the palette and \
                 fonts carried across, so pick a preset from the gallery",
            ),
        );
    }

    for (from, to) in [("logo", "logo"), ("favicon", "favicon")] {
        if let Some(value) = take(from) {
            set(&mut site, to, image(value));
        }
    }

    if let Some(Value::Object(fonts)) = take("fonts") {
        for slot in ["heading", "body", "mono"] {
            if let Some(font) = fonts.get(slot) {
                set(&mut site, &format!("theme.fonts.{slot}"), font_of(font));
            }
        }
        // A family named at the top level applies to body text.
        if fonts.contains_key("family") {
            set(
                &mut site,
                "theme.fonts.body",
                font_of(&Value::Object(fonts.clone())),
            );
        }
    }

    if let Some(Value::Object(icons)) = take("icons")
        && let Some(Value::String(library)) = icons.get("library")
    {
        if ICON_LIBRARIES.contains(&library.as_str()) {
            set(&mut site, "theme.icons.library", json!(library));
        } else {
            out.attention.push(
                Attention::new(Kind::ConfigKey, format!("icons.library: {library}"))
                    .help("CFG-07 allows lucide, phosphor, tabler, and fontawesome"),
            );
        }
    }

    if let Some(Value::Object(appearance)) = take("appearance") {
        for (from, to) in [
            ("default", "theme.appearance.default"),
            ("strict", "theme.appearance.strict"),
        ] {
            if let Some(value) = appearance.get(from) {
                set(&mut site, to, value.clone());
            }
        }
    }
    if let Some(Value::Object(background)) = take("background") {
        for (from, to) in [
            ("image", "theme.appearance.background.image"),
            ("color", "theme.appearance.background.color"),
            ("decoration", "theme.appearance.background.decoration"),
        ] {
            if let Some(value) = background.get(from) {
                set(&mut site, to, value.clone());
            }
        }
    }

    if let Some(Value::Object(navbar)) = take("navbar") {
        if let Some(links) = navbar.get("links") {
            set(&mut site, "navbar.links", links.clone());
        }
        if let Some(primary) = navbar.get("primary") {
            set(&mut site, "navbar.primary", primary.clone());
        }
    }
    if let Some(Value::Object(footer)) = take("footer") {
        for key in ["socials", "links"] {
            if let Some(value) = footer.get(key) {
                set(&mut site, &format!("footer.{key}"), value.clone());
            }
        }
    }

    for key in ["banner", "redirects", "integrations", "errors"] {
        if let Some(value) = take(key) {
            set(&mut site, key, value.clone());
        }
    }
    if let Some(Value::Object(seo)) = take("seo") {
        for key in ["metatags", "indexing"] {
            if let Some(value) = seo.get(key) {
                set(&mut site, &format!("seo.{key}"), value.clone());
            }
        }
    }
    if let Some(Value::Object(search)) = take("search")
        && let Some(prompt) = search.get("prompt")
    {
        set(&mut site, "search.placeholder", prompt.clone());
    }
    if let Some(thumbnails) = take("thumbnails") {
        set(&mut site, "social.thumbnails", thumbnails.clone());
    }

    // Legacy `mint.json` shapes (MIG-01 carries both).
    if let Some(Value::Array(links)) = take("topbarLinks") {
        set(
            &mut site,
            "navbar.links",
            Value::Array(links.iter().map(nav_link).collect()),
        );
    }
    if let Some(Value::Object(cta)) = take("topbarCtaButton") {
        set(&mut site, "navbar.primary", primary_of(cta));
    }
    if let Some(socials) = take("footerSocials") {
        set(&mut site, "footer.socials", socials.clone());
    }
    if let Some(analytics) = take("analytics") {
        set(&mut site, "integrations", analytics.clone());
    }
    if let Some(Value::Object(toggle)) = take("modeToggle") {
        if let Some(value) = toggle.get("default") {
            set(&mut site, "theme.appearance.default", value.clone());
        }
        if let Some(Value::Bool(hidden)) = toggle.get("isHidden") {
            set(&mut site, "theme.appearance.strict", json!(hidden));
        }
    }
    if let Some(Value::String(image)) = take("backgroundImage") {
        set(&mut site, "theme.appearance.background.image", json!(image));
    }

    // Versions and locales can be declared at the top level or inside the tree.
    let declared_versions = take("versions").cloned();
    let declared_locales = take("languages").cloned();

    // OpenAPI, from either the modern `api.openapi` or the legacy top level.
    let mut specs = Vec::new();
    if let Some(Value::Object(api)) = take("api") {
        if let Some(value) = api.get("openapi") {
            specs.extend(spec_paths(value));
        }
        for key in api.keys().filter(|key| key.as_str() != "openapi") {
            out.attention.push(
                Attention::new(Kind::ConfigKey, format!("api.{key}"))
                    .help("§13's playground and reference options are configured per spec"),
            );
        }
    }
    if let Some(value) = take("openapi") {
        specs.extend(spec_paths(value));
    }
    specs.sort();
    specs.dedup();
    if !specs.is_empty() {
        set(
            &mut site,
            "openapi",
            Value::Array(specs.iter().map(|path| json!(path)).collect()),
        );
    }
    out.openapi = specs;

    // The navigation tree, which also declares versions and locales.
    let tree = take("navigation").cloned().unwrap_or(Value::Null);
    let mut navigation = nav::convert(&tree);
    let empty = navigation.navigation.as_array().is_some_and(Vec::is_empty)
        || navigation.navigation.as_object().is_some_and(Map::is_empty);
    if !navigation.navigation.is_null() && !empty {
        set(&mut site, "navigation", navigation.navigation.clone());
    }
    out.pages = std::mem::take(&mut navigation.pages);
    out.attention.append(&mut navigation.attention);

    match declared_versions {
        Some(Value::Array(items)) if !items.is_empty() => {
            set(&mut site, "versions", Value::Array(items));
        }
        _ if !navigation.versions.is_empty() => {
            set(
                &mut site,
                "versions",
                Value::Array(navigation.versions.clone()),
            );
        }
        _ => {}
    }
    match declared_locales {
        Some(Value::Array(items)) if !items.is_empty() => {
            set(
                &mut site,
                "locales",
                Value::Array(items.iter().map(locale_of).collect()),
            );
        }
        _ if !navigation.locales.is_empty() => {
            set(
                &mut site,
                "locales",
                Value::Array(navigation.locales.clone()),
            );
        }
        _ => {}
    }

    // Anything the source declared and this function did not read.
    seen.insert("$schema");
    for key in map.keys().filter(|key| !seen.contains(key.as_str())) {
        out.attention.push(
            Attention::new(Kind::ConfigKey, key.clone())
                .help("§8 has no counterpart, so it was not carried"),
        );
    }

    out.value = Value::Object(site);
    out
}

fn schema_url() -> String {
    format!("{}liyasa.json", liyasa_core::site::SCHEMA_URL_BASE)
}

/// `"logo.svg"` and `{"light": …, "dark": …}` both reach §8.2's object form.
fn image(value: &Value) -> Value {
    match value {
        Value::String(path) => json!({ "light": path, "dark": path }),
        Value::Object(map) => {
            let mut out = Map::new();
            for key in ["light", "dark", "href"] {
                if let Some(found) = map.get(key) {
                    out.insert(key.to_owned(), found.clone());
                }
            }
            Value::Object(out)
        }
        other => other.clone(),
    }
}

fn font_of(value: &Value) -> Value {
    let Some(map) = value.as_object() else {
        return json!({ "family": value });
    };
    let mut out = Map::new();
    for key in ["family", "weight", "source", "format"] {
        if let Some(found) = map.get(key) {
            out.insert(key.to_owned(), found.clone());
        }
    }
    Value::Object(out)
}

/// `{"name": …, "url": …}` is `mint.json`'s spelling of a navbar link.
fn nav_link(value: &Value) -> Value {
    let Some(map) = value.as_object() else {
        return value.clone();
    };
    let mut out = Map::new();
    if let Some(name) = map.get("name").or_else(|| map.get("label")) {
        out.insert("label".to_owned(), name.clone());
    }
    if let Some(url) = map.get("url").or_else(|| map.get("href")) {
        out.insert("href".to_owned(), url.clone());
    }
    if let Some(icon) = map.get("icon") {
        out.insert("icon".to_owned(), icon.clone());
    }
    Value::Object(out)
}

fn primary_of(cta: &Map<String, Value>) -> Value {
    let mut out = Map::new();
    let kind = match cta.get("type") {
        Some(Value::String(kind)) if kind == "github" => "github",
        _ => "button",
    };
    out.insert("type".to_owned(), json!(kind));
    if let Some(name) = cta.get("name").or_else(|| cta.get("label")) {
        out.insert("label".to_owned(), name.clone());
    }
    if let Some(url) = cta.get("url").or_else(|| cta.get("href")) {
        out.insert("href".to_owned(), url.clone());
    }
    Value::Object(out)
}

fn locale_of(value: &Value) -> Value {
    match value {
        Value::Object(map) => {
            let mut out = Map::new();
            if let Some(code) = map.get("language").or_else(|| map.get("code")) {
                out.insert("code".to_owned(), code.clone());
            }
            for key in ["default", "label"] {
                if let Some(found) = map.get(key) {
                    out.insert(key.to_owned(), found.clone());
                }
            }
            Value::Object(out)
        }
        other => other.clone(),
    }
}

/// Every spec path a Mintlify `openapi` value names.
fn spec_paths(value: &Value) -> Vec<String> {
    match value {
        Value::String(path) => vec![path.clone()],
        Value::Array(items) => items.iter().flat_map(spec_paths).collect(),
        Value::Object(map) => map
            .get("source")
            .or_else(|| map.get("openapi"))
            .map(spec_paths)
            .unwrap_or_default(),
        _ => Vec::new(),
    }
}

/// Sets a dotted path, creating the objects along the way.
fn set(site: &mut Map<String, Value>, path: &str, value: Value) {
    let mut at = site;
    let mut parts = path.split('.').peekable();
    while let Some(part) = parts.next() {
        if parts.peek().is_none() {
            at.insert(part.to_owned(), value);
            return;
        }
        at = at
            .entry(part.to_owned())
            .or_insert_with(|| Value::Object(Map::new()))
            .as_object_mut()
            .expect("the path was built as objects");
    }
}
