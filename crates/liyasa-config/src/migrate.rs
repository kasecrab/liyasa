//! `liyasa migrate-config` (CLI-14, CFG-91).
//!
//! Migrations are an ordered chain, one step per schema major version, so the
//! next one is an entry rather than a rewrite. What v0 is, and why it had to be
//! defined here at all, is `plan/rfcs/0103-config-schema-v0.md`.

use liyasa_core::diagnostics::{Diagnostic, Diagnostics, code};
use serde_json::{Map, Value, json};

use crate::json::SpanIndex;
use crate::schema::{CONFIG_SCHEMA_VERSION, config_schema_id, declared_version};

/// One rewrite the command prints. An empty `to` means the key had no home in
/// the new version and was dropped.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Change {
    pub from: String,
    pub to: String,
    pub note: String,
}

impl Change {
    fn moved(from: &str, to: &str, note: &str) -> Self {
        Self {
            from: from.to_owned(),
            to: to.to_owned(),
            note: note.to_owned(),
        }
    }

    fn dropped(from: &str) -> Self {
        Self {
            from: from.to_owned(),
            to: String::new(),
            note: "no key in v1 carries this".to_owned(),
        }
    }
}

#[derive(Debug, Clone)]
pub struct Migrated {
    pub value: Value,
    /// The migrated config, pretty-printed and newline-terminated.
    pub json: String,
    pub changes: Vec<Change>,
    pub diagnostics: Diagnostics,
}

type Step = fn(&Value, &mut Vec<Change>) -> Value;

/// `STEPS[n]` takes a config at version `n` to version `n + 1`.
const STEPS: &[Step] = &[v0_to_v1];

pub fn migrate(config: &Value, spans: &SpanIndex) -> Migrated {
    let mut diagnostics = Diagnostics::new();
    // A config with no `$schema` predates the key, so it is v0.
    let from = declared_version(config, spans, &mut diagnostics).unwrap_or(0);

    let mut changes = Vec::new();
    let mut value = config.clone();
    if from > CONFIG_SCHEMA_VERSION {
        return Migrated {
            json: pretty(&value),
            value,
            changes,
            diagnostics,
        };
    }
    for step in &STEPS[from as usize..] {
        value = step(&value, &mut changes);
    }
    if !changes.is_empty()
        && let Some(object) = value.as_object_mut()
    {
        object.insert("$schema".to_owned(), json!(config_schema_id()));
    }

    Migrated {
        json: pretty(&value),
        value,
        changes,
        diagnostics,
    }
}

/// A one-line summary for the command to print above the diff.
pub fn summary(migrated: &Migrated) -> String {
    match migrated.changes.len() {
        0 => "already at the current schema version".to_owned(),
        1 => "1 key rewritten".to_owned(),
        n => format!("{n} keys rewritten"),
    }
}

fn pretty(value: &Value) -> String {
    let mut json = serde_json::to_string_pretty(value).unwrap_or_default();
    json.push('\n');
    json
}

/// RFC 0103's table.
fn v0_to_v1(config: &Value, changes: &mut Vec<Change>) -> Value {
    let Some(old) = config.as_object() else {
        return config.clone();
    };
    let mut new = Map::new();
    let mut theme = Map::new();
    let mut navbar = Map::new();
    let mut footer = Map::new();
    let mut anchors: Vec<Value> = Vec::new();
    let mut tabs: Vec<Value> = Vec::new();

    for (key, value) in old {
        match key.as_str() {
            "$schema" => {}
            "name" | "description" | "versions" | "locales" | "redirects" | "seo" | "build" => {
                new.insert(key.clone(), value.clone());
            }
            "navigation" => {
                new.insert("navigation".to_owned(), value.clone());
            }
            "logo" | "favicon" => match value.as_str() {
                Some(path) => {
                    new.insert(key.clone(), json!({ "light": path }));
                    changes.push(Change::moved(
                        key,
                        &format!("{key}.light"),
                        "a single image is the light variant",
                    ));
                }
                None => {
                    new.insert(key.clone(), value.clone());
                }
            },
            "colors" => {
                theme.insert("colors".to_owned(), value.clone());
                changes.push(Change::moved(
                    "colors",
                    "theme.colors",
                    "colours moved under `theme`",
                ));
            }
            "font" => {
                theme.insert("fonts".to_owned(), fonts(value));
                changes.push(Change::moved(
                    "font",
                    "theme.fonts",
                    "`headings` is now `heading`",
                ));
            }
            "modeToggle" => {
                theme.insert("appearance".to_owned(), appearance(value));
                changes.push(Change::moved(
                    "modeToggle",
                    "theme.appearance",
                    "`isHidden` is now `strict`",
                ));
            }
            "topbarLinks" => {
                navbar.insert("links".to_owned(), links(value));
                changes.push(Change::moved(
                    "topbarLinks",
                    "navbar.links",
                    "`name` and `url` are now `label` and `href`",
                ));
            }
            "topbarCtaButton" => {
                navbar.insert("primary".to_owned(), cta(value));
                changes.push(Change::moved(
                    "topbarCtaButton",
                    "navbar.primary",
                    "`name` and `url` are now `label` and `href`",
                ));
            }
            "footerSocials" => {
                footer.insert("socials".to_owned(), value.clone());
                changes.push(Change::moved(
                    "footerSocials",
                    "footer.socials",
                    "grouped under `footer`",
                ));
            }
            "anchors" => {
                anchors = value
                    .as_array()
                    .map(|items| items.iter().map(anchor).collect())
                    .unwrap_or_default();
                changes.push(Change::moved(
                    "anchors",
                    "navigation",
                    "anchors are navigation nodes",
                ));
            }
            "tabs" => {
                tabs = value
                    .as_array()
                    .map(|items| items.iter().map(tab).collect())
                    .unwrap_or_default();
                changes.push(Change::moved(
                    "tabs",
                    "navigation",
                    "tabs are navigation nodes",
                ));
            }
            "openApi" => {
                new.insert("openapi".to_owned(), specs(value));
                changes.push(Change::moved(
                    "openApi",
                    "openapi",
                    "each spec now carries an `id` navigation can name",
                ));
            }
            "analytics" => {
                new.insert("integrations".to_owned(), value.clone());
                changes.push(Change::moved(
                    "analytics",
                    "integrations",
                    "third-party scripts are integrations",
                ));
            }
            other => changes.push(Change::dropped(other)),
        }
    }

    if !anchors.is_empty() || !tabs.is_empty() {
        let mut nodes = match new.remove("navigation") {
            Some(Value::Array(nodes)) => nodes,
            Some(other) => vec![other],
            None => Vec::new(),
        };
        nodes.extend(tabs);
        nodes.extend(anchors);
        new.insert("navigation".to_owned(), Value::Array(nodes));
    }
    for (key, section) in [("theme", theme), ("navbar", navbar), ("footer", footer)] {
        if !section.is_empty() {
            new.insert(key.to_owned(), Value::Object(section));
        }
    }
    Value::Object(new)
}

fn fonts(value: &Value) -> Value {
    let mut out = Map::new();
    for (key, face) in value.as_object().into_iter().flatten() {
        let renamed = if key == "headings" { "heading" } else { key };
        out.insert(renamed.to_owned(), face.clone());
    }
    Value::Object(out)
}

fn appearance(value: &Value) -> Value {
    let mut out = Map::new();
    if let Some(default) = value.get("default") {
        out.insert("default".to_owned(), default.clone());
    }
    if let Some(hidden) = value.get("isHidden") {
        out.insert("strict".to_owned(), hidden.clone());
    }
    Value::Object(out)
}

fn links(value: &Value) -> Value {
    let items = value
        .as_array()
        .map(|items| items.iter().map(link).collect())
        .unwrap_or_default();
    Value::Array(items)
}

fn link(item: &Value) -> Value {
    let mut out = Map::new();
    rename(item, "name", "label", &mut out);
    rename(item, "url", "href", &mut out);
    if let Some(icon) = item.get("icon") {
        out.insert("icon".to_owned(), icon.clone());
    }
    Value::Object(out)
}

fn cta(value: &Value) -> Value {
    let mut out = Map::new();
    if let Some(kind) = value.get("type") {
        out.insert("type".to_owned(), kind.clone());
    }
    rename(value, "name", "label", &mut out);
    rename(value, "url", "href", &mut out);
    Value::Object(out)
}

fn anchor(item: &Value) -> Value {
    let mut out = Map::new();
    rename(item, "name", "anchor", &mut out);
    rename(item, "url", "href", &mut out);
    if let Some(icon) = item.get("icon") {
        out.insert("icon".to_owned(), icon.clone());
    }
    Value::Object(out)
}

/// A v0 tab named one page; v1 tabs hold a list.
fn tab(item: &Value) -> Value {
    let mut out = Map::new();
    rename(item, "name", "tab", &mut out);
    if let Some(url) = item.get("url").and_then(Value::as_str) {
        out.insert("pages".to_owned(), json!([url]));
    }
    Value::Object(out)
}

fn specs(value: &Value) -> Value {
    let sources: Vec<&str> = match value {
        Value::String(one) => vec![one.as_str()],
        Value::Array(many) => many.iter().filter_map(Value::as_str).collect(),
        _ => Vec::new(),
    };
    let specs: Vec<Value> = sources
        .into_iter()
        .enumerate()
        .map(|(at, source)| {
            let id = if at == 0 {
                "api".to_owned()
            } else {
                format!("api-{}", at + 1)
            };
            json!({ "id": id, "source": source })
        })
        .collect();
    Value::Array(specs)
}

fn rename(from: &Value, old: &str, new: &str, into: &mut Map<String, Value>) {
    if let Some(value) = from.get(old) {
        into.insert(new.to_owned(), value.clone());
    }
}

/// The diagnostic a loader raises when it is handed a config that predates the
/// current schema (CFG-91).
pub fn upgrade_hint(from: u32) -> Diagnostic {
    Diagnostic::new(
        code::E0102,
        format!("this config is written for schema v{from}"),
    )
    .help("run `liyasa migrate-config` to upgrade it")
}
