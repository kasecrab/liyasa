//! ANA-02: the schema published at `/_liyasa/schema/event.json`.
//!
//! Two documents hold it honest, and neither is written by this crate: the
//! normative example from PRD §34.6, verbatim, and a row built the way
//! `liyasa-server`'s beacon handler builds one and serialised by
//! `liyasa_store::records::EventRecord`'s own `Serialize`.

use liyasa_analytics::schema;
use liyasa_store::records::EventRecord;
use serde_json::{Value, json};

fn compiled() -> jsonschema::Validator {
    jsonschema::validator_for(&schema::event()).expect("the published schema compiles")
}

fn errors(instance: &Value) -> Vec<String> {
    compiled()
        .iter_errors(instance)
        .map(|e| format!("{} at {}", e, e.instance_path()))
        .collect()
}

/// PRD §34.6, copied without edits. ANA-02 calls it the normative example.
const PRD_34_6: &str = r#"{
  "ts": "2026-09-14T10:22:31Z",
  "site": "acme-docs",
  "env": "production",
  "type": "page_view",
  "route": "/payments/create",
  "variant": { "version": "v2", "locale": "en", "region": "us" },
  "caller": { "kind": "agent", "agent_name": "claude-code" },
  "format": "markdown",
  "session_key": "k1:7f3a...e9",
  "referrer_host": null,
  "device": { "class": "server", "os_family": null, "browser_family": null },
  "country": "US",
  "duration_ms": 12
}"#;

#[test]
fn the_normative_example_validates() {
    let mut example: Value = serde_json::from_str(PRD_34_6).expect("§34.6 parses");
    // `k1:7f3a...e9` is an ellipsis in prose, not a key. Everything else is
    // taken as written.
    example["session_key"] = json!("k1:7f3a5c1d9e4b2a8f6c0d3e7b1a9f5c2e");
    assert_eq!(errors(&example), Vec::<String>::new());
}

#[test]
fn the_normative_examples_timestamp_converts_to_the_stored_form() {
    let example: Value = serde_json::from_str(PRD_34_6).expect("§34.6 parses");
    let ms = schema::timestamp_ms(&example["ts"]).expect("§34.6's ts converts");
    assert_eq!(ms, 1_789_381_351_000);
    // And the stored form round-trips as itself.
    assert_eq!(schema::timestamp_ms(&json!(ms)), Some(ms));
}

#[test]
fn a_row_the_server_writes_validates() {
    let record = EventRecord {
        ts: 1_789_381_351_000,
        site: "acme-docs".to_owned(),
        env: "production".to_owned(),
        route: "/payments/create?utm_source=changelog".to_owned(),
        kind: "page_view".to_owned(),
        variant: json!({ "version": "v2", "locale": "en", "region": "us", "product": null }),
        caller: json!({ "kind": "agent", "agent_name": "claudebot" }),
        format: "markdown".to_owned(),
        session_key: "k1:7f3a5c1d9e4b2a8f6c0d3e7b1a9f5c2e".to_owned(),
        referrer_host: Some("news.example.com".to_owned()),
        device: json!({ "class": "server", "os_family": null, "browser_family": null }),
        country: Some("US".to_owned()),
        duration_ms: Some(12),
        props: json!({ "depth": 75 }),
    };
    let instance = serde_json::to_value(&record).expect("a record serialises");
    assert_eq!(errors(&instance), Vec::<String>::new());
}

#[test]
fn a_beacon_that_sent_nothing_optional_still_validates() {
    // `EventRecord`'s optional fields default to `Value::Null`, and the beacon
    // handler passes a client's absent `variant` straight through, so a null
    // there is a row the writer really produces.
    let record = EventRecord {
        ts: 1,
        site: "s".to_owned(),
        env: "production".to_owned(),
        route: "/".to_owned(),
        kind: "copy_code".to_owned(),
        format: "html".to_owned(),
        ..EventRecord::default()
    };
    let instance = serde_json::to_value(&record).expect("a record serialises");
    assert_eq!(errors(&instance), Vec::<String>::new());
}

#[test]
fn the_schema_names_every_field_the_stored_row_has() {
    let record = EventRecord {
        format: "html".to_owned(),
        ..EventRecord::default()
    };
    let instance = serde_json::to_value(&record).expect("a record serialises");
    let published = schema::event();
    let properties = published["properties"].as_object().expect("properties");
    for name in instance.as_object().expect("an object").keys() {
        assert!(
            properties.contains_key(name),
            "the stored row has `{name}` and the published schema does not"
        );
    }
    // The schema closes the object, so a field it does not name would be
    // rejected outright rather than silently ignored.
    assert_eq!(published["additionalProperties"], json!(false));
}

#[test]
fn the_two_closed_sets_are_closed() {
    let mut instance: Value = serde_json::from_str(PRD_34_6).expect("§34.6 parses");
    instance["session_key"] = json!("");
    instance["format"] = json!("pdf");
    assert!(
        !errors(&instance).is_empty(),
        "ANA-02 closes format to html, markdown and json"
    );
    instance["format"] = json!("json");
    assert_eq!(errors(&instance), Vec::<String>::new());

    instance["caller"] = json!({ "kind": "robot" });
    assert!(
        !errors(&instance).is_empty(),
        "ANA-02 closes caller.kind to four values"
    );
}

#[test]
fn a_field_outside_the_schema_is_refused() {
    let mut instance: Value = serde_json::from_str(PRD_34_6).expect("§34.6 parses");
    instance["session_key"] = json!("");
    instance["ip"] = json!("203.0.113.7");
    let found = errors(&instance);
    assert!(
        !found.is_empty(),
        "an address must not pass the schema even by accident (ANA-03)"
    );
}

#[test]
fn a_row_with_no_type_or_no_site_is_refused() {
    for missing in ["type", "site", "env", "route", "ts"] {
        let mut instance: Value = serde_json::from_str(PRD_34_6).expect("§34.6 parses");
        instance["session_key"] = json!("");
        instance
            .as_object_mut()
            .expect("an object")
            .remove(missing)
            .expect("the example has it");
        assert!(
            !errors(&instance).is_empty(),
            "`{missing}` is required and its absence passed"
        );
    }
}

#[test]
fn rfc_3339_is_parsed_without_a_date_crate() {
    for (text, expected) in [
        ("1970-01-01T00:00:00Z", 0),
        ("2026-09-14T10:22:31Z", 1_789_381_351_000),
        ("2026-09-14T10:22:31.250Z", 1_789_381_351_250),
        // An offset is applied, not ignored.
        ("2026-09-14T12:22:31+02:00", 1_789_381_351_000),
        ("2026-09-14T08:22:31-02:00", 1_789_381_351_000),
        // A leap day, which a naive month table gets wrong.
        ("2024-02-29T00:00:00Z", 1_709_164_800_000),
        ("2000-02-29T00:00:00Z", 951_782_400_000),
        // 1900 is not a leap year; 2000 is. A century that got this wrong
        // would be off by a day here.
        ("1900-03-01T00:00:00Z", -2_203_891_200_000),
    ] {
        assert_eq!(schema::parse_rfc3339_ms(text), Some(expected), "for {text}");
    }
    for bad in [
        "",
        "2026-09-14",
        "not a time",
        "2026-13-01T00:00:00Z",
        "2026-09-14X10:22:31Z",
    ] {
        assert_eq!(schema::parse_rfc3339_ms(bad), None, "for {bad:?}");
    }
}

#[test]
fn utm_comes_back_out_of_the_route_ana_02_puts_it_on() {
    assert_eq!(
        schema::utm_from_route("/guides/start?utm_source=changelog&utm_medium=email&q=install"),
        Some(json!({ "source": "changelog", "medium": "email" }))
    );
    assert_eq!(
        schema::utm_from_route("/guides/start?utm_campaign=launch%20week"),
        Some(json!({ "campaign": "launch week" })),
        "a percent-encoded value is decoded, not stored encoded"
    );
    assert_eq!(schema::utm_from_route("/guides/start"), None);
    assert_eq!(schema::utm_from_route("/guides/start?q=install"), None);
}

#[test]
fn the_schema_is_served_where_ana_02_says() {
    assert_eq!(schema::PATH, "/_liyasa/schema/event.json");
}
