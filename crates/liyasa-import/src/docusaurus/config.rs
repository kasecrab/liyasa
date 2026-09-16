//! `docusaurus.config.js` as `liyasa.json` (MIG-02).
//!
//! Like the Mintlify conversion, this is an allow list: §8's schema refuses an
//! unknown key, so a Docusaurus key is either mapped here or named in the
//! report. Docusaurus splits its settings between the top level, a preset's
//! options, and `themeConfig`, so the conversion reads all three.

use serde_json::{Map, Value, json};

use crate::report::{Attention, Kind, Redirect};

/// What one `docusaurus.config.js` became.
#[derive(Debug, Default)]
pub struct Config {
    pub value: Value,
    /// Where the docs plugin serves its pages, with no leading or trailing
    /// slash: `docs` by default, empty for `routeBasePath: '/'`.
    pub route_base: String,
    /// The directory the docs plugin reads, `docs` by default.
    pub docs_dir: String,
    /// The sidebars module the preset named.
    pub sidebars: Option<String>,
    /// Redirects declared by `@docusaurus/plugin-client-redirects`.
    pub redirects: Vec<Redirect>,
    pub attention: Vec<Attention>,
}

pub fn convert(source: &Value) -> Config {
    let mut out = Config {
        route_base: "docs".to_owned(),
        docs_dir: "docs".to_owned(),
        ..Config::default()
    };
    let Some(map) = source.as_object() else {
        out.attention.push(
            Attention::new(Kind::ConfigKey, "the whole file")
                .help("the module did not export an object"),
        );
        return out;
    };

    let mut site = Map::new();
    set(&mut site, "$schema", json!(schema_url()));
    let mut read: Vec<&str> = Vec::new();

    if let Some(title) = map.get("title") {
        set(&mut site, "name", title.clone());
        read.push("title");
    }
    if let Some(tagline) = map.get("tagline") {
        set(&mut site, "description", tagline.clone());
        read.push("tagline");
    }
    if let Some(Value::String(favicon)) = map.get("favicon") {
        // `static/` is served at the root, and the importer moves it there, so
        // the path the config names is already the path the site serves.
        let href = format!("/{}", favicon.trim_start_matches('/'));
        set(&mut site, "favicon", json!({ "light": href, "dark": href }));
        read.push("favicon");
    }
    if let Some(Value::String(url)) = map.get("url") {
        set(&mut site, "seo.canonicalOrigin", json!(url));
        read.push("url");
    }
    if let Some(Value::String(base)) = map.get("baseUrl") {
        let base = base.trim_matches('/');
        if !base.is_empty() {
            set(&mut site, "build.basePath", json!(format!("/{base}")));
        }
        read.push("baseUrl");
    }
    if let Some(Value::Bool(trailing)) = map.get("trailingSlash") {
        set(&mut site, "seo.trailingSlash", json!(trailing));
        read.push("trailingSlash");
    }

    if let Some(Value::Object(i18n)) = map.get("i18n") {
        let default = i18n.get("defaultLocale").and_then(Value::as_str);
        if let Some(Value::Array(locales)) = i18n.get("locales") {
            let list: Vec<Value> = locales
                .iter()
                .filter_map(Value::as_str)
                .map(|code| {
                    let mut entry = Map::new();
                    entry.insert("code".to_owned(), json!(code));
                    if Some(code) == default {
                        entry.insert("default".to_owned(), json!(true));
                    }
                    Value::Object(entry)
                })
                .collect();
            if !list.is_empty() {
                set(&mut site, "locales", Value::Array(list));
            }
        }
        read.push("i18n");
    }

    // The classic preset carries the docs and theme options.
    if let Some(Value::Array(presets)) = map.get("presets") {
        for preset in presets {
            let Some(options) = preset_options(preset) else {
                continue;
            };
            if let Some(Value::Object(docs)) = options.get("docs") {
                if let Some(Value::String(base)) = docs.get("routeBasePath") {
                    out.route_base = base.trim_matches('/').to_owned();
                }
                if let Some(Value::String(path)) = docs.get("path") {
                    out.docs_dir = path.trim_matches('/').to_owned();
                }
                if let Some(Value::String(sidebars)) = docs.get("sidebarPath") {
                    out.sidebars = Some(sidebars.trim_start_matches("./").to_owned());
                }
            }
            if let Some(Value::Object(theme)) = options.get("theme")
                && let Some(css) = theme.get("customCss")
            {
                set(&mut site, "theme.css", stylesheets(css));
            }
        }
        read.push("presets");
    }

    if let Some(Value::Object(theme)) = map.get("themeConfig") {
        theme_config(theme, &mut site, &mut out.attention);
        read.push("themeConfig");
    }

    if let Some(Value::Array(plugins)) = map.get("plugins") {
        for plugin in plugins {
            out.redirects.extend(client_redirects(plugin));
        }
        read.push("plugins");
    }
    if !out.redirects.is_empty() {
        set(
            &mut site,
            "redirects",
            Value::Array(
                out.redirects
                    .iter()
                    .map(|rule| json!({ "source": rule.source, "destination": rule.destination }))
                    .collect(),
            ),
        );
    }

    // Keys that configure a build Liyasa does not run are not losses, so they
    // are listed once rather than reported one by one as manual attention.
    const TOOLCHAIN: &[&str] = &[
        "organizationName",
        "projectName",
        "deploymentBranch",
        "onBrokenLinks",
        "onBrokenMarkdownLinks",
        "onDuplicateRoutes",
        "staticDirectories",
        "clientModules",
        "future",
        "customFields",
        "headTags",
        "stylesheets",
        "scripts",
        "markdown",
        "themes",
        "noIndex",
        "titleDelimiter",
        "baseUrlIssueBanner",
    ];
    for key in map.keys() {
        if read.contains(&key.as_str()) || TOOLCHAIN.contains(&key.as_str()) {
            continue;
        }
        out.attention.push(
            Attention::new(Kind::ConfigKey, key.clone())
                .help("§8 has no counterpart, so it was not carried"),
        );
    }

    out.value = Value::Object(site);
    out
}

/// `themeConfig`'s navbar, footer, and colour mode.
fn theme_config(
    theme: &Map<String, Value>,
    site: &mut Map<String, Value>,
    attention: &mut Vec<Attention>,
) {
    if let Some(Value::Object(navbar)) = theme.get("navbar") {
        if let Some(Value::Object(logo)) = navbar.get("logo") {
            let mut image = Map::new();
            if let Some(Value::String(src)) = logo.get("src") {
                image.insert(
                    "light".to_owned(),
                    json!(format!("/{}", src.trim_start_matches('/'))),
                );
            }
            let dark = logo.get("srcDark").or_else(|| logo.get("src"));
            if let Some(Value::String(src)) = dark {
                image.insert(
                    "dark".to_owned(),
                    json!(format!("/{}", src.trim_start_matches('/'))),
                );
            }
            if let Some(href) = logo.get("href") {
                image.insert("href".to_owned(), href.clone());
            }
            if !image.is_empty() {
                set(site, "logo", Value::Object(image));
            }
        }
        if let Some(Value::Array(items)) = navbar.get("items") {
            let links: Vec<Value> = items.iter().filter_map(navbar_link).collect();
            if !links.is_empty() {
                set(site, "navbar.links", Value::Array(links));
            }
        }
    }

    if let Some(Value::Object(footer)) = theme.get("footer") {
        if let Some(Value::Array(columns)) = footer.get("links") {
            let converted: Vec<Value> = columns.iter().map(footer_column).collect();
            set(site, "footer.links", Value::Array(converted));
        }
        if let Some(copyright) = footer.get("copyright") {
            set(site, "footer.text", copyright.clone());
        }
    }

    if let Some(Value::Object(mode)) = theme.get("colorMode") {
        if let Some(Value::String(default)) = mode.get("defaultMode") {
            set(site, "theme.appearance.default", json!(default));
        }
        if let Some(Value::Bool(disabled)) = mode.get("disableSwitch") {
            set(site, "theme.appearance.strict", json!(disabled));
        }
    }

    for key in ["algolia", "prism", "announcementBar"] {
        if theme.contains_key(key) {
            attention.push(
                Attention::new(Kind::ConfigKey, format!("themeConfig.{key}")).help(match key {
                    "algolia" => "Liyasa ships its own search (§12); no provider is configured",
                    "prism" => "code themes are `theme.codeTheme` (CFG-05)",
                    _ => "a site-wide notice is `banner` (CFG-70)",
                }),
            );
        }
    }
}

/// A navbar item that is a link. A `docSidebar` item points at a sidebar that
/// is already the navigation, and a dropdown of versions or locales is built
/// from `versions` and `locales`, so neither is a loss.
fn navbar_link(item: &Value) -> Option<Value> {
    let map = item.as_object()?;
    match map.get("type").and_then(Value::as_str) {
        Some("docSidebar" | "docsVersionDropdown" | "localeDropdown" | "search" | "doc") => None,
        _ => {
            let label = map.get("label")?.clone();
            let href = map.get("href").or_else(|| map.get("to"))?.clone();
            let mut link = Map::new();
            link.insert("label".to_owned(), label);
            link.insert("href".to_owned(), href);
            Some(Value::Object(link))
        }
    }
}

fn footer_column(column: &Value) -> Value {
    let Some(map) = column.as_object() else {
        return json!({});
    };
    let mut out = Map::new();
    if let Some(title) = map.get("title") {
        out.insert("header".to_owned(), title.clone());
    }
    if let Some(Value::Array(items)) = map.get("items") {
        let links: Vec<Value> = items
            .iter()
            .filter_map(|item| {
                let item = item.as_object()?;
                let mut link = Map::new();
                link.insert("label".to_owned(), item.get("label")?.clone());
                link.insert(
                    "href".to_owned(),
                    item.get("href").or_else(|| item.get("to"))?.clone(),
                );
                Some(Value::Object(link))
            })
            .collect();
        out.insert("items".to_owned(), Value::Array(links));
    }
    Value::Object(out)
}

/// `['@docusaurus/plugin-client-redirects', { redirects: [{ from, to }] }]`.
fn client_redirects(plugin: &Value) -> Vec<Redirect> {
    let Value::Array(pair) = plugin else {
        return Vec::new();
    };
    let named = pair.first().and_then(Value::as_str).unwrap_or("");
    if !named.contains("client-redirects") {
        return Vec::new();
    }
    let Some(Value::Array(rules)) = pair.get(1).and_then(|options| options.get("redirects")) else {
        return Vec::new();
    };
    let mut out = Vec::new();
    for rule in rules {
        let Some(map) = rule.as_object() else {
            continue;
        };
        let Some(to) = map.get("to").and_then(Value::as_str) else {
            continue;
        };
        match map.get("from") {
            Some(Value::String(from)) => out.push(Redirect {
                source: from.clone(),
                destination: to.to_owned(),
            }),
            Some(Value::Array(froms)) => {
                out.extend(froms.iter().filter_map(Value::as_str).map(|from| Redirect {
                    source: from.to_owned(),
                    destination: to.to_owned(),
                }))
            }
            _ => {}
        }
    }
    out
}

/// `['classic', { … }]` and `{ … }` both reach the options object.
fn preset_options(preset: &Value) -> Option<&Map<String, Value>> {
    match preset {
        Value::Array(pair) => pair.get(1)?.as_object(),
        Value::Object(map) => Some(map),
        _ => None,
    }
}

fn stylesheets(value: &Value) -> Value {
    let one = |path: &str| json!(format!("/{}", path.trim_start_matches("./")));
    match value {
        Value::String(path) => one(path),
        Value::Array(paths) => {
            Value::Array(paths.iter().filter_map(Value::as_str).map(one).collect())
        }
        other => other.clone(),
    }
}

fn schema_url() -> String {
    format!("{}liyasa.json", liyasa_core::site::SCHEMA_URL_BASE)
}

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
