//! The endpoint contract (ANA-70), against the fixture the dashboard reads.
//!
//! `web/dashboard/test/api.test.ts` holds the TypeScript client to the same
//! file. The seam this protects is between two packages: `liyasa-server` owns
//! the routes, this one owns the caller, and neither compiles against the
//! other.

use liyasa_analytics::api::{self, ServedBy};
use serde_json::Value;

fn fixture() -> Value {
    let text = include_str!(concat!(
        env!("CARGO_MANIFEST_DIR"),
        "/../../web/dashboard/test/endpoints.fixture.json"
    ));
    serde_json::from_str(text).expect("the endpoint fixture parses")
}

#[test]
fn the_fixture_and_this_crate_list_the_same_endpoints() {
    // Compared as JSON rather than as structs: `Endpoint`'s fields are
    // `&'static str`, which borrow from the program rather than from a
    // document and so cannot be deserialised out of one.
    let ours = serde_json::to_value(api::ENDPOINTS).expect("the endpoint list serialises");
    assert_eq!(ours, fixture()["endpoints"]);
}

#[test]
fn the_base_path_is_the_one_the_server_already_serves_under() {
    assert_eq!(api::BASE, "/_liyasa/api/v1");
    assert_eq!(fixture()["base"], api::BASE);
    for endpoint in api::ENDPOINTS {
        assert!(
            endpoint.path.starts_with("/_liyasa/"),
            "{} is outside the reserved prefix and would be served as a page",
            endpoint.path
        );
    }
}

#[test]
fn no_endpoint_id_or_path_and_method_pair_appears_twice() {
    let mut ids = std::collections::BTreeSet::new();
    let mut pairs = std::collections::BTreeSet::new();
    for endpoint in api::ENDPOINTS {
        assert!(ids.insert(endpoint.id), "{} appears twice", endpoint.id);
        assert!(
            pairs.insert((endpoint.method, endpoint.path)),
            "{} {} is claimed by two ids",
            endpoint.method,
            endpoint.path
        );
    }
}

#[test]
fn the_deploy_paths_are_spelled_the_way_main_spells_them() {
    // Read off `crates/liyasa-server/src/deploy/routes.rs` on `main` at
    // 08f9ca3, where WP-16 merged them — not off `wp/16-deployments-git`,
    // which is the same code until integrate rebases it and stale after.
    // GIT-21 and GIT-24 both name the dashboard as their missing half.
    for (id, path) in [
        ("builds.trigger", "/_liyasa/api/v1/builds"),
        ("builds.queue", "/_liyasa/api/v1/builds"),
        ("builds.status", "/_liyasa/api/v1/builds/{id}"),
        ("builds.activate", "/_liyasa/api/v1/builds/{id}/deploy"),
        (
            "deployments.history",
            "/_liyasa/api/v1/deployments/{env}/history",
        ),
        (
            "deployments.retained",
            "/_liyasa/api/v1/deployments/{env}/retained",
        ),
        (
            "deployments.rollback",
            "/_liyasa/api/v1/deployments/{env}/rollback/{buildId}",
        ),
        (
            "deployments.latest",
            "/_liyasa/api/v1/deployments/{env}/latest",
        ),
    ] {
        let endpoint = api::endpoint(id).unwrap_or_else(|| panic!("{id} is missing"));
        assert_eq!(endpoint.path, path, "for {id}");
        assert_eq!(endpoint.served_by, ServedBy::Wp16, "for {id}");
    }
}

#[test]
fn the_paths_already_on_main_are_spelled_the_way_wp_14_spells_them() {
    // `crates/liyasa-server/src/routes/mod.rs`, merged.
    for (id, path) in [
        ("content.tree", "/_liyasa/api/v1/content"),
        ("feedback.list", "/_liyasa/feedback"),
        ("feedback.summary", "/_liyasa/feedback/summary"),
        ("feedback.status", "/_liyasa/feedback/{id}"),
        ("jobs.list", "/_liyasa/api/v1/jobs"),
        ("jobs.retry", "/_liyasa/api/v1/jobs/{id}/retry"),
        ("jobs.cancel", "/_liyasa/api/v1/jobs/{id}/cancel"),
        ("deployments.list", "/_liyasa/api/v1/deployments"),
        ("deployments.current", "/_liyasa/api/v1/deployments/{env}"),
    ] {
        let endpoint = api::endpoint(id).unwrap_or_else(|| panic!("{id} is missing"));
        assert_eq!(endpoint.path, path, "for {id}");
        assert_eq!(endpoint.served_by, ServedBy::Wp14, "for {id}");
    }
}

#[test]
fn the_analytics_reads_have_no_handler_and_the_list_says_so() {
    // The honest half of this package's state on 2026-09-17: every query in
    // this crate works and nothing serves it over HTTP yet, because the router
    // belongs to `liyasa-server`. A page whose endpoint is unbuilt renders its
    // controls and says so rather than drawing an empty chart.
    let unbuilt: Vec<&str> = api::unbuilt().map(|e| e.id).collect();
    assert!(unbuilt.contains(&"traffic.series"));
    assert!(unbuilt.contains(&"search.queries"));
    assert!(unbuilt.contains(&"insights.cards"));
    assert!(unbuilt.contains(&"settings.integrations"));
    assert!(
        !unbuilt.contains(&"jobs.list"),
        "the job endpoints are merged and must not be reported as missing"
    );
    assert_eq!(unbuilt.len(), 17);
}

#[test]
fn a_path_parameter_is_spelled_the_way_axum_spells_one() {
    for endpoint in api::ENDPOINTS {
        assert!(
            !endpoint.path.contains(':'),
            "{} uses the old `:name` form; axum 0.8 takes `{{name}}`",
            endpoint.path
        );
    }
}
