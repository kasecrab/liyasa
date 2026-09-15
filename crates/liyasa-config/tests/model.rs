//! The generated `SiteConfig` against the PRD §34.2 example (CFG-94).

use liyasa_config::SiteConfig;
use liyasa_config::model::{Navigation, NavigationNode};

/// PRD §34.2, with the two keys RFC 0005 records as schema drift written the
/// way the schema accepts them.
const EXAMPLE: &str = include_str!("fixtures/example.json");

fn example() -> SiteConfig {
    serde_json::from_str(EXAMPLE).expect("the §34.2 example deserializes")
}

#[test]
fn the_example_config_deserializes() {
    let config = example();
    assert_eq!(config.name, "Acme Docs");
    assert_eq!(config.versions.len(), 2);
    assert_eq!(config.locales.len(), 2);
}

#[test]
fn the_example_config_round_trips() {
    let config = example();
    let text = serde_json::to_string(&config).expect("a config serializes");
    let again: SiteConfig = serde_json::from_str(&text).expect("its own output deserializes");
    assert_eq!(config, again);
}

#[test]
fn navigation_keeps_every_node_form() {
    let Navigation::List(tabs) = example().navigation.expect("the example has navigation") else {
        panic!("the example's navigation is an array of nodes");
    };
    assert_eq!(tabs.len(), 3);
    let NavigationNode::Tab { tab, pages, .. } = &tabs[0] else {
        panic!("the first node is a tab");
    };
    assert_eq!(tab, "Guides");
    assert!(matches!(pages[0], NavigationNode::Group { .. }));
    assert!(matches!(pages[1], NavigationNode::Directory { .. }));
    assert!(matches!(tabs[2], NavigationNode::Tab { .. }));
}

#[test]
fn an_unknown_key_is_refused_by_the_type() {
    let error = serde_json::from_str::<SiteConfig>(r#"{"name":"x","nope":1}"#)
        .expect_err("`nope` is not a config key");
    assert!(error.to_string().contains("nope"), "{error}");
}
