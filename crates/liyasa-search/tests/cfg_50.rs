//! CFG-50..CFG-54: the `search` settings, held to
//! `schemas/liyasa.schema.json`, which CFG-94 makes the source of truth.
//!
//! `liyasa-search` sits below `liyasa-config` in §34.7's dependency tree, so
//! the type is mirrored rather than imported; this test is what keeps the
//! mirror honest.

use liyasa_search::config::{Facet, SearchMode, SearchSettings};
use serde_json::Value;

fn schema() -> Value {
    let path = concat!(
        env!("CARGO_MANIFEST_DIR"),
        "/../../schemas/liyasa.schema.json"
    );
    let text = std::fs::read_to_string(path).expect("the config schema is in the repository");
    serde_json::from_str(&text).expect("the config schema is valid JSON")
}

fn search_object() -> Value {
    schema()["properties"]["search"].clone()
}

#[test]
fn every_schema_key_round_trips_through_the_settings() {
    let properties = search_object()["properties"]
        .as_object()
        .expect("`search` is an object")
        .clone();

    // A value for every key, so a key the type has no field for is dropped by
    // the round trip and the comparison below fails.
    let mut written = serde_json::Map::new();
    for (key, property) in &properties {
        written.insert(key.clone(), sample(key, property));
    }
    let settings: SearchSettings =
        serde_json::from_value(Value::Object(written.clone())).expect("the schema shape parses");
    let read_back = serde_json::to_value(&settings).expect("serializes");

    for key in properties.keys() {
        assert_eq!(
            read_back.get(key),
            written.get(key),
            "`search.{key}` does not survive a round trip through SearchSettings"
        );
    }
}

#[test]
fn the_defaults_are_the_ones_the_schema_documents() {
    let properties = search_object()["properties"].clone();
    let settings = SearchSettings::default();

    assert_eq!(
        properties["maxResults"]["default"].as_u64(),
        Some(settings.max_results as u64)
    );
    assert_eq!(
        properties["snippets"]["default"].as_bool(),
        Some(settings.snippets)
    );
}

#[test]
fn the_facets_are_the_ones_the_schema_enumerates() {
    let enumerated: Vec<String> = search_object()["properties"]["filters"]["items"]["enum"]
        .as_array()
        .expect("`filters` is an enum")
        .iter()
        .filter_map(|value| value.as_str().map(str::to_owned))
        .collect();
    let ours: Vec<String> = [Facet::Tab, Facet::Version, Facet::Locale, Facet::Type]
        .into_iter()
        .map(|facet| facet.as_str().to_owned())
        .collect();
    assert_eq!(enumerated, ours);
}

#[test]
fn the_modes_are_the_ones_the_schema_enumerates() {
    let enumerated: Vec<String> = search_object()["properties"]["mode"]["enum"]
        .as_array()
        .expect("`mode` is an enum")
        .iter()
        .filter_map(|value| value.as_str().map(str::to_owned))
        .collect();
    let ours: Vec<String> = [SearchMode::Keyword, SearchMode::Hybrid]
        .into_iter()
        .map(|mode| {
            serde_json::to_value(mode)
                .ok()
                .and_then(|v| v.as_str().map(str::to_owned))
                .unwrap_or_default()
        })
        .collect();
    assert_eq!(enumerated, ours);
}

#[test]
fn the_shard_size_keys_match() {
    let search = search_object();
    let keys: Vec<&str> = search["properties"]["shardSize"]["properties"]
        .as_object()
        .expect("`shardSize` is an object")
        .keys()
        .map(String::as_str)
        .collect();
    assert_eq!(keys, ["max", "min"], "the type carries both");
}

/// A value of the shape the schema declares for `key`.
fn sample(key: &str, property: &Value) -> Value {
    match key {
        "placeholder" => Value::String("Search the docs".to_owned()),
        "shortcut" => Value::String("mod+k".to_owned()),
        "boost" => serde_json::json!([{ "match": "/api/**", "factor": 2.0 }]),
        "exclude" => serde_json::json!(["/internal/**"]),
        "maxResults" => serde_json::json!(8),
        "snippets" => Value::Bool(false),
        "filters" => serde_json::json!(["tab", "version"]),
        "shardSize" => serde_json::json!({ "min": "100KB", "max": "1MB" }),
        "mode" => Value::String("hybrid".to_owned()),
        other => panic!(
            "`search.{other}` is in the schema but this test has no sample for it; \
             add a field to SearchSettings and a sample here. Schema says: {property}"
        ),
    }
}

/// CFG-50 calls the placeholder localizable, and the schema gives it one
/// string. `plan/rfcs/0704-localizable-placeholder.md` resolves that: the
/// per-locale theme file localizes it, the config key is the site-wide
/// default.
mod placeholder {
    use super::*;
    use liyasa_search::config::THEME_PLACEHOLDER_KEY;

    const THEME_DEFAULT: &str = "Search the documentation";

    fn settings(placeholder: Option<&str>) -> SearchSettings {
        SearchSettings {
            placeholder: placeholder.map(str::to_owned),
            ..SearchSettings::default()
        }
    }

    #[test]
    fn a_site_that_sets_nothing_gets_the_themes_words() {
        assert_eq!(
            settings(None).placeholder_resolved(None, THEME_DEFAULT),
            THEME_DEFAULT
        );
    }

    #[test]
    fn the_config_key_is_the_site_wide_default() {
        assert_eq!(
            settings(Some("Search the API")).placeholder_resolved(None, THEME_DEFAULT),
            "Search the API"
        );
    }

    #[test]
    fn a_locales_own_words_win_over_the_config_key() {
        assert_eq!(
            settings(Some("Search the API"))
                .placeholder_resolved(Some("Dokumentation durchsuchen"), THEME_DEFAULT),
            "Dokumentation durchsuchen",
            "the more specific of the two wins"
        );
    }

    #[test]
    fn a_locale_without_a_translation_falls_back_to_the_key() {
        // What `theme/strings.fr.json` looks like when it exists but leaves
        // this string alone.
        assert_eq!(
            settings(Some("Search the API")).placeholder_resolved(None, THEME_DEFAULT),
            "Search the API"
        );
    }

    #[test]
    fn an_empty_string_is_not_a_translation() {
        assert_eq!(
            settings(Some("Search the API")).placeholder_resolved(Some("  "), THEME_DEFAULT),
            "Search the API"
        );
        assert_eq!(
            settings(Some(" ")).placeholder_resolved(None, THEME_DEFAULT),
            THEME_DEFAULT
        );
    }

    #[test]
    fn the_theme_key_this_cascade_reads_is_the_one_the_theme_publishes() {
        assert_eq!(THEME_PLACEHOLDER_KEY, "searchPlaceholder");

        // The theme owns the string table; this crate may not depend on it, so
        // the check is on the file. A workspace without it still passes, which
        // is what a package built on its own branch needs.
        let theme = concat!(
            env!("CARGO_MANIFEST_DIR"),
            "/../liyasa-theme/src/strings.rs"
        );
        if let Ok(text) = std::fs::read_to_string(theme) {
            assert!(
                text.contains(&format!("{THEME_PLACEHOLDER_KEY:?}")),
                "the theme no longer publishes `{THEME_PLACEHOLDER_KEY}`"
            );
        }
    }
}
