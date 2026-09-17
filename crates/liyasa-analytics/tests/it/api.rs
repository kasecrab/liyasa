//! The endpoint contract (ANA-70), against the fixture the dashboard reads.
//!
//! `web/dashboard/test/api.test.ts` holds the TypeScript client to the same
//! file. The seam this protects is between two packages: `liyasa-server` owns
//! the routes, this one owns the caller, and neither compiles against the
//! other.

use liyasa_analytics::api::{self, Auth, ServedBy};
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
fn the_two_endpoints_this_package_cannot_serve_are_the_ones_that_read_elsewhere() {
    // This assertion used to read "the analytics reads have no handler", and
    // it was true until `serve::mount` existed. What is left is the honest
    // remainder: two endpoints the dashboard calls whose data is in a store
    // this package does not own — drift is the verification engine's and
    // proposals are the editor's — so neither can be served from here however
    // the router is wired.
    let unbuilt: Vec<&str> = api::unbuilt().map(|e| e.id).collect();
    assert_eq!(unbuilt, ["drift.open", "proposals.list"]);

    let ours: Vec<&str> = api::ENDPOINTS
        .iter()
        .filter(|e| e.served_by == ServedBy::Wp17)
        .map(|e| e.id)
        .collect();
    assert_eq!(ours.len(), 18);
    for id in [
        "schema.event",
        "traffic.series",
        "search.queries",
        "assistant.summary",
        "insights.cards",
        "insights.act",
        "settings.integrations",
    ] {
        assert!(ours.contains(&id), "{id} should be served from this crate");
    }
    assert!(
        !ours.contains(&"jobs.list"),
        "the job endpoints are liyasa-server's and must not be claimed here"
    );
}

#[test]
fn the_published_schema_is_the_one_route_that_cannot_need_a_credential() {
    // ANA-02: a collector and a browser client validate their events against
    // this document BEFORE they are allowed to post any, so a caller that
    // reaches it has no dashboard credential by definition. A subtree that
    // applies one permission to everything it mounts cannot host it alongside
    // the rest, which is why `auth` is a field the mount can read.
    let schema = api::endpoint("schema.event").expect("ANA-02 publishes a schema");
    assert_eq!(schema.path, liyasa_analytics::schema::PATH);
    assert_eq!(schema.method, "GET");
    assert_eq!(schema.auth, Auth::Public);

    let public: Vec<&str> = api::behind(Auth::Public).map(|e| e.id).collect();
    assert_eq!(
        public,
        ["schema.event"],
        "every other endpoint is a dashboard operation and must stay gated"
    );
    assert_eq!(
        api::behind(Auth::DashboardRead).count(),
        api::ENDPOINTS.len() - 1
    );
}

#[test]
fn every_endpoint_the_dashboard_reads_is_gated() {
    // The inverse of the test above, and the one that would catch a new row
    // pasted in with the wrong `auth`: nothing that reads a reader's traffic,
    // a search query or a feedback comment may be public.
    for endpoint in api::ENDPOINTS {
        if endpoint.id == "schema.event" {
            continue;
        }
        assert_eq!(
            endpoint.auth,
            Auth::DashboardRead,
            "{} is public and reads analytics",
            endpoint.id
        );
    }
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
