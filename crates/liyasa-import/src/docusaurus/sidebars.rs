//! `sidebars.js` as a Liyasa navigation tree (§8.4, RFC 2901).
//!
//! A Docusaurus sidebar item names a *document id*; a Liyasa navigation string
//! names a *page path*. They differ by the route base, so every id is prefixed
//! with wherever the importer put that plugin's pages.

use std::collections::BTreeSet;

use serde_json::{Map, Value, json};

use crate::report::{Attention, Kind};

#[derive(Debug, Default)]
pub struct Sidebars {
    pub navigation: Value,
    /// Every page the tree names, for the dangling-entry check.
    pub pages: BTreeSet<String>,
    pub attention: Vec<Attention>,
}

/// Converts the object `sidebars.js` exports.
///
/// `prefix` is where the plugin's pages landed, so a doc id becomes the path
/// that serves it.
pub fn convert(exported: &Value, prefix: &str) -> Sidebars {
    let mut out = Sidebars::default();
    let Some(map) = exported.as_object() else {
        out.attention.push(
            Attention::new(Kind::ConfigKey, "sidebars")
                .help("the module did not export an object of named sidebars"),
        );
        return out;
    };

    let named: Vec<(&String, &Value)> = map.iter().collect();
    // TODO(rfc-2901): one sidebar is the navigation; several become tabs.
    let tree = match named.as_slice() {
        [] => Value::Array(Vec::new()),
        [(_, only)] => Value::Array(out.items(only, prefix)),
        several => Value::Array(
            several
                .iter()
                .map(|(name, items)| {
                    let pages = out.items(items, prefix);
                    json!({ "tab": tab_label(name), "pages": pages })
                })
                .collect(),
        ),
    };
    out.navigation = tree;
    out
}

/// `tutorialSidebar` reads as "Tutorial"; `apiSidebar` as "Api". The operator
/// renames it in one edit, which is better than inventing a dictionary.
fn tab_label(name: &str) -> String {
    let stem = name.strip_suffix("Sidebar").unwrap_or(name);
    let stem = stem.strip_suffix("_sidebar").unwrap_or(stem);
    let mut out = String::with_capacity(stem.len() + 2);
    for (at, ch) in stem.char_indices() {
        if at == 0 {
            out.extend(ch.to_uppercase());
        } else if ch.is_ascii_uppercase() {
            out.push(' ');
            out.push(ch);
        } else if ch == '_' || ch == '-' {
            out.push(' ');
        } else {
            out.push(ch);
        }
    }
    out
}

impl Sidebars {
    fn items(&mut self, value: &Value, prefix: &str) -> Vec<Value> {
        match value {
            Value::Array(items) => items
                .iter()
                .filter_map(|item| self.item(item, prefix))
                .collect(),
            other => self.item(other, prefix).into_iter().collect(),
        }
    }

    fn item(&mut self, value: &Value, prefix: &str) -> Option<Value> {
        match value {
            Value::String(id) => {
                let path = with_prefix(prefix, id);
                self.pages.insert(path.clone());
                Some(json!(path))
            }
            Value::Object(map) => self.object(map, prefix),
            other => {
                self.attention.push(
                    Attention::new(Kind::ConfigKey, "sidebar item")
                        .help(format!("expected a document id or an item, found {other}")),
                );
                None
            }
        }
    }

    fn object(&mut self, map: &Map<String, Value>, prefix: &str) -> Option<Value> {
        let kind = map.get("type").and_then(Value::as_str).unwrap_or("");
        match kind {
            "doc" | "ref" => {
                let id = map.get("id").and_then(Value::as_str)?;
                let path = with_prefix(prefix, id);
                self.pages.insert(path.clone());
                Some(json!(path))
            }
            "category" => {
                let label = map.get("label").cloned().unwrap_or(json!("Untitled"));
                let mut node = Map::new();
                node.insert("group".to_owned(), label);
                // Docusaurus collapses by default; §8.4 expands by default.
                if map.get("collapsed").and_then(Value::as_bool) == Some(false) {
                    node.insert("expanded".to_owned(), json!(true));
                }
                // `link: {type: 'doc', id}` is the category's own landing page,
                // which §8.4 spells `root`.
                if let Some(Value::Object(link)) = map.get("link")
                    && let Some(id) = link.get("id").and_then(Value::as_str)
                {
                    node.insert("root".to_owned(), json!(with_prefix(prefix, id)));
                    self.pages.insert(with_prefix(prefix, id));
                }
                let items = map.get("items").map(|items| self.items(items, prefix));
                if let Some(items) = items
                    && !items.is_empty()
                {
                    node.insert("pages".to_owned(), Value::Array(items));
                }
                Some(Value::Object(node))
            }
            "link" => {
                let mut node = Map::new();
                node.insert(
                    "anchor".to_owned(),
                    map.get("label").cloned().unwrap_or(json!("Link")),
                );
                if let Some(href) = map.get("href") {
                    node.insert("href".to_owned(), href.clone());
                }
                Some(Value::Object(node))
            }
            "autogenerated" => {
                let dir = map.get("dirName").and_then(Value::as_str).unwrap_or(".");
                Some(json!({ "directory": with_prefix(prefix, dir) }))
            }
            "html" => {
                self.attention.push(
                    Attention::new(Kind::ConfigKey, "sidebar item of type `html`")
                        .help("§8.4 has no raw-HTML node; add the markup to a theme partial"),
                );
                None
            }
            "" => {
                // The shorthand `{ "Category": [items] }` of Docusaurus v1.
                let mut nodes = Vec::new();
                for (label, items) in map {
                    let pages = self.items(items, prefix);
                    nodes.push(json!({ "group": label, "pages": pages }));
                }
                match nodes.len() {
                    0 => None,
                    1 => nodes.pop(),
                    _ => Some(Value::Array(nodes)),
                }
            }
            other => {
                self.attention.push(
                    Attention::new(Kind::ConfigKey, format!("sidebar item of type `{other}`"))
                        .help("§8.4 has no counterpart, so the entry was dropped"),
                );
                None
            }
        }
    }
}

/// A document id under the route base its pages were written to.
fn with_prefix(prefix: &str, id: &str) -> String {
    let id = id.trim_start_matches('/');
    if prefix.is_empty() {
        return id.to_owned();
    }
    if id == "." {
        return prefix.to_owned();
    }
    format!("{prefix}/{id}")
}
