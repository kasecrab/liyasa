//! CM-15: each template function, expanded against a fixture site.
//!
//! The site is a `Host` — the content tree, the asset manifest, the loaded
//! specs, the region, and the build clock — because that is the shape
//! `liyasa-markdown` takes a build in: the crate does no I/O, so a build hands
//! it the answers rather than a path to look them up at (§6.2). Filling a
//! `Host` from a real project is `liyasa-build`'s, and until it does the
//! functions answer in this test and not in a `liyasa build`.

use std::collections::BTreeMap;
use std::sync::Arc;

use liyasa_core::markdown::TemplateContext;
use liyasa_core::source_map::SourceMap;
use liyasa_core::vfs::VfsPath;
use liyasa_markdown::source::context::Layers;
use liyasa_markdown::source::host::{Host, PageEntry};
use liyasa_markdown::source::{ExpandOptions, environment, expand, host, scan};

const CLOCK: &str = "2026-01-01T09:30:00Z";

fn entry(id: &str, route: &str, data: serde_json::Value) -> PageEntry {
    PageEntry {
        id: id.to_owned(),
        route: route.to_owned(),
        data,
    }
}

/// The fixture site: four pages in content-tree order, one asset, one spec.
fn site() -> Host {
    Host {
        pages: vec![
            entry("index", "/", serde_json::json!({ "title": "Home" })),
            entry(
                "guides/install",
                "/guides/install",
                serde_json::json!({ "title": "Install", "order": 1, "draft": false }),
            ),
            entry(
                "guides/configure",
                "/guides/configure",
                serde_json::json!({ "title": "Configure", "order": 2, "draft": false }),
            ),
            entry(
                "guides/tuning",
                "/guides/tuning",
                serde_json::json!({ "title": "Tuning", "order": 3, "draft": true }),
            ),
        ],
        assets: BTreeMap::from([(
            "img/logo.svg".to_owned(),
            "/_liyasa/img/logo.9f8e7d.svg".to_owned(),
        )]),
        openapi: BTreeMap::from([(
            "petstore".to_owned(),
            BTreeMap::from([(
                "listPets".to_owned(),
                serde_json::json!({ "operationId": "listPets", "summary": "List pets" }),
            )]),
        )]),
        region: Some("eu".to_owned()),
        features: BTreeMap::from([
            ("logs".to_owned(), vec!["eu".to_owned(), "us".to_owned()]),
            ("mfa".to_owned(), vec!["us".to_owned()]),
        ]),
        now: Some(CLOCK.to_owned()),
    }
}

fn context() -> TemplateContext {
    Layers {
        facts: serde_json::json!({ "plan": { "pro": { "price": 20 } } }),
        page: serde_json::json!({ "title": "Rate limits" }),
        env: serde_json::json!({ "CI": "true" }),
        ..Layers::default()
    }
    .build()
}

fn render(source: &str) -> String {
    let mut map = SourceMap::new();
    let id = map.intern(VfsPath::new("page.md"), Arc::from(source));
    let (document, diagnostics) = scan(source, id);
    assert!(
        !diagnostics.has_errors(),
        "the fixture does not scan: {:?}",
        diagnostics.iter().map(|d| d.code).collect::<Vec<_>>()
    );
    let mut env = environment(&ExpandOptions::default());
    host::install(&mut env, Arc::new(site()));
    match expand(&map, &document, &context(), &env) {
        Ok(expanded) => expanded.text,
        Err(diagnostics) => panic!(
            "expansion failed: {:?}",
            diagnostics
                .iter()
                .map(|d| format!("{} {}", d.code, d.message))
                .collect::<Vec<_>>()
        ),
    }
}

fn fails(source: &str) -> Vec<String> {
    let mut map = SourceMap::new();
    let id = map.intern(VfsPath::new("page.md"), Arc::from(source));
    let (document, _) = scan(source, id);
    let mut env = environment(&ExpandOptions::default());
    host::install(&mut env, Arc::new(site()));
    match expand(&map, &document, &context(), &env) {
        Ok(text) => panic!("expected a failure, got {text:?}"),
        Err(diagnostics) => diagnostics
            .iter()
            .map(|d| d.code.as_str().to_owned())
            .collect(),
    }
}

#[test]
fn pages_lists_the_expected_pages_in_order() {
    assert_eq!(
        render(r#"{% for p in pages("guides/*") %}{{ p.id }} {% endfor %}"#),
        "guides/install guides/configure guides/tuning "
    );
}

#[test]
fn pages_filters_on_front_matter() {
    assert_eq!(
        render(r#"{% for p in pages("guides/*", draft=false) %}{{ p.title }} {% endfor %}"#),
        "Install Configure "
    );
    assert_eq!(
        render(r#"{{ pages("guides/*", order=2)[0].id }}"#),
        "guides/configure"
    );
}

#[test]
fn link_resolves_to_a_route() {
    assert_eq!(
        render(r#"{{ "guides/install" | link }}"#),
        "/guides/install"
    );
    assert_eq!(
        render(r#"{% for p in pages("guides/*", draft=false) %}{{ p.id | link }} {% endfor %}"#),
        "/guides/install /guides/configure "
    );
}

#[test]
fn now_equals_the_build_clock() {
    assert_eq!(render("{{ now() }}"), CLOCK);
    assert_eq!(render(r#"{{ now() | date("%Y-%m-%d") }}"#), "2026-01-01");
    // Twice in one page is the same instant: a build clock, not a wall clock.
    assert_eq!(
        render("{{ now() }} {{ now() }}"),
        format!("{CLOCK} {CLOCK}")
    );
}

#[test]
fn page_reads_the_tree_and_its_own_front_matter() {
    assert_eq!(render("{{ page.title }}"), "Rate limits");
    assert_eq!(render(r#"{{ page("guides/install").title }}"#), "Install");
    assert_eq!(render(r#"{{ page("index").route }}"#), "/");
}

#[test]
fn snippet_fact_and_env_answer_out_of_the_context() {
    assert_eq!(render(r#"{{ fact("plan.pro.price") }}"#), "20");
    assert_eq!(render(r#"{{ env("CI") }}"#), "true");
}

#[test]
fn openapi_reads_an_operation() {
    assert_eq!(
        render(r#"{{ openapi("petstore", "listPets").summary }}"#),
        "List pets"
    );
}

#[test]
fn region_available_answers_for_the_region_being_built() {
    assert_eq!(
        render(r#"{% if region_available("logs") %}yes{% else %}no{% endif %}"#),
        "yes"
    );
    assert_eq!(
        render(r#"{% if region_available("mfa") %}yes{% else %}no{% endif %}"#),
        "no"
    );
}

#[test]
fn range_and_dict_are_the_builtins_the_requirement_names() {
    assert_eq!(render("{% for n in range(3) %}{{ n }}{% endfor %}"), "012");
    assert_eq!(render(r#"{{ dict(a=1, b=2).b }}"#), "2");
}

#[test]
fn every_function_reports_what_it_could_not_find() {
    assert_eq!(fails(r#"{{ page("guides/nope") }}"#), ["E0213"]);
    assert_eq!(fails(r#"{{ "guides/nope" | link }}"#), ["E0213"]);
    assert_eq!(fails(r#"{{ "img/nope.svg" | asset }}"#), ["E0214"]);
    assert_eq!(fails(r#"{{ openapi("petstore", "nope") }}"#), ["E0215"]);
    assert_eq!(fails(r#"{{ region_available("nope") }}"#), ["E0216"]);
    assert_eq!(fails(r#"{{ fact("plan.free.price") }}"#), ["E0209"]);
    assert_eq!(fails(r#"{{ env("SECRET") }}"#), ["E0211"]);
}

/// A build that has not filled a `Host` in still renders every page that does
/// not reach for one, and tells the truth about the ones that do.
#[test]
fn a_build_that_installed_nothing_still_says_what_is_missing() {
    let source = "{{ now() }}\n";
    let mut map = SourceMap::new();
    let id = map.intern(VfsPath::new("page.md"), Arc::from(source));
    let (document, _) = scan(source, id);
    let env = environment(&ExpandOptions::default());
    let diagnostics = expand(&map, &document, &context(), &env).expect_err("no build clock");
    assert_eq!(
        diagnostics
            .iter()
            .map(|d| d.code.as_str())
            .collect::<Vec<_>>(),
        ["E0216"]
    );
}
