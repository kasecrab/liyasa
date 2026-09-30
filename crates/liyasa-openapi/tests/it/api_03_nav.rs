//! API-03: a node generates one page per operation, grouped and ordered as
//! configured, and an operation can also be placed on its own.

use liyasa_openapi::config::{GroupBy, SpecConfig};
use liyasa_openapi::load;
use liyasa_openapi::nav::{self, GroupConfig, Node};

const SPEC: &str = r##"
openapi: 3.1.0
info: { title: Widgets, version: "1" }
tags:
  - { name: Widgets }
  - { name: Admin }
paths:
  /widgets:
    get:
      operationId: listWidgets
      summary: List widgets
      tags: [Widgets]
      responses: { "200": { description: ok } }
  /widgets/{id}:
    get:
      operationId: getWidget
      summary: Fetch one widget
      tags: [Widgets]
      responses: { "200": { description: ok } }
  /admin/flush:
    post:
      operationId: flush
      tags: [Admin]
      responses: { "204": { description: done } }
"##;

fn fixture() -> (liyasa_openapi::Spec, SpecConfig) {
    let loaded = load::from_bytes("api", "api.yaml", SPEC.as_bytes()).expect("the spec loads");
    assert!(
        !loaded.diagnostics.has_errors(),
        "{:?}",
        loaded.diagnostics.as_slice()
    );
    let config =
        SpecConfig::parse(&serde_json::json!({ "id": "api", "source": "openapi/api.yaml" }))
            .expect("the config reads");
    (loaded.spec, config)
}

#[test]
fn a_node_generates_a_page_per_operation_grouped_by_tag() {
    let (spec, config) = fixture();
    let (reference, diagnostics) = nav::generate(
        &spec,
        &config,
        &Node {
            openapi: "api".to_owned(),
            group_by: Some(GroupBy::Tag),
            ..Node::default()
        },
    );
    assert!(!diagnostics.has_errors(), "{:?}", diagnostics.as_slice());
    assert_eq!(reference.entries().count(), 3);
    assert_eq!(
        reference
            .groups
            .iter()
            .map(|g| g.title.as_str())
            .collect::<Vec<_>>(),
        vec!["Widgets", "Admin"]
    );
    assert_eq!(
        reference.groups[0]
            .pages
            .iter()
            .map(|p| p.route.as_str())
            .collect::<Vec<_>>(),
        vec!["/api-reference/listwidgets", "/api-reference/getwidget"]
    );
}

#[test]
fn configured_group_order_and_display_names_are_what_the_page_shows() {
    let (spec, config) = fixture();
    let (reference, _) = nav::generate(
        &spec,
        &config,
        &Node {
            openapi: "api".to_owned(),
            groups: vec![
                GroupConfig {
                    name: "Admin".to_owned(),
                    title: Some("Operations".to_owned()),
                    order: Some(1),
                    collapsed: Some(true),
                },
                GroupConfig {
                    name: "Widgets".to_owned(),
                    order: Some(2),
                    ..GroupConfig::default()
                },
            ],
            ..Node::default()
        },
    );
    assert_eq!(
        reference
            .groups
            .iter()
            .map(|g| g.title.as_str())
            .collect::<Vec<_>>(),
        vec!["Operations", "Widgets"]
    );
    assert!(reference.groups[0].collapsed);
}

#[test]
fn one_operation_can_be_placed_anywhere_by_selector() {
    let (spec, config) = fixture();
    let (id, selector) = nav::parse_selector("api:GET /widgets/{id}").expect("the selector parses");
    assert_eq!(id, "api");

    let entry = nav::place(&spec, &config, selector, nav::DEFAULT_BASE).expect("it resolves");
    assert_eq!(entry.route, "/api-reference/getwidget");
    assert_eq!(entry.title, "Fetch one widget");
}

/// API-05 lists `collapsed` among the `x-liyasa` keys, and API-51 says `hidden`
/// applies to tags as well as operations. Both were read into `XLiyasa` and
/// consumed nowhere: the declared-tags loop took the tag's `title` hint and then
/// hardcoded `collapsed: false`, and a hidden tag still produced a group.
const TAGGED: &str = r##"
openapi: 3.1.0
info: { title: Widgets, version: "1" }
tags:
  - name: Widgets
    description: The spec's own prose.
    x-liyasa: { collapsed: true, description: "The operator's prose." }
  - name: Internal
    x-liyasa: { hidden: true }
  - { name: Admin }
paths:
  /widgets:
    get:
      operationId: listWidgets
      tags: [Widgets]
      responses: { "200": { description: ok } }
  /internal/flush:
    post:
      operationId: flush
      tags: [Internal]
      responses: { "204": { description: done } }
  /admin/ping:
    get:
      operationId: ping
      tags: [Admin]
      responses: { "200": { description: ok } }
"##;

fn tagged() -> (liyasa_openapi::Spec, SpecConfig) {
    let loaded = load::from_bytes("api", "api.yaml", TAGGED.as_bytes()).expect("the spec loads");
    assert!(
        !loaded.diagnostics.has_errors(),
        "{:?}",
        loaded.diagnostics.as_slice()
    );
    let config =
        SpecConfig::parse(&serde_json::json!({ "id": "api", "source": "openapi/api.yaml" }))
            .expect("the config reads");
    (loaded.spec, config)
}

#[test]
fn a_tag_collapses_its_own_group() {
    let (spec, config) = tagged();
    let (reference, _) = nav::generate(
        &spec,
        &config,
        &Node {
            openapi: "api".to_owned(),
            ..Node::default()
        },
    );
    let widgets = reference
        .groups
        .iter()
        .find(|group| group.name == "Widgets")
        .expect("the tag is a group");
    assert!(
        widgets.collapsed,
        "`x-liyasa.collapsed` on the tag collapses the group it becomes"
    );
    assert_eq!(
        widgets.description.as_deref(),
        Some("The operator's prose."),
        "and `x-liyasa.description` replaces the spec's own"
    );
}

#[test]
fn a_group_with_no_hint_is_left_alone() {
    let (spec, config) = tagged();
    let (reference, _) = nav::generate(
        &spec,
        &config,
        &Node {
            openapi: "api".to_owned(),
            ..Node::default()
        },
    );
    let admin = reference
        .groups
        .iter()
        .find(|group| group.name == "Admin")
        .expect("the tag is a group");
    assert!(!admin.collapsed);
    assert_eq!(admin.description, None);
}

#[test]
fn a_hidden_tag_is_not_a_group_at_all() {
    let (spec, config) = tagged();
    let (reference, _) = nav::generate(
        &spec,
        &config,
        &Node {
            openapi: "api".to_owned(),
            ..Node::default()
        },
    );
    assert!(
        !reference
            .groups
            .iter()
            .any(|group| group.name == "Internal"),
        "`x-liyasa.hidden` on a tag removes it from navigation (API-51): {:?}",
        reference
            .groups
            .iter()
            .map(|group| group.name.as_str())
            .collect::<Vec<_>>()
    );
    assert!(
        !reference
            .entries()
            .any(|entry| entry.selector.contains("flush")),
        "and its operations go with it rather than becoming an untagged group"
    );
}
