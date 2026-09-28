//! API-21: a manual page compared with the spec that later describes it.
//!
//! `manual::drift` and `Drift::diagnostic` (W0513) were already here and tested.
//! What was missing was anything that turns a page's parsed components into the
//! `manual::Endpoint` drift takes — so the verifier had a comparison it could
//! not reach its input for. This crate declares what those props mean
//! (`manual::prop_schemas`'s doc says so), so reading them back belongs here
//! too.

use liyasa_core::components::ComponentInst;
use liyasa_core::document::{Origin, PropValue, Props};
use liyasa_core::ids::BlockId;
use liyasa_openapi::load;
use liyasa_openapi::manual::{self, Drift};

fn inst(name: &str, props: &[(&str, PropValue)]) -> ComponentInst {
    ComponentInst {
        name: name.to_owned(),
        props: Props(
            props
                .iter()
                .map(|(key, value)| ((*key).to_owned(), value.clone()))
                .collect(),
        ),
        children: Vec::new(),
        slots: Default::default(),
        id: BlockId([0; 12]),
        origin: Origin::default(),
    }
}

fn text(value: &str) -> PropValue {
    PropValue::Str(value.to_owned())
}

/// A page describing `GET /widgets/{id}` by hand, the way an author writes it.
fn page() -> Vec<ComponentInst> {
    vec![
        inst(
            "endpoint",
            &[("method", text("GET")), ("path", text("/widgets/{id}"))],
        ),
        inst(
            "param",
            &[
                ("name", text("id")),
                ("in", text("path")),
                ("type", text("string")),
                ("required", PropValue::Bool(true)),
            ],
        ),
        inst(
            "response-field",
            &[("name", text("size")), ("type", text("integer"))],
        ),
    ]
}

const SPEC: &str = r##"
openapi: 3.1.0
info: { title: Widgets, version: "1" }
paths:
  /widgets/{id}:
    get:
      operationId: getWidget
      parameters:
        - { name: id, in: path, required: true, schema: { type: string } }
      responses:
        "200":
          description: ok
          content:
            application/json:
              schema:
                type: object
                properties:
                  size: { type: integer }
"##;

fn spec(source: &str) -> liyasa_openapi::Spec {
    let loaded = load::from_bytes("api", "api.yaml", source.as_bytes()).expect("the spec loads");
    assert!(
        !loaded.diagnostics.has_errors(),
        "{:?}",
        loaded.diagnostics.as_slice()
    );
    loaded.spec
}

#[test]
fn a_pages_components_become_the_endpoint_it_describes() {
    let endpoints = manual::endpoints(&page());
    assert_eq!(endpoints.len(), 1, "one endpoint component, one endpoint");

    let endpoint = &endpoints[0];
    assert_eq!(endpoint.selector(), "GET /widgets/{id}");
    assert_eq!(
        endpoint
            .params
            .iter()
            .map(|f| f.name.as_str())
            .collect::<Vec<_>>(),
        vec!["id"],
        "a param row is a parameter"
    );
    assert!(endpoint.params[0].required);
    assert_eq!(endpoint.params[0].type_label.as_deref(), Some("string"));
    assert_eq!(
        endpoint
            .response_fields
            .iter()
            .map(|f| f.name.as_str())
            .collect::<Vec<_>>(),
        vec!["size"],
        "and a response-field row is a response field, not a parameter"
    );
}

#[test]
fn a_page_that_matches_its_spec_drifts_in_no_way() {
    let endpoints = manual::endpoints(&page());
    assert!(
        manual::drift(&endpoints[0], &spec(SPEC)).is_empty(),
        "the page and the spec agree, so promoting it changes nothing"
    );
}

#[test]
fn the_verifier_sees_the_drift_a_page_has_from_the_components_alone() {
    // The author wrote the parameter optional; the spec requires it.
    let mut components = page();
    components[1] = inst(
        "param",
        &[
            ("name", text("id")),
            ("in", text("path")),
            ("type", text("string")),
        ],
    );
    let endpoints = manual::endpoints(&components);
    let drift = manual::drift(&endpoints[0], &spec(SPEC));

    assert!(
        drift.iter().any(|d| matches!(d, Drift::Required { .. })),
        "{drift:?}"
    );
    let diagnostic = drift[0].diagnostic(&endpoints[0].selector());
    assert_eq!(diagnostic.code.as_str(), "W0513");
    assert!(
        diagnostic.message.contains("id"),
        "it names the field: {}",
        diagnostic.message
    );
}

#[test]
fn several_endpoints_on_one_page_keep_their_own_rows() {
    let mut components = page();
    components.push(inst(
        "endpoint",
        &[("method", text("DELETE")), ("path", text("/widgets/{id}"))],
    ));
    components.push(inst(
        "param",
        &[("name", text("force")), ("in", text("query"))],
    ));

    let endpoints = manual::endpoints(&components);
    assert_eq!(endpoints.len(), 2);
    assert_eq!(endpoints[0].selector(), "GET /widgets/{id}");
    assert_eq!(endpoints[1].selector(), "DELETE /widgets/{id}");
    assert_eq!(
        endpoints[1]
            .params
            .iter()
            .map(|f| f.name.as_str())
            .collect::<Vec<_>>(),
        vec!["force"],
        "a row belongs to the endpoint it follows, not to the first one"
    );
}

#[test]
fn a_row_before_any_endpoint_is_ignored_rather_than_guessed_at() {
    let orphan = vec![
        inst("param", &[("name", text("stray"))]),
        inst(
            "endpoint",
            &[("method", text("GET")), ("path", text("/widgets"))],
        ),
    ];
    let endpoints = manual::endpoints(&orphan);
    assert_eq!(endpoints.len(), 1);
    assert!(
        endpoints[0].params.is_empty(),
        "a row with no endpoint above it belongs to nothing"
    );
}

#[test]
fn the_alias_an_author_may_have_written_is_read_too() {
    let tagged = vec![
        inst(
            "Endpoint",
            &[("method", text("get")), ("path", text("/widgets"))],
        ),
        inst("ParamField", &[("name", text("q")), ("in", text("query"))]),
    ];
    let endpoints = manual::endpoints(&tagged);
    assert_eq!(endpoints.len(), 1, "the tag form is the same component");
    assert_eq!(endpoints[0].selector(), "GET /widgets");
    assert_eq!(endpoints[0].params.len(), 1);
}
