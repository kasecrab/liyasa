//! `liyasa schema <name>` (CLI-13).

use liyasa_config::schema;

#[test]
fn every_named_schema_is_valid_json_with_its_published_id() {
    for named in schema::SCHEMAS {
        let parsed: serde_json::Value =
            serde_json::from_str(named.json).unwrap_or_else(|e| panic!("{}: {e}", named.name));
        assert_eq!(
            parsed["$id"],
            schema::schema_url(named),
            "{} publishes an $id outside the base the config schema sets",
            named.name
        );
    }
}

#[test]
fn config_and_frontmatter_are_both_addressable() {
    assert_eq!(
        schema::named("config").map(|s| s.json),
        Some(schema::CONFIG_SCHEMA)
    );
    assert!(schema::named("frontmatter").is_some());
    assert!(
        schema::named("components").is_none(),
        "the component schema arrives with the component registry"
    );
}

#[test]
fn the_emitted_config_schema_validates_the_example() {
    let example: serde_json::Value =
        serde_json::from_str(include_str!("../fixtures/example.json")).expect("valid JSON");
    let emitted: serde_json::Value =
        serde_json::from_str(schema::named("config").expect("config").json).expect("valid JSON");
    let validator = jsonschema::options()
        .with_draft(jsonschema::Draft::Draft202012)
        .build(&emitted)
        .expect("the emitted schema compiles");
    let errors: Vec<String> = validator
        .iter_errors(&example)
        .map(|e| e.to_string())
        .collect();
    assert_eq!(errors, Vec::<String>::new());
}
