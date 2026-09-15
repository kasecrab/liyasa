//! API-04: a Markdown page with `openapi:` front matter augments the generated
//! page — its body above the parameters, its slots at the injection points.

#[path = "support.rs"]
mod support;

use liyasa_openapi::markdown;
use liyasa_openapi::model::OrderedMap;
use liyasa_openapi::page::{self, Augmentation, Rendered};

const SPEC: &str = r##"
openapi: 3.1.0
info: { title: Users, version: "1" }
paths:
  /users/{id}:
    get:
      operationId: getUser
      summary: Fetch one user
      parameters:
        - { name: id, in: path, required: true, schema: { type: string } }
      responses:
        "200": { description: The user }
"##;

fn augmented() -> String {
    let spec = support::spec(SPEC);
    let mut page = support::page(&spec, "getUser");
    let mut slots = OrderedMap::new();
    slots.insert(
        "after-params",
        Rendered {
            html: "<p>Rate limits apply.</p>".to_owned(),
            markdown: "Rate limits apply.".to_owned(),
        },
    );
    page.augment(Augmentation {
        intro: Some(Rendered {
            html: "<p>Read the guide first.</p>".to_owned(),
            markdown: "Read the guide first.".to_owned(),
        }),
        slots,
    });
    markdown::render(&page, &markdown::Options::default())
}

#[test]
fn front_matter_names_the_spec_and_the_operation() {
    let (id, selector) =
        page::parse_front_matter("api GET /users/{id}").expect("the front matter parses");
    assert_eq!(id, "api");
    assert_eq!(selector, "GET /users/{id}");

    let spec = support::spec(SPEC);
    assert!(
        spec.operations().any(|op| op.selector() == selector),
        "the selector names an operation the spec has"
    );
}

#[test]
fn the_body_renders_above_the_generated_parameters() {
    let out = augmented();
    let intro = out
        .find("Read the guide first.")
        .expect("the body is on the page");
    let parameters = out
        .find("## Path parameters")
        .expect("the parameters are there");
    assert!(intro < parameters, "the body comes first:\n{out}");
}

#[test]
fn a_slot_renders_at_its_injection_point() {
    let out = augmented();
    let parameters = out
        .find("## Path parameters")
        .expect("the parameters are there");
    let slot = out
        .find("Rate limits apply.")
        .expect("the slot is on the page");
    let responses = out.find("## Responses").expect("the responses are there");
    assert!(
        parameters < slot && slot < responses,
        "after the parameters:\n{out}"
    );
}

#[test]
fn only_the_documented_slot_names_are_accepted() {
    for name in [
        "before-request",
        "after-params",
        "before-responses",
        "rail-top",
    ] {
        assert!(Augmentation::is_slot(name), "`{name}` is documented");
    }
    assert!(!Augmentation::is_slot("after-everything"));
}

#[test]
fn the_augmented_page_matches_its_golden() {
    support::golden("api_04", &augmented());
}
