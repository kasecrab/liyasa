//! `contextRepos[]`, read into the shape GIT-11's clone policy needs, and the
//! three refusals a configuration can be checked for on its own (CFG-99).

use liyasa_config::context_repos::{
    ContextRepoConfig, context_repos, escapes, parse_bytes, parse_refresh_seconds,
};
use liyasa_config::json::SpanIndex;
use liyasa_config::pages::Pages;
use liyasa_config::validate::{self, Context, Mode};
use liyasa_core::span::SourceId;
use serde_json::json;

/// Every fixture below carries `seo.canonicalOrigin`, because a config without
/// one is `W0131` and this file is about `W0142`: an assertion on the whole set
/// of diagnostics is worth more than one that filters to the code it expects,
/// and it only works if the fixture is otherwise clean.
fn diagnose(text: &str) -> Vec<(String, String)> {
    let value: serde_json::Value = serde_json::from_str(text).expect("the fixture is valid JSON");
    let spans = SpanIndex::scan(SourceId(0), text);
    let pages = Pages::new();
    validate::validate(
        &value,
        &spans,
        &Context {
            pages: &pages,
            mode: Mode::Build,
        },
    )
    .iter()
    .map(|diagnostic| {
        (
            diagnostic.code.as_str().to_owned(),
            diagnostic.message.clone(),
        )
    })
    .collect()
}

#[test]
fn an_entry_is_read_field_for_field() {
    let repos = context_repos(&json!({
        "contextRepos": [{
            "repo": "acme/api",
            "ref": "v2",
            "paths": ["openapi.yaml", "spec/"],
            "depth": 1,
            "maxBytes": "512MB",
            "refresh": "180d"
        }]
    }));
    assert_eq!(
        repos,
        vec![ContextRepoConfig {
            repo: "acme/api".to_owned(),
            r#ref: Some("v2".to_owned()),
            paths: vec!["openapi.yaml".to_owned(), "spec/".to_owned()],
            depth: Some(1),
            max_bytes: Some(512 * 1024 * 1024),
            refresh_seconds: Some(180 * 24 * 60 * 60),
        }]
    );
}

#[test]
fn what_is_absent_is_left_to_the_consumer() {
    let repos = context_repos(&json!({ "contextRepos": [{ "repo": "acme/api" }] }));
    let entry = repos.first().expect("the entry");
    // Not filled in here: `liyasa_git` has DEFAULT_DEPTH, DEFAULT_MAX_BYTES
    // and DEFAULT_REFRESH, and a default invented twice is a default that can
    // disagree with itself.
    assert_eq!(entry.depth, None);
    assert_eq!(entry.max_bytes, None);
    assert_eq!(entry.refresh_seconds, None);
    assert!(entry.paths.is_empty(), "empty is not everything");
}

#[test]
fn a_key_that_is_not_there_reads_as_no_repositories() {
    assert!(context_repos(&json!({ "name": "Acme" })).is_empty());
    // A shape the schema refuses reads as absent rather than panicking.
    assert!(context_repos(&json!({ "contextRepos": "acme/api" })).is_empty());
    assert!(context_repos(&json!({ "contextRepos": [{ "ref": "v2" }] })).is_empty());
}

#[test]
fn the_sizes_are_the_ones_a_human_typed() {
    assert_eq!(parse_bytes("512MB"), Some(512 * 1024 * 1024));
    assert_eq!(
        parse_bytes("1.5 GB"),
        Some(1024 * 1024 * 1024 + 536_870_912)
    );
    assert_eq!(parse_bytes("900"), Some(900), "a bare number is bytes");
    assert_eq!(parse_bytes("12 flagons"), None);

    assert_eq!(parse_refresh_seconds("30s"), Some(30));
    assert_eq!(parse_refresh_seconds("6h"), Some(6 * 60 * 60));
    assert_eq!(
        parse_refresh_seconds("500ms"),
        Some(0),
        "a sub-second refresh is `every time you ask`, not one second"
    );
    assert_eq!(
        parse_refresh_seconds("1w"),
        None,
        "not a unit the schema has"
    );
}

#[test]
fn only_dot_dot_leaves_the_repository() {
    assert!(escapes("../spec"));
    assert!(escapes("spec/../../etc"));
    assert!(!escapes("spec/openapi.yaml"));
    // Git's sparse-checkout syntax anchors with a leading slash, so this is
    // the repository's own `etc`, not the host's.
    assert!(!escapes("/etc/passwd"));
}

#[test]
fn an_entry_with_no_paths_is_w0142_naming_the_repository() {
    let found = diagnose(
        r#"{ "name": "Acme", "seo": { "canonicalOrigin": "https://acme.dev" },
             "contextRepos": [{ "repo": "acme/api" }] }"#,
    );
    assert_eq!(found.len(), 1, "{found:?}");
    let (code, message) = &found[0];
    assert_eq!(code, "W0142");
    assert!(
        message.contains("acme/api") && message.contains("no paths"),
        "{message}"
    );
}

#[test]
fn a_path_that_climbs_out_is_w0142_naming_the_path() {
    let found = diagnose(
        r#"{ "name": "Acme", "seo": { "canonicalOrigin": "https://acme.dev" },
             "contextRepos": [{ "repo": "acme/api", "paths": ["spec", "../secrets"] }] }"#,
    );
    assert_eq!(found.len(), 1, "{found:?}");
    assert_eq!(found[0].0, "W0142");
    assert!(found[0].1.contains("../secrets"), "{:?}", found[0].1);
}

#[test]
fn a_well_formed_entry_says_nothing() {
    let found = diagnose(
        r#"{ "name": "Acme", "seo": { "canonicalOrigin": "https://acme.dev" },
             "contextRepos": [{ "repo": "acme/api", "paths": ["openapi.yaml"],
                                "maxBytes": "512MB" }] }"#,
    );
    assert!(found.is_empty(), "{found:?}");
}

#[test]
fn past_the_limit_is_one_warning_about_the_list() {
    let entries: Vec<serde_json::Value> = (0..11)
        .map(|n| json!({ "repo": format!("acme/api-{n}"), "paths": ["openapi.yaml"] }))
        .collect();
    let text = serde_json::to_string(&json!({
        "name": "Acme",
        "seo": { "canonicalOrigin": "https://acme.dev" },
        "contextRepos": entries
    }))
    .expect("the fixture serializes");
    let found = diagnose(&text);
    assert_eq!(found.len(), 1, "one warning for the list, not one each");
    assert_eq!(found[0].0, "W0142");
    assert!(
        found[0].1.contains("11") && found[0].1.contains("10"),
        "{:?}",
        found[0].1
    );
}

#[test]
fn the_span_points_at_the_entry_that_is_wrong() {
    let text = "{\n  \"seo\": { \"canonicalOrigin\": \"https://acme.dev\" },\n  \"contextRepos\": [\n    { \"repo\": \"ok\", \"paths\": [\"a\"] },\n    { \"repo\": \"bare\" }\n  ]\n}\n";
    let value: serde_json::Value = serde_json::from_str(text).expect("valid JSON");
    let spans = SpanIndex::scan(SourceId(0), text);
    let pages = Pages::new();
    let found = validate::validate(
        &value,
        &spans,
        &Context {
            pages: &pages,
            mode: Mode::Build,
        },
    );
    let span = found
        .iter()
        .next()
        .and_then(|diagnostic| diagnostic.span)
        .expect("the diagnostic is located");
    let line = text[..span.start as usize].matches('\n').count() + 1;
    assert_eq!(line, 5, "the second entry is on line 5, the first is fine");
}

/// The agent's own `ai.agent.contextRepos` is a different key with a different
/// meaning (AGT-12, RFC 0112), and until this sitting its entries were
/// `"items": {}` — anything at all validated. A typo in an allow list is the
/// failure that looks like success there: `liyasa_agent::repos` reads an entry
/// with no `allow` as "may read nothing", so a misspelt key is a silent denial.
#[test]
fn a_typo_in_an_agent_context_repo_is_reported() {
    let text = r#"{ "name": "Acme",
        "ai": { "agent": { "contextRepos": [{ "name": "acme/api", "alow": ["src"] }] } } }"#;
    let value: serde_json::Value = serde_json::from_str(text).expect("valid JSON");
    let report = liyasa_config::schema::check(&value, &SpanIndex::scan(SourceId(0), text));
    let codes: Vec<&str> = report
        .diagnostics
        .iter()
        .map(|diagnostic| diagnostic.code.as_str())
        .collect();
    assert!(codes.contains(&"E0103"), "{codes:?}");
    assert!(
        report.unknown.iter().any(|path| path.ends_with("alow")),
        "{:?}",
        report.unknown
    );
}

#[test]
fn an_agent_context_repo_with_no_name_is_refused() {
    let text = r#"{ "name": "Acme",
        "ai": { "agent": { "contextRepos": [{ "allow": ["src"] }] } } }"#;
    let value: serde_json::Value = serde_json::from_str(text).expect("valid JSON");
    let report = liyasa_config::schema::check(&value, &SpanIndex::scan(SourceId(0), text));
    assert!(
        report.diagnostics.has_errors(),
        "an entry the agent cannot key on is not a configuration: {:?}",
        report.diagnostics
    );
}
