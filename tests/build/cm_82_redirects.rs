//! CM-82: redirects with wildcards and named parameters reach `_redirects`,
//! `vercel.json`, and the manifest, and a renamed page gets one for free.

use liyasa_build::redirects::{self, Table};

fn table(rules: &[(&str, &str)]) -> Table {
    let rules: Vec<redirects::Input> = rules
        .iter()
        .map(|(source, destination)| redirects::Input {
            source: (*source).to_owned(),
            destination: (*destination).to_owned(),
            status: None,
        })
        .collect();
    let (table, diagnostics) = Table::compile(&rules, &[]);
    assert!(
        !diagnostics.has_errors(),
        "compiling {rules:?} failed: {diagnostics:?}"
    );
    table
}

#[test]
fn a_wildcard_carries_the_rest_of_the_path_across() {
    let table = table(&[("/v1/*", "/v2/:splat")]);
    let hit = table.resolve("/v1/guides/install").expect("a redirect");
    assert_eq!(hit.destination, "/v2/guides/install");
    assert_eq!(hit.status, 301);
}

#[test]
fn a_named_parameter_is_interpolated_into_the_destination() {
    let table = table(&[("/docs/:slug", "/guides/:slug")]);
    let hit = table.resolve("/docs/install").expect("a redirect");
    assert_eq!(hit.destination, "/guides/install");
}

#[test]
fn a_temporary_redirect_keeps_its_status() {
    let rules = [redirects::Input {
        source: "/beta".to_owned(),
        destination: "/preview".to_owned(),
        status: Some(302),
    }];
    let (table, _) = Table::compile(&rules, &[]);
    assert_eq!(table.resolve("/beta").expect("a redirect").status, 302);
}

#[test]
fn the_netlify_file_lists_every_rule() {
    let table = table(&[("/v1/*", "/v2/:splat"), ("/docs/:slug", "/guides/:slug")]);
    let text = redirects::netlify(&table);
    assert_eq!(
        text.lines().collect::<Vec<_>>(),
        ["/v1/* /v2/:splat 301", "/docs/:slug /guides/:slug 301"]
    );
}

#[test]
fn the_vercel_file_lists_every_rule_with_permanence() {
    let table = table(&[("/v1/*", "/v2/:splat")]);
    let json: serde_json::Value =
        serde_json::from_str(&redirects::vercel(&table)).expect("vercel.json is JSON");
    let first = &json["redirects"][0];
    assert_eq!(first["source"], "/v1/:splat*");
    assert_eq!(first["destination"], "/v2/:splat");
    assert_eq!(first["permanent"], true);
}

#[test]
fn the_manifest_entries_are_the_compiled_rules() {
    let table = table(&[("/v1/*", "/v2/:splat")]);
    let entries = table.manifest_entries();
    assert_eq!(entries.len(), 1);
    assert_eq!(entries[0].source, "/v1/*");
    assert_eq!(entries[0].destination, "/v2/:splat");
    assert_eq!(entries[0].status, 301);
}

#[test]
fn an_absolute_destination_needs_its_host_on_the_allow_list() {
    let rules = [redirects::Input {
        source: "/status".to_owned(),
        destination: "https://status.example.com/".to_owned(),
        status: None,
    }];
    let (_, denied) = Table::compile(&rules, &[]);
    assert!(denied.has_errors());
    assert!(
        denied
            .iter()
            .any(|diagnostic| diagnostic.code.as_str() == "E0109")
    );

    let (allowed, diagnostics) = Table::compile(&rules, &["status.example.com".to_owned()]);
    assert!(!diagnostics.has_errors());
    assert_eq!(
        allowed.resolve("/status").expect("a redirect").destination,
        "https://status.example.com/"
    );
}

#[test]
fn a_parameter_may_never_reach_the_scheme_or_the_host() {
    for destination in [
        "https://:host/docs",
        "https://:splat.example.com/docs",
        "//:splat/docs",
    ] {
        let rules = [redirects::Input {
            source: "/go/:host".to_owned(),
            destination: destination.to_owned(),
            status: None,
        }];
        let (table, diagnostics) = Table::compile(&rules, &["example.com".to_owned()]);
        assert!(
            diagnostics
                .iter()
                .any(|diagnostic| diagnostic.code.as_str() == "E0109"),
            "{destination} should be refused"
        );
        assert!(table.resolve("/go/evil.example").is_none());
    }
}

#[test]
fn two_rules_with_the_same_source_are_a_conflict() {
    let rules = [
        redirects::Input {
            source: "/a".to_owned(),
            destination: "/b".to_owned(),
            status: None,
        },
        redirects::Input {
            source: "/a".to_owned(),
            destination: "/c".to_owned(),
            status: None,
        },
    ];
    let (table, diagnostics) = Table::compile(&rules, &[]);
    assert!(
        diagnostics
            .iter()
            .any(|diagnostic| diagnostic.code.as_str() == "E0106")
    );
    assert_eq!(
        table
            .resolve("/a")
            .expect("the first rule wins")
            .destination,
        "/b"
    );
}

#[test]
fn a_renamed_page_gets_a_permanent_redirect() {
    let rule = redirects::for_move("/guides/install", "/guides/setup");
    assert_eq!(rule.source, "/guides/install");
    assert_eq!(rule.destination, "/guides/setup");
    assert_eq!(rule.status, Some(301));
}

#[test]
fn a_path_that_matches_nothing_is_not_redirected() {
    let table = table(&[("/v1/*", "/v2/:splat")]);
    assert!(table.resolve("/v2/guides").is_none());
    assert!(table.resolve("/").is_none());
}
