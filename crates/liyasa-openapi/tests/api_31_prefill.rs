//! API-31: samples are prefilled from the spec's examples, come in a
//! required-only and an all-parameters variant, carry placeholder credentials,
//! and are replaced entirely by `x-codeSamples` when the spec ships its own.

use liyasa_openapi::codegen::Registry;
use liyasa_openapi::sample::{self, Options, OptionsFill, Request};

#[path = "support.rs"]
mod support;

const SPEC: &str = r##"
openapi: 3.1.0
info: { title: Widgets, version: "1" }
servers:
  - url: https://api.example.com
security:
  - bearer: []
components:
  securitySchemes:
    bearer: { type: http, scheme: bearer }
paths:
  /widgets:
    post:
      operationId: createWidget
      parameters:
        - name: tenant
          in: query
          required: true
          schema: { type: string }
          example: acme
        - name: verbose
          in: query
          required: false
          schema: { type: boolean }
          example: true
      requestBody:
        content:
          application/json:
            schema:
              type: object
              required: [name]
              properties:
                name: { type: string }
                size: { type: integer }
            example: { name: "widget from the spec", size: 7 }
      responses: { "201": { description: made } }
  /widgets/{id}:
    get:
      operationId: getWidget
      x-codeSamples:
        - lang: curl
          label: Fetch one
          source: "curl https://api.example.com/widgets/42"
        - lang: ruby
          source: "Net::HTTP.get(URI('https://api.example.com/widgets/42'))"
      parameters:
        - { name: id, in: path, required: true, schema: { type: string } }
      responses: { "200": { description: ok } }
"##;

fn languages() -> Vec<String> {
    vec!["curl".to_owned(), "python".to_owned()]
}

fn samples(operation_id: &str, fill: OptionsFill) -> Vec<liyasa_openapi::codegen::Sample> {
    let spec = support::spec(SPEC);
    let operation = spec
        .by_operation_id(operation_id)
        .unwrap_or_else(|| panic!("the spec has `{operation_id}`"));
    Registry::new().samples(
        &spec,
        &operation,
        &languages(),
        &Options {
            fill,
            ..Options::default()
        },
    )
}

#[test]
fn the_sample_is_prefilled_from_what_the_spec_wrote() {
    for sample in samples("createWidget", OptionsFill::All) {
        assert!(
            sample.source.contains("widget from the spec"),
            "{} sends the spec's body example:\n{}",
            sample.language,
            sample.source
        );
        assert!(
            sample.source.contains("acme"),
            "{} sends the parameter's example:\n{}",
            sample.language,
            sample.source
        );
        assert!(!sample.from_spec, "Liyasa generated this one");
    }
}

#[test]
fn required_only_and_all_parameters_are_two_variants_of_one_operation() {
    let spec = support::spec(SPEC);
    let operation = spec.by_operation_id("createWidget").expect("it is there");

    let all = Request::build(
        &spec,
        &operation,
        &Options {
            fill: OptionsFill::All,
            ..Options::default()
        },
    );
    let required = Request::build(
        &spec,
        &operation,
        &Options {
            fill: OptionsFill::Required,
            ..Options::default()
        },
    );

    let names = |request: &Request| -> Vec<String> {
        request
            .query
            .iter()
            .map(|pair| pair.name.clone())
            .collect::<Vec<_>>()
    };
    assert_eq!(names(&all), vec!["tenant", "verbose"]);
    assert_eq!(
        names(&required),
        vec!["tenant"],
        "the optional parameter is what the toggle drops"
    );

    let body = required.body.as_ref().expect("the body is required");
    assert!(
        body.text().contains("widget from the spec"),
        "a required-only sample still sends the body the spec gave an example for"
    );
}

#[test]
fn credentials_are_obvious_placeholders() {
    let request = Request::build(
        &support::spec(SPEC),
        &support::spec(SPEC)
            .by_operation_id("createWidget")
            .expect("it is there"),
        &Options::default(),
    );
    let authorization = request
        .headers
        .iter()
        .find(|pair| pair.name.eq_ignore_ascii_case("authorization"))
        .expect("the operation is secured");
    assert_eq!(authorization.value, format!("Bearer {}", sample::TOKEN));
}

#[test]
fn x_code_samples_replaces_the_generated_set_rather_than_joining_it() {
    let samples = samples("getWidget", OptionsFill::All);
    assert_eq!(
        samples
            .iter()
            .map(|sample| sample.language.as_str())
            .collect::<Vec<_>>(),
        vec!["curl", "ruby"],
        "the spec's list is the whole list, in the spec's order, even though \
         ruby is not a configured language and python is"
    );
    assert!(samples.iter().all(|sample| sample.from_spec));
    assert_eq!(samples[0].label, "Fetch one");
    assert_eq!(samples[0].source, "curl https://api.example.com/widgets/42");
    assert!(
        !samples
            .iter()
            .any(|sample| sample.source.contains("$ACCESS_TOKEN")),
        "nothing was generated alongside them"
    );
}
