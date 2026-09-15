//! Assembling the template context (CM-12, CM-72).
//!
//! CM-12 fixes the order and says later wins. Only two layers share the root
//! namespace — the site's `variables` and `snippets/vars.*` — plus the
//! dimension values; everything else arrives under its own name, so "later
//! wins" is a deep merge of two maps and then a set of namespaced keys.
//!
//! Per-dimension overrides (`variables.by_version["2.0"]`) are applied after
//! the base, in the dimension order CM-12 lists, so a locale override wins over
//! a version override for the same key.

use std::collections::BTreeMap;

use liyasa_core::diagnostics::Diagnostics;
use liyasa_core::markdown::TemplateContext;
use serde_json::{Map, Value};

use super::escape;

/// The dimensions that select an override, in the order they are applied.
pub const DIMENSIONS: &[&str] = &["version", "locale", "product", "region"];

/// One page's layers, in CM-12's order.
#[derive(Debug, Default, Clone)]
pub struct Layers {
    /// `variables` from `liyasa.json`.
    pub site_variables: Value,
    /// `snippets/vars.{json,yaml,toml}`.
    pub vars_file: Value,
    /// `facts/` exposed as `facts.*`.
    pub facts: Value,
    /// The dimension values in play, such as `version` → `2.0`.
    pub dimensions: BTreeMap<String, String>,
    /// The page's front matter, exposed as `page.*`.
    pub page: Value,
    pub site: Value,
    pub nav: Value,
    /// The allow-listed environment variables from `build.env`.
    pub env: Value,
    /// Only populated for a page that declares `personalized: true` (CM-12).
    pub reader: Option<Value>,
}

impl Layers {
    /// The merged context, ready for [`expand`](super::expand).
    pub fn build(&self) -> TemplateContext {
        TemplateContext {
            values: minijinja::Value::from_serialize(self.values()),
            tracking: true,
        }
    }

    pub fn values(&self) -> Value {
        let mut root = Map::new();
        merge(&mut root, &self.site_variables);
        merge(&mut root, &self.vars_file);
        for dimension in DIMENSIONS {
            let Some(chosen) = self.dimensions.get(*dimension) else {
                continue;
            };
            let key = format!("by_{dimension}");
            for layer in [&self.site_variables, &self.vars_file] {
                if let Some(overrides) = layer.get(&key).and_then(|by| by.get(chosen)) {
                    merge(&mut root, overrides);
                }
            }
        }
        // The `by_*` tables are configuration, not variables.
        root.retain(|key, _| !key.starts_with("by_"));

        for (dimension, chosen) in &self.dimensions {
            root.insert(dimension.clone(), Value::String(chosen.clone()));
        }
        for (name, layer) in [
            ("facts", &self.facts),
            ("page", &self.page),
            ("site", &self.site),
            ("nav", &self.nav),
            ("env", &self.env),
        ] {
            if !layer.is_null() {
                root.insert(name.to_owned(), layer.clone());
            }
        }
        if let Some(reader) = &self.reader {
            root.insert("reader".to_owned(), reader.clone());
        }
        Value::Object(root)
    }
}

/// Escapes every string in a value that entered the build below `operator`
/// trust (CM-20).
///
/// This is the call a build makes once, where the value *enters* the context:
/// a fact from a `url`, `command`, or `screenshot` source, a `reader.*` field,
/// anything an automation supplied. Escaping here rather than at each
/// interpolation is what lets a multi-line value be refused with `E0320`
/// instead of being rendered into a document that no longer parses.
///
/// Block openers are escaped unconditionally, because where a value lands is
/// not known until it is interpolated and the safe assumption is line start.
pub fn escape_untrusted(value: &Value) -> Result<Value, Diagnostics> {
    let mut diagnostics = Diagnostics::new();
    let escaped = walk(value, "", &mut diagnostics);
    if diagnostics.has_errors() {
        return Err(diagnostics);
    }
    Ok(escaped)
}

fn walk(value: &Value, path: &str, diagnostics: &mut Diagnostics) -> Value {
    match value {
        Value::String(text) => match escape::escape_untrusted_markdown(text, true) {
            Ok(escaped) => Value::String(escaped),
            Err(error) => {
                let at = if path.is_empty() { "the value" } else { path };
                let mut reported = *error;
                reported.message = format!("{at}: {}", reported.message);
                diagnostics.push(reported);
                Value::Null
            }
        },
        Value::Array(items) => Value::Array(
            items
                .iter()
                .enumerate()
                .map(|(at, item)| walk(item, &format!("{path}[{at}]"), diagnostics))
                .collect(),
        ),
        Value::Object(fields) => Value::Object(
            fields
                .iter()
                .map(|(key, field)| {
                    let nested = if path.is_empty() {
                        key.clone()
                    } else {
                        format!("{path}.{key}")
                    };
                    (key.clone(), walk(field, &nested, diagnostics))
                })
                .collect(),
        ),
        other => other.clone(),
    }
}

/// Deep-merges `layer` into `root`: a map merges key by key, anything else
/// replaces.
fn merge(root: &mut Map<String, Value>, layer: &Value) {
    let Value::Object(layer) = layer else { return };
    for (key, value) in layer {
        match (root.get_mut(key), value) {
            (Some(Value::Object(existing)), Value::Object(_)) => {
                let mut nested = std::mem::take(existing);
                merge(&mut nested, value);
                *existing = nested;
            }
            _ => {
                root.insert(key.clone(), value.clone());
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use serde_json::json;

    use super::*;

    fn layers() -> Layers {
        Layers {
            site_variables: json!({ "product": "Acme", "limit": 100 }),
            vars_file: json!({ "limit": 200, "support": "support@acme.test" }),
            page: json!({ "title": "Rate limits" }),
            site: json!({ "name": "Acme Docs" }),
            ..Layers::default()
        }
    }

    #[test]
    fn a_later_layer_wins() {
        let values = layers().values();
        assert_eq!(values["limit"], 200);
        assert_eq!(values["product"], "Acme");
        assert_eq!(values["support"], "support@acme.test");
    }

    #[test]
    fn namespaced_layers_keep_their_names() {
        let values = layers().values();
        assert_eq!(values["page"]["title"], "Rate limits");
        assert_eq!(values["site"]["name"], "Acme Docs");
    }

    #[test]
    fn front_matter_does_not_leak_into_the_root() {
        let values = layers().values();
        assert!(values.get("title").is_none());
    }

    #[test]
    fn a_dimension_override_wins_over_the_base() {
        let mut layers = layers();
        layers.site_variables = json!({
            "limit": 100,
            "by_version": { "2.0": { "limit": 1000 } },
        });
        layers.vars_file = Value::Null;
        layers.dimensions = BTreeMap::from([("version".to_owned(), "2.0".to_owned())]);
        let values = layers.values();
        assert_eq!(values["limit"], 1000);
        assert_eq!(values["version"], "2.0");
    }

    #[test]
    fn an_override_for_another_value_does_not_apply() {
        let mut layers = Layers {
            site_variables: json!({
                "limit": 100,
                "by_version": { "2.0": { "limit": 1000 } },
            }),
            ..Layers::default()
        };
        layers.dimensions = BTreeMap::from([("version".to_owned(), "1.0".to_owned())]);
        assert_eq!(layers.values()["limit"], 100);
    }

    #[test]
    fn a_later_dimension_wins_over_an_earlier_one() {
        let layers = Layers {
            site_variables: json!({
                "greeting": "hello",
                "by_version": { "2.0": { "greeting": "hello v2" } },
                "by_locale": { "de": { "greeting": "hallo" } },
            }),
            dimensions: BTreeMap::from([
                ("version".to_owned(), "2.0".to_owned()),
                ("locale".to_owned(), "de".to_owned()),
            ]),
            ..Layers::default()
        };
        assert_eq!(layers.values()["greeting"], "hallo");
    }

    #[test]
    fn the_override_tables_are_not_variables() {
        let layers = Layers {
            site_variables: json!({ "by_version": { "2.0": { "x": 1 } } }),
            ..Layers::default()
        };
        assert!(layers.values().get("by_version").is_none());
    }

    #[test]
    fn nested_maps_merge_key_by_key() {
        let layers = Layers {
            site_variables: json!({ "plan": { "name": "Pro", "price": 99 } }),
            vars_file: json!({ "plan": { "price": 120 } }),
            ..Layers::default()
        };
        let values = layers.values();
        assert_eq!(values["plan"]["name"], "Pro");
        assert_eq!(values["plan"]["price"], 120);
    }

    #[test]
    fn reader_is_absent_unless_the_page_is_personalized() {
        assert!(layers().values().get("reader").is_none());
        let personalized = Layers {
            reader: Some(json!({ "name": "Ada" })),
            ..layers()
        };
        assert_eq!(personalized.values()["reader"]["name"], "Ada");
    }

    #[test]
    fn the_built_context_tracks_reads() {
        assert!(layers().build().tracking);
    }

    // ---- CM-20 ----

    #[test]
    fn untrusted_strings_are_escaped_wherever_they_sit() {
        let value = json!({
            "plan": { "name": "# Pro" },
            "tags": ["- one", "`two`"],
            "count": 3,
        });
        let escaped = escape_untrusted(&value).expect("single-line values");
        assert_eq!(escaped["plan"]["name"], "\\# Pro");
        assert_eq!(escaped["tags"][0], "\\- one");
        assert_eq!(escaped["tags"][1], "\\`two\\`");
        assert_eq!(escaped["count"], 3);
    }

    #[test]
    fn a_multi_line_untrusted_value_is_refused_with_its_path() {
        let value = json!({ "plan": { "notes": "line one\nline two" } });
        let diagnostics = escape_untrusted(&value).expect_err("a line break");
        let reported = diagnostics.iter().next().expect("a diagnostic");
        assert_eq!(reported.code.as_str(), "E0320");
        assert!(
            reported.message.starts_with("plan.notes:"),
            "{}",
            reported.message
        );
    }

    #[test]
    fn every_offending_value_is_listed() {
        let value = json!({ "a": "x\ny", "b": "p\rq", "c": "fine" });
        let diagnostics = escape_untrusted(&value).expect_err("line breaks");
        assert_eq!(diagnostics.len(), 2);
    }
}
