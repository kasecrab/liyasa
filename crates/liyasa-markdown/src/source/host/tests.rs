use serde_json::json;

use super::*;

fn entry(id: &str, route: &str, data: serde_json::Value) -> PageEntry {
    PageEntry {
        id: id.to_owned(),
        route: route.to_owned(),
        data,
    }
}

fn host() -> Arc<Host> {
    Arc::new(Host {
        pages: vec![
            entry("index", "/", json!({ "title": "Home" })),
            entry(
                "guides/install",
                "/guides/install",
                json!({ "title": "Install", "draft": false }),
            ),
            entry(
                "guides/deep/tuning",
                "/guides/deep/tuning",
                json!({ "title": "Tuning", "draft": true }),
            ),
            entry(
                "reference/cli",
                "/reference/cli",
                json!({ "title": "CLI", "draft": false }),
            ),
        ],
        assets: BTreeMap::from([(
            "img/logo.svg".to_owned(),
            "/_liyasa/img/logo.9f8e7d.svg".to_owned(),
        )]),
        openapi: BTreeMap::from([(
            "petstore".to_owned(),
            BTreeMap::from([("listPets".to_owned(), json!({ "summary": "List pets" }))]),
        )]),
        region: Some("eu".to_owned()),
        features: BTreeMap::from([
            ("logs".to_owned(), vec!["eu".to_owned(), "us".to_owned()]),
            ("mfa".to_owned(), vec!["us".to_owned()]),
        ]),
        now: Some("2026-09-16T00:00:00Z".to_owned()),
    })
}

fn environment(host: Arc<Host>) -> Environment<'static> {
    let mut env = Environment::new();
    crate::source::filters::install(&mut env);
    install(&mut env, host);
    env.set_undefined_behavior(minijinja::UndefinedBehavior::Strict);
    env
}

fn render_on(host: Arc<Host>, template: &str, values: Value) -> String {
    environment(host)
        .template_from_str(template)
        .expect("a valid template")
        .render(values)
        .expect("a successful render")
}

fn render(template: &str) -> String {
    render_on(host(), template, Value::UNDEFINED)
}

fn fails_on(host: Arc<Host>, template: &str) -> String {
    environment(host)
        .template_from_str(template)
        .expect("a valid template")
        .render(Value::UNDEFINED)
        .expect_err("a failure")
        .to_string()
}

fn fails(template: &str) -> String {
    fails_on(host(), template)
}

// ---- CM-14: link, asset ----

#[test]
fn link_resolves_a_page_id_to_a_route() {
    assert_eq!(
        render(r#"{{ "guides/install" | link }}"#),
        "/guides/install"
    );
}

#[test]
fn link_carries_a_fragment_through() {
    assert_eq!(
        render(r#"{{ "guides/install#requirements" | link }}"#),
        "/guides/install#requirements"
    );
}

#[test]
fn an_unknown_page_is_reported_with_the_nearest_id() {
    let message = fails(r#"{{ "guides/instal" | link }}"#);
    assert!(message.contains("E0213"), "{message}");
    assert!(message.contains("guides/install"), "{message}");
}

#[test]
fn asset_resolves_to_the_hashed_url() {
    assert_eq!(
        render(r#"{{ "img/logo.svg" | asset }}"#),
        "/_liyasa/img/logo.9f8e7d.svg"
    );
    assert_eq!(
        render(r#"{{ "/img/logo.svg" | asset }}"#),
        "/_liyasa/img/logo.9f8e7d.svg"
    );
}

#[test]
fn an_asset_the_build_did_not_produce_is_reported() {
    assert!(fails(r#"{{ "img/missing.svg" | asset }}"#).contains("E0214"));
}

// ---- CM-15: page, pages ----

#[test]
fn page_reads_its_front_matter() {
    assert_eq!(render(r#"{{ page("guides/install").title }}"#), "Install");
    assert_eq!(
        render(r#"{{ page("guides/install").route }}"#),
        "/guides/install"
    );
    assert_eq!(render(r#"{{ page("index").id }}"#), "index");
}

#[test]
fn pages_lists_the_tree_in_its_own_order() {
    assert_eq!(
        render(r#"{% for p in pages("**") %}{{ p.id }} {% endfor %}"#),
        "index guides/install guides/deep/tuning reference/cli "
    );
}

#[test]
fn a_single_star_does_not_cross_a_separator() {
    assert_eq!(
        render(r#"{% for p in pages("guides/*") %}{{ p.id }}{% endfor %}"#),
        "guides/install"
    );
}

#[test]
fn a_double_star_crosses_separators_and_matches_none_of_them() {
    assert_eq!(
        render(r#"{% for p in pages("guides/**") %}{{ p.id }} {% endfor %}"#),
        "guides/install guides/deep/tuning "
    );
    let host = Arc::new(Host {
        pages: vec![entry("guides/x", "/guides/x", json!({}))],
        ..Host::default()
    });
    assert_eq!(
        render_on(
            host,
            r#"{% for p in pages("guides/**/x") %}{{ p.id }}{% endfor %}"#,
            Value::UNDEFINED
        ),
        "guides/x"
    );
}

#[test]
fn a_question_mark_matches_one_character() {
    assert_eq!(
        render(r#"{% for p in pages("inde?") %}{{ p.id }}{% endfor %}"#),
        "index"
    );
}

#[test]
fn a_keyword_filters_on_front_matter() {
    assert_eq!(
        render(r#"{% for p in pages("**", draft=false) %}{{ p.id }} {% endfor %}"#),
        "guides/install reference/cli "
    );
}

#[test]
fn a_glob_that_matches_nothing_is_an_empty_list_not_an_error() {
    assert_eq!(render(r#"{{ pages("nothing/*") | length }}"#), "0");
}

#[test]
fn an_unknown_page_is_reported_by_the_function_too() {
    assert!(fails(r#"{{ page("nope").title }}"#).contains("E0213"));
}

// ---- CM-15: openapi, region_available, now ----

#[test]
fn openapi_reads_an_operation() {
    assert_eq!(
        render(r#"{{ openapi("petstore", "listPets").summary }}"#),
        "List pets"
    );
}

#[test]
fn an_unknown_spec_and_an_unknown_operation_are_both_reported() {
    assert!(fails(r#"{{ openapi("nope", "listPets") }}"#).contains("E0215"));
    let message = fails(r#"{{ openapi("petstore", "listPet") }}"#);
    assert!(message.contains("E0215"), "{message}");
    assert!(message.contains("listPets"), "{message}");
}

#[test]
fn region_available_answers_for_the_region_being_built() {
    // minijinja prints a bool the way Jinja2 does.
    assert_eq!(render(r#"{{ region_available("logs") }}"#), "True");
    assert_eq!(render(r#"{{ region_available("mfa") }}"#), "False");
}

#[test]
fn a_build_without_a_region_cannot_answer() {
    let message = fails_on(
        Arc::new(Host::default()),
        r#"{{ region_available("logs") }}"#,
    );
    assert!(message.contains("E0216"), "{message}");
}

#[test]
fn an_unknown_feature_is_reported_rather_than_answered_false() {
    assert!(fails(r#"{{ region_available("logz") }}"#).contains("E0216"));
}

#[test]
fn now_is_the_build_clock() {
    assert_eq!(render("{{ now() }}"), "2026-09-16T00:00:00Z");
    assert_eq!(render(r#"{{ now() | date("%Y") }}"#), "2026");
}

#[test]
fn a_build_without_a_clock_has_no_now() {
    let message = fails_on(Arc::new(Host::default()), "{{ now() }}");
    assert!(message.contains("E0216"), "{message}");
}

// ---- the empty host ----

#[test]
fn every_name_exists_before_the_build_supplies_anything() {
    // The failure a page gets is about the thing that was missing, never
    // `E0203 unknown filter`.
    for (template, code) in [
        (r#"{{ "x" | link }}"#, "E0213"),
        (r#"{{ "x" | asset }}"#, "E0214"),
        (r#"{{ page("x") }}"#, "E0213"),
        (r#"{{ openapi("x", "y") }}"#, "E0215"),
        (r#"{{ region_available("x") }}"#, "E0216"),
        ("{{ now() }}", "E0216"),
    ] {
        let message = fails_on(Arc::new(Host::default()), template);
        assert!(message.contains(code), "{template}: {message}");
    }
    assert_eq!(
        render_on(
            Arc::new(Host::default()),
            r#"{{ pages("**") | length }}"#,
            Value::UNDEFINED
        ),
        "0"
    );
}

// ---- CM-15: the minijinja builtins the requirement also names ----

#[test]
fn range_and_dict_are_the_builtins() {
    assert_eq!(render("{% for n in range(3) %}{{ n }}{% endfor %}"), "012");
    assert_eq!(render(r#"{{ dict(a=1).a }}"#), "1");
}

#[test]
fn a_page_may_be_looked_up_in_a_loop_over_a_glob() {
    let out =
        render(r#"{% for p in pages("guides/*") %}[{{ p.title }}]({{ p.id | link }}){% endfor %}"#);
    assert_eq!(out, "[Install](/guides/install)");
}

#[test]
fn the_nearest_suggestion_is_not_offered_for_something_unrelated() {
    let message = fails_on(
        Arc::new(Host {
            pages: vec![entry("index", "/", json!({}))],
            ..Host::default()
        }),
        r#"{{ "completely-different" | link }}"#,
    );
    assert!(!message.contains("did you mean"), "{message}");
}
