//! SRC-01: one document per page section, with the fields §12.1 lists.

mod support;

use liyasa_core::document::{PropValue, Props};
use liyasa_core::ids::{Locale, Route, Version};
use liyasa_search::doc::{DocKind, EndpointFacts, PageMeta, SectionDocument};
use liyasa_search::section;
use support::{component, document, fence, heading, para};

fn page() -> PageMeta {
    PageMeta {
        route: Route::new("/guides/auth"),
        title: "Authentication".to_owned(),
        breadcrumb: vec!["Guides".to_owned()],
        tab: Some("docs".to_owned()),
        version: Some(Version::new("v2")),
        locale: Locale::new("en"),
        kind: DocKind::Page,
        keywords: vec!["auth".to_owned()],
        groups: vec!["beta".to_owned()],
        regions: vec!["eu".to_owned()],
        boost: 1.0,
        updated: Some(1_700_000_000_000),
        endpoint: None,
    }
}

fn guide() -> Vec<SectionDocument> {
    let ast = document(vec![
        para("Sign requests with an API key."),
        heading(2, "api-keys", "API keys"),
        para("Create a key in the dashboard."),
        fence(
            "bash",
            "curl -H 'Authorization: Bearer $KEY' https://api.example.com",
        ),
        heading(3, "rotating", "Rotating a key"),
        para("Rotate keys every ninety days."),
        heading(2, "oauth", "OAuth"),
        para("Use the authorization code flow."),
    ]);
    section::extract(&ast, &page())
}

#[test]
fn every_h2_and_h3_becomes_a_document() {
    let docs = guide();
    let anchors: Vec<&str> = docs.iter().map(|d| d.anchor.as_str()).collect();
    assert_eq!(anchors, ["", "api-keys", "rotating", "oauth"]);
}

#[test]
fn the_lead_paragraph_is_the_pages_own_document() {
    let docs = guide();
    let lead = &docs[0];
    assert_eq!(lead.anchor, "");
    assert_eq!(lead.section, "Authentication");
    assert_eq!(lead.body, "Sign requests with an API key.");
}

#[test]
fn a_section_holds_only_its_own_prose() {
    let docs = guide();
    let keys = &docs[1];
    assert_eq!(keys.section, "API keys");
    assert_eq!(keys.body, "Create a key in the dashboard.");
    assert!(
        !keys.body.contains("Rotate keys"),
        "the H3 below it is its own document"
    );
}

#[test]
fn code_is_a_field_of_its_own() {
    let docs = guide();
    let keys = &docs[1];
    assert!(keys.code.contains("Authorization"), "{}", keys.code);
    assert!(
        !keys.body.contains("Authorization"),
        "a fence is code, not prose"
    );
}

#[test]
fn a_subsection_breadcrumbs_through_its_parent() {
    let docs = guide();
    let rotating = &docs[2];
    assert_eq!(rotating.breadcrumb, ["Guides", "API keys"]);
    assert_eq!(docs[1].breadcrumb, ["Guides"]);
}

#[test]
fn every_document_carries_the_pages_facets() {
    for doc in guide() {
        assert_eq!(doc.route.as_str(), "/guides/auth");
        assert_eq!(doc.title, "Authentication");
        assert_eq!(doc.tab.as_deref(), Some("docs"));
        assert_eq!(doc.version.as_ref().map(Version::as_str), Some("v2"));
        assert_eq!(doc.locale.as_str(), "en");
        assert_eq!(doc.kind, DocKind::Page);
        assert_eq!(doc.keywords, ["auth"]);
        assert_eq!(doc.groups, ["beta"]);
        assert_eq!(doc.regions, ["eu"]);
        assert_eq!(doc.boost, 1.0);
        assert_eq!(doc.updated, Some(1_700_000_000_000));
    }
}

#[test]
fn a_section_with_no_prose_is_still_findable_by_its_heading() {
    let ast = document(vec![heading(2, "empty", "Rate limits")]);
    let docs = section::extract(&ast, &page());
    assert_eq!(docs.len(), 1);
    assert_eq!(docs[0].section, "Rate limits");
    assert!(docs[0].body.is_empty());
}

#[test]
fn a_page_with_no_lead_prose_emits_no_lead_document() {
    let ast = document(vec![heading(2, "only", "Only section"), para("Body.")]);
    let docs = section::extract(&ast, &page());
    assert_eq!(docs.len(), 1);
    assert_eq!(docs[0].anchor, "only");
}

#[test]
fn deep_headings_stay_inside_the_section_above_them() {
    let ast = document(vec![
        heading(2, "limits", "Limits"),
        heading(4, "burst", "Burst"),
        para("Ten per second."),
    ]);
    let docs = section::extract(&ast, &page());
    assert_eq!(docs.len(), 1);
    assert!(docs[0].body.contains("Burst"), "{}", docs[0].body);
    assert!(docs[0].body.contains("Ten per second."));
}

#[test]
fn component_text_and_string_props_are_prose() {
    let mut props = Props::default();
    props
        .0
        .insert("title".to_owned(), PropValue::Str("Heads up".to_owned()));
    props.0.insert(
        "href".to_owned(),
        PropValue::Expr("{{ reader.plan }}".to_owned()),
    );
    let ast = document(vec![component("Callout", props, vec![para("Read this.")])]);
    let docs = section::extract(&ast, &page());
    assert!(docs[0].body.contains("Heads up"));
    assert!(docs[0].body.contains("Read this."));
    assert!(
        !docs[0].body.contains("reader."),
        "an unexpanded expression never reaches the index (SRC-12)"
    );
}

#[test]
fn an_endpoint_page_indexes_its_method_path_summary_and_responses() {
    let mut meta = page();
    meta.route = Route::new("/api-reference/users/get");
    meta.title = "Get a user".to_owned();
    meta.kind = DocKind::Endpoint;
    meta.endpoint = Some(EndpointFacts {
        method: "GET".to_owned(),
        path: "/users/{id}".to_owned(),
        summary: "Fetch one user by id.".to_owned(),
        parameters: vec!["id — the user's identifier".to_owned()],
        responses: vec!["200".to_owned(), "404".to_owned()],
    });
    let ast = document(vec![para("Returns the user record.")]);
    let docs = section::extract(&ast, &meta);

    let endpoint = &docs[0];
    assert_eq!(endpoint.kind, DocKind::Endpoint);
    assert!(endpoint.keywords.contains(&"GET".to_owned()));
    assert!(endpoint.keywords.contains(&"200".to_owned()));
    assert!(endpoint.keywords.contains(&"404".to_owned()));
    assert!(endpoint.code.contains("/users/{id}"), "{}", endpoint.code);
    assert!(endpoint.body.contains("Fetch one user by id."));
    assert!(endpoint.body.contains("the user's identifier"));
    assert!(endpoint.body.contains("Returns the user record."));
}

#[test]
fn an_endpoint_page_is_one_document_per_section_like_any_other() {
    let mut meta = page();
    meta.kind = DocKind::Endpoint;
    meta.endpoint = Some(EndpointFacts {
        method: "POST".to_owned(),
        path: "/users".to_owned(),
        summary: "Create a user.".to_owned(),
        parameters: Vec::new(),
        responses: vec!["201".to_owned()],
    });
    let ast = document(vec![
        para("Creates a user."),
        heading(2, "errors", "Errors"),
        para("Duplicate emails are rejected."),
    ]);
    let docs = section::extract(&ast, &meta);
    assert_eq!(docs.len(), 2);
    assert!(
        !docs[1].keywords.contains(&"POST".to_owned()),
        "the operation belongs to the lead document"
    );
    assert_eq!(docs[1].kind, DocKind::Endpoint);
}
