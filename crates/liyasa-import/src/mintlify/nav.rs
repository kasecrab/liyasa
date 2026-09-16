//! A Mintlify navigation tree as a Liyasa one (§8.4, RFC 2900).
//!
//! §8.4's node shapes were modelled on Mintlify's, so most of this is a copy.
//! The two that are not are in RFC 2900: a node's children live under `groups`
//! in Mintlify and under `pages` in Liyasa, where `groups` means the reader
//! groups that gate the node instead; and a Mintlify anchor can hold children
//! where a Liyasa anchor is a link.

use std::collections::BTreeSet;

use serde_json::{Map, Value, json};

use crate::report::{Attention, Kind};

/// What one navigation tree became.
#[derive(Debug, Default)]
pub struct Nav {
    pub navigation: Value,
    /// Versions the tree declared, for the top-level `versions` key.
    pub versions: Vec<Value>,
    /// Locales the tree declared, for the top-level `locales` key.
    pub locales: Vec<Value>,
    /// Every page the tree names, for the dangling-entry check.
    pub pages: BTreeSet<String>,
    pub attention: Vec<Attention>,
}

/// Converts `docs.json`'s `navigation`, or `mint.json`'s array form.
pub fn convert(navigation: &Value) -> Nav {
    let mut nav = Nav::default();
    let tree = match navigation {
        Value::Array(items) => Value::Array(nav.nodes(items)),
        Value::Object(map) => {
            let mut object = Map::new();
            // The tree options Liyasa carries on the object form.
            for key in ["breadcrumbs", "autofill", "drilldown"] {
                if let Some(value) = map.get(key) {
                    object.insert(key.to_owned(), value.clone());
                }
            }
            // Not `children`: at the top level `tabs` is its own key of the
            // object form, and folding it into `pages` as well would list every
            // tab twice.
            let mut pages = Vec::new();
            for key in ["pages", "groups"] {
                if let Some(Value::Array(items)) = map.get(key) {
                    pages.extend(nav.nodes(items));
                }
            }
            if let Some(Value::Array(tabs)) = map.get("tabs") {
                object.insert("tabs".to_owned(), Value::Array(nav.nodes(tabs)));
            }
            for key in ["anchors", "dropdowns", "versions", "languages", "products"] {
                if let Some(Value::Array(items)) = map.get(key) {
                    pages.extend(nav.nodes(items));
                }
            }
            if let Some(Value::Object(global)) = map.get("global")
                && let Some(Value::Array(anchors)) = global.get("anchors")
            {
                pages.extend(nav.nodes(anchors));
            }
            if !pages.is_empty() {
                object.insert("pages".to_owned(), Value::Array(pages));
            }
            Value::Object(object)
        }
        Value::Null => Value::Array(Vec::new()),
        other => {
            nav.attention.push(
                Attention::new(Kind::ConfigKey, "navigation")
                    .help(format!("expected an array or an object, found {other}")),
            );
            Value::Array(Vec::new())
        }
    };
    nav.navigation = tree;
    nav
}

impl Nav {
    fn nodes(&mut self, items: &[Value]) -> Vec<Value> {
        items.iter().filter_map(|item| self.node(item)).collect()
    }

    fn node(&mut self, item: &Value) -> Option<Value> {
        match item {
            Value::String(path) => {
                self.pages.insert(path.clone());
                Some(item.clone())
            }
            Value::Object(map) => self.object(map),
            other => {
                self.attention.push(
                    Attention::new(Kind::ConfigKey, "navigation entry")
                        .help(format!("expected a page path or a node, found {other}")),
                );
                None
            }
        }
    }

    fn object(&mut self, map: &Map<String, Value>) -> Option<Value> {
        if let Some(name) = map.get("group") {
            return Some(self.section(
                "group",
                name,
                map,
                &["icon", "tag", "expanded", "root", "hidden"],
            ));
        }
        if let Some(name) = map.get("tab") {
            return Some(self.section("tab", name, map, &["icon", "hidden"]));
        }
        if let Some(name) = map.get("product") {
            return Some(self.section("product", name, map, &[]));
        }
        if let Some(name) = map.get("version") {
            self.versions.push(version(name, self.versions.is_empty()));
            return Some(self.section("version", name, map, &[]));
        }
        if let Some(name) = map.get("language") {
            self.locales.push(locale(name, self.locales.is_empty()));
            return Some(self.section("language", name, map, &[]));
        }
        if let Some(name) = map.get("dropdown") {
            let items = self.children(map);
            return Some(json!({ "dropdown": name, "items": items }));
        }
        if let Some(name) = map.get("menu") {
            let items = self.children(map);
            return Some(json!({ "menu": name, "items": items }));
        }
        if let Some(name) = map.get("anchor") {
            // TODO(rfc-2900): an anchor with children is a section, which is
            // what §8.4's `tab` node is; one with only an href is a link.
            let children = self.children(map);
            if children.is_empty() {
                let mut node = Map::new();
                node.insert("anchor".to_owned(), name.clone());
                for key in ["href", "icon", "color"] {
                    if let Some(value) = map.get(key) {
                        node.insert(key.to_owned(), value.clone());
                    }
                }
                return Some(Value::Object(node));
            }
            let mut node = Map::new();
            node.insert("tab".to_owned(), name.clone());
            if let Some(icon) = map.get("icon") {
                node.insert("icon".to_owned(), icon.clone());
            }
            node.insert("pages".to_owned(), Value::Array(children));
            return Some(Value::Object(node));
        }
        if let Some(spec) = map.get("openapi") {
            return Some(self.openapi(spec, map));
        }
        if let Some(Value::String(directory)) = map.get("directory") {
            return Some(json!({ "directory": directory }));
        }

        let keys: Vec<&str> = map.keys().map(String::as_str).collect();
        self.attention.push(
            Attention::new(
                Kind::ConfigKey,
                format!("navigation node {{{}}}", keys.join(", ")),
            )
            .help("no §8.4 node has these keys, so the entry was dropped"),
        );
        None
    }

    /// A node that names a section and holds children.
    fn section(
        &mut self,
        key: &str,
        name: &Value,
        map: &Map<String, Value>,
        carry: &[&str],
    ) -> Value {
        let mut node = Map::new();
        node.insert(key.to_owned(), name.clone());
        for extra in carry {
            if let Some(value) = map.get(*extra) {
                node.insert((*extra).to_owned(), value.clone());
            }
        }
        // A `groups` array of strings is already Liyasa's reader-group gate and
        // stays; an array of objects is Mintlify's children (RFC 2900).
        if let Some(Value::Array(groups)) = map.get("groups")
            && groups.iter().all(Value::is_string)
        {
            node.insert("groups".to_owned(), Value::Array(groups.clone()));
        }
        let children = self.children(map);
        if !children.is_empty() {
            node.insert("pages".to_owned(), Value::Array(children));
        }
        Value::Object(node)
    }

    /// Every child of a Mintlify node, from whichever key it used.
    fn children(&mut self, map: &Map<String, Value>) -> Vec<Value> {
        let mut out = Vec::new();
        if let Some(Value::Array(pages)) = map.get("pages") {
            out.extend(self.nodes(pages));
        }
        // TODO(rfc-2900): Mintlify's `groups` children fold into `pages`.
        if let Some(Value::Array(groups)) = map.get("groups")
            && !groups.iter().all(Value::is_string)
        {
            out.extend(self.nodes(groups));
        }
        if let Some(Value::Array(tabs)) = map.get("tabs") {
            out.extend(self.nodes(tabs));
        }
        if let Some(Value::Array(anchors)) = map.get("anchors") {
            out.extend(self.nodes(anchors));
        }
        if let Some(Value::Array(dropdowns)) = map.get("dropdowns") {
            out.extend(self.nodes(dropdowns));
        }
        // A spec named beside children is a sibling node, not a property: §8.4's
        // group node has no `openapi` key, and an entry that is only a spec was
        // already settled by `object`.
        if let Some(spec) = map.get("openapi") {
            out.push(self.openapi(spec, map));
        }
        out
    }

    fn openapi(&mut self, spec: &Value, map: &Map<String, Value>) -> Value {
        let mut node = Map::new();
        // Mintlify allows `"openapi": ["GET /a", "POST /b"]`, which names
        // operations of the project's single spec rather than a file.
        match spec {
            Value::Array(operations) => {
                node.insert("openapi".to_owned(), json!(""));
                node.insert("operations".to_owned(), Value::Array(operations.clone()));
            }
            other => {
                node.insert("openapi".to_owned(), other.clone());
                if let Some(operations) = map.get("operations") {
                    node.insert("operations".to_owned(), operations.clone());
                }
            }
        }
        Value::Object(node)
    }
}

fn version(name: &Value, first: bool) -> Value {
    let mut out = Map::new();
    out.insert("name".to_owned(), name.clone());
    if first {
        out.insert("default".to_owned(), Value::Bool(true));
    }
    Value::Object(out)
}

fn locale(code: &Value, first: bool) -> Value {
    let mut out = Map::new();
    out.insert("code".to_owned(), code.clone());
    if first {
        out.insert("default".to_owned(), Value::Bool(true));
    }
    Value::Object(out)
}
