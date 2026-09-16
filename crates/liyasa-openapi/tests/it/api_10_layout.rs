//! API-10: every section of an operation appears, with an anchor per
//! parameter.

use super::support;

use liyasa_openapi::markdown;
use liyasa_openapi::model::ParameterIn;

const SPEC: &str = r##"
openapi: 3.1.0
info: { title: Widgets, version: "1" }
servers:
  - url: https://api.example.com/v1
    description: Production
paths:
  /widgets/{id}:
    put:
      operationId: replaceWidget
      summary: Replace a widget
      description: Replaces the whole widget.
      deprecated: true
      tags: [Widgets]
      security:
        - bearer: [write]
      externalDocs: { url: https://example.com/guide, description: The guide }
      parameters:
        - { name: id, in: path, required: true, schema: { type: string }, description: Which widget }
        - { name: dryRun, in: query, schema: { type: boolean, default: false } }
        - { name: If-Match, in: header, required: true, schema: { type: string } }
        - { name: session, in: cookie, schema: { type: string } }
      requestBody:
        required: true
        description: The replacement
        content:
          application/json:
            schema:
              type: object
              required: [name]
              properties:
                name: { type: string, examples: ["Bolt"] }
                size: { type: integer, minimum: 1, maximum: 10 }
      responses:
        "200":
          description: Replaced
          headers:
            X-Rate-Limit: { schema: { type: integer }, description: Calls left }
          content:
            application/json:
              schema:
                type: object
                properties:
                  id: { type: string }
          links:
            widget: { operationId: getWidget, description: Read it back }
        "412": { description: Precondition failed }
      callbacks:
        onChange:
          "{$request.body#/callbackUrl}":
            post:
              summary: Widget changed
              responses: { "204": { description: ok } }
  /widgets/{id}/history:
    get:
      operationId: getWidget
      responses: { "200": { description: ok } }
components:
  securitySchemes:
    bearer: { type: http, scheme: bearer, description: A token }
"##;

#[test]
fn every_section_of_an_operation_is_on_the_page() {
    let spec = support::spec(SPEC);
    let page = support::page(&spec, "replaceWidget");

    assert_eq!(page.method.as_str(), "PUT");
    assert_eq!(page.path, "/widgets/{id}");
    assert_eq!(page.title, "Replace a widget");
    assert!(page.deprecated);
    assert_eq!(page.tags, vec!["Widgets".to_owned()]);
    assert_eq!(page.servers.len(), 1);
    assert_eq!(page.auth.len(), 1);
    assert_eq!(page.auth[0].scopes, vec!["write".to_owned()]);
    assert_eq!(
        page.external_docs.as_ref().map(|d| d.url.as_str()),
        Some("https://example.com/guide")
    );

    assert_eq!(
        page.parameters
            .iter()
            .map(|section| section.location)
            .collect::<Vec<_>>(),
        vec![
            ParameterIn::Path,
            ParameterIn::Query,
            ParameterIn::Header,
            ParameterIn::Cookie
        ]
    );

    let body = page.body.as_ref().expect("there is a request body");
    assert!(body.required);
    assert_eq!(body.media_types[0].media_type, "application/json");
    assert_eq!(
        body.media_types[0]
            .fields
            .iter()
            .map(|f| f.name.as_str())
            .collect::<Vec<_>>(),
        vec!["name", "size"]
    );

    assert_eq!(
        page.responses
            .iter()
            .map(|r| r.status.as_str())
            .collect::<Vec<_>>(),
        vec!["200", "412"]
    );
    assert_eq!(page.responses[0].headers[0].name, "X-Rate-Limit");
    assert_eq!(page.responses[0].links[0].name, "widget");

    assert_eq!(page.callbacks.len(), 1);
    assert_eq!(page.callbacks[0].expression, "{$request.body#/callbackUrl}");
}

#[test]
fn every_parameter_has_an_anchor_and_no_two_collide() {
    let spec = support::spec(SPEC);
    let page = support::page(&spec, "replaceWidget");
    let anchors: Vec<&str> = page
        .parameters
        .iter()
        .flat_map(|section| section.fields.iter())
        .map(|field| field.anchor.as_str())
        .collect();
    assert_eq!(
        anchors,
        vec![
            "path-id",
            "query-dryrun",
            "header-if-match",
            "cookie-session"
        ]
    );
    for (index, anchor) in anchors.iter().enumerate() {
        assert!(!anchor.is_empty());
        assert!(
            !anchors[..index].contains(anchor),
            "`{anchor}` is used twice"
        );
    }
}

#[test]
fn the_rendered_page_matches_its_golden() {
    let spec = support::spec(SPEC);
    let page = support::page(&spec, "replaceWidget");
    support::golden(
        "api_10",
        &markdown::render(&page, &markdown::Options::default()),
    );
}
