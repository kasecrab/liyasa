//! API-44: when the site's auth knows something about the reader, the
//! playground starts with it filled in.

use liyasa_openapi::config::Display;
use liyasa_openapi::playground::{Identity, Playground};

#[path = "support.rs"]
mod support;

const SPEC: &str = r##"
openapi: 3.1.0
info: { title: Widgets, version: "1" }
servers:
  - url: https://api.example.com
components:
  securitySchemes:
    accountKey: { type: apiKey, in: header, name: X-Account-Key }
    bearer: { type: http, scheme: bearer }
paths:
  /tenants/{tenant}/widgets:
    get:
      operationId: listWidgets
      security:
        - accountKey: []
        - bearer: []
      parameters:
        - { name: tenant, in: path, required: true, schema: { type: string } }
        - { name: limit, in: query, schema: { type: integer } }
      responses: { "200": { description: ok } }
"##;

fn playground() -> Playground {
    let spec = support::spec(SPEC);
    let operation = spec
        .by_operation_id("listWidgets")
        .expect("the spec has the operation");
    Playground::build(&spec, &operation, Display::Interactive, None)
}

#[test]
fn an_api_key_the_reader_already_has_is_filled_in() {
    let mut playground = playground();
    assert!(
        playground
            .auth
            .iter()
            .all(|control| control.prefilled.is_none()),
        "nothing is filled in before the reader is known"
    );
    assert!(!playground.personalized);

    let filled = playground.prefill(&Identity::new().with("accountKey", "live_abc123"));
    assert_eq!(filled, 1);

    let control = playground
        .auth
        .iter()
        .find(|control| control.scheme == "accountKey")
        .expect("the scheme is offered");
    assert_eq!(control.prefilled.as_deref(), Some("live_abc123"));
    assert!(
        playground
            .auth
            .iter()
            .find(|control| control.scheme == "bearer")
            .is_some_and(|control| control.prefilled.is_none()),
        "a scheme the reader has nothing for stays empty"
    );
    assert!(
        playground.personalized,
        "a page with a reader's credential in it may not be cached for everyone"
    );
}

#[test]
fn the_header_name_finds_the_key_when_the_scheme_id_does_not() {
    let mut playground = playground();
    assert_eq!(
        playground.prefill(&Identity::new().with("x-account-key", "live_def456")),
        1,
        "the site names its key by the header it travels in"
    );
    assert_eq!(playground.auth[0].prefilled.as_deref(), Some("live_def456"));
}

#[test]
fn a_bearer_token_is_found_under_the_name_the_rest_of_liyasa_uses_for_it() {
    let mut playground = playground();
    playground.prefill(&Identity::new().with("token", "eyJhbGci"));
    let bearer = playground
        .auth
        .iter()
        .find(|control| control.scheme == "bearer")
        .expect("the scheme is offered");
    assert_eq!(bearer.prefilled.as_deref(), Some("eyJhbGci"));
}

#[test]
fn basic_auth_takes_a_username_as_well_as_a_secret() {
    let spec = support::spec(
        r##"
openapi: 3.1.0
info: { title: Widgets, version: "1" }
components:
  securitySchemes:
    login: { type: http, scheme: basic }
paths:
  /widgets:
    get:
      operationId: listWidgets
      security: [{ login: [] }]
      responses: { "200": { description: ok } }
"##,
    );
    let operation = spec.by_operation_id("listWidgets").expect("it is there");
    let mut playground = Playground::build(&spec, &operation, Display::Interactive, None);
    playground.prefill(
        &Identity::new()
            .with("username", "ada")
            .with("password", "hunter2"),
    );
    assert_eq!(playground.auth[0].prefilled_user.as_deref(), Some("ada"));
    assert_eq!(playground.auth[0].prefilled.as_deref(), Some("hunter2"));
}

#[test]
fn a_parameter_the_reader_has_a_value_for_starts_on_it() {
    let mut playground = playground();
    let before = playground
        .fields
        .iter()
        .find(|field| field.name == "limit")
        .map(|field| field.value.clone());

    playground.prefill(&Identity::new().with("tenant", "acme-corp"));

    let tenant = playground
        .fields
        .iter()
        .find(|field| field.name == "tenant")
        .expect("the path parameter is a form field");
    assert_eq!(tenant.value, "acme-corp");
    assert!(
        tenant.from_reader,
        "the form says where the value came from"
    );

    let limit = playground
        .fields
        .iter()
        .find(|field| field.name == "limit")
        .expect("the query parameter is a form field");
    assert_eq!(
        Some(limit.value.clone()),
        before,
        "a field the reader has nothing for keeps its generated placeholder"
    );
    assert!(!limit.from_reader);
}

#[test]
fn an_empty_identity_changes_nothing() {
    let mut playground = playground();
    let before = format!("{:?}", playground.fields);
    assert_eq!(playground.prefill(&Identity::new()), 0);
    assert_eq!(format!("{:?}", playground.fields), before);
    assert!(!playground.personalized);
}
