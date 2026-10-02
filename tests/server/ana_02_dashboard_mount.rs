//! The analytics subtree: that the dashboard's routes are reachable at all,
//! and that ANA-02's schema is reachable by the caller who needs it.
//!
//! Eighteen analytics endpoints were written, unit-tested and driven as real
//! HTTP in `liyasa-analytics`'s own tests for weeks while nothing mounted
//! them — the eighth time this project has shipped a correct mechanism with
//! no caller. These tests are the mechanical answer to "is it wired": they
//! assert the router answers, not that the numbers are right, which is
//! `liyasa-analytics`'s job and already done there.

use http::StatusCode;
use liyasa_server::auth::roles::{Grant, Role};
use liyasa_server::auth::session::Principal;
use liyasa_server::org::model::Member;
use liyasa_tests::server::{Harness, Setup, body_json, expect_status};
use serde_json::json;

fn with_analytics(name: &str) -> Setup {
    Setup {
        analytics: true,
        ..Setup::new(name)
    }
}

/// Two statements, because neither alone says the dashboard is mounted.
///
/// This asserted only `status != 404`, and the status it was getting was
/// **401**: the subtree sits behind `Permission::DashboardRead`, a fixture site
/// configures no auth so no session layer is mounted, and the guard denies a
/// request carrying no grant. So the assertion passed on a router that had
/// mounted the subtree and on one that had mounted a guard over nothing, and it
/// would have gone on passing had the mount been removed and the guard left —
/// `!= 404` is satisfied by every failure mode there is.
///
/// The record is the direct claim. The exact 401 is the second half: it says
/// the credential is the only thing between the caller and the dashboard,
/// which is what distinguishes this from a 404 (nothing mounted) and from a
/// 500 (mounted and broken).
#[tokio::test]
async fn the_dashboard_endpoints_are_mounted_when_there_is_a_database() {
    let (harness, _site) = Harness::new(with_analytics("ana02-mounted")).await;

    let record = harness
        .mounted
        .iter()
        .find(|record| record.name == "analytics")
        .expect("the analytics subtree is declared");
    assert!(
        record.mounted && record.skipped.is_none(),
        "an instance with an analytics database must mount the dashboard: {record:?}"
    );

    expect_status(
        harness.get("/_liyasa/api/v1/analytics/totals").await,
        StatusCode::UNAUTHORIZED,
    );
}

#[tokio::test]
async fn an_instance_without_an_analytics_database_skips_rather_than_failing() {
    let (harness, _site) = Harness::serving("ana02-absent").await;

    expect_status(
        harness.get("/_liyasa/api/v1/analytics/totals").await,
        StatusCode::NOT_FOUND,
    );
    let record = harness
        .mounted
        .iter()
        .find(|record| record.name == "analytics")
        .expect("the analytics subtree is declared whether or not it mounts");
    assert!(
        record.skipped.is_some(),
        "an instance with no analytics database must say why it mounted nothing, \
         not mount an empty router"
    );
}

/// ANA-02. A static-site collector validates its events against this document
/// before it is allowed to post any, so it holds no dashboard credential by
/// construction. Inside the guarded layer this answers 401 to exactly the
/// caller that needs it, which is why it is a second subtree entry.
#[tokio::test]
async fn the_event_schema_is_served_without_a_dashboard_credential() {
    let (harness, _site) = Harness::serving("ana02-schema").await;

    let response = expect_status(
        harness.get(liyasa_analytics::schema::PATH).await,
        StatusCode::OK,
    );
    assert_eq!(
        liyasa_tests::server::header(&response, "content-type"),
        Some("application/schema+json")
    );
}

/// ANA-60. The `integrations` block reaches the view the dashboard is built
/// from, and is read as configuration rather than carried as an opaque value.
///
/// `view()` used to build `Analytics` as a struct literal that hardcoded
/// `integrations: Value::Null`, and `mount.rs` carried a note saying the
/// builders were "deliberately not used" because "each needs configuration
/// this state does not carry" — written before RFC 1403 put the whole of
/// `liyasa.json` on `AppState.config.site_config`. The note outlived its
/// reason, and nothing failed while it did: `configure(Null)` returns two
/// empty vectors, so the endpoint answered with `enabled: []` on an instance
/// that had configured a vendor, indistinguishable from one that had
/// configured none.
///
/// Asserted through `configure`, not on the field: the configured vendor comes
/// back by name, the junk key comes back as unknown, and `stuck` names the
/// vendor that wants consent on a site with no consent provider. Those are
/// three readings of the block, and an empty block produces none of them —
/// where `integrations != Null` would pass on `{}`.
#[tokio::test]
async fn the_configured_integrations_block_reaches_the_dashboard() {
    let (harness, _site) = Harness::new(Setup {
        analytics: true,
        site_config: Some(serde_json::json!({
            "name": "docs",
            "integrations": { "ga4": "G-XYZ", "notAVendor": true },
        })),
        ..Setup::new("ana60-integrations")
    })
    .await;

    let view = liyasa_server::routes::analytics::view(&harness.state)
        .expect("an instance with an analytics database has a dashboard view");
    let (enabled, unknown) = liyasa_analytics::integrations::configure(&view.integrations);

    let keys: Vec<&str> = enabled.iter().map(|c| c.key.as_str()).collect();
    assert_eq!(
        keys,
        ["ga4"],
        "the configured vendor must reach the dashboard; `[]` is what a \
         hardcoded `Value::Null` produced for seven weeks"
    );
    assert_eq!(
        unknown.iter().map(|u| u.0.as_str()).collect::<Vec<_>>(),
        ["notAVendor"],
        "an unknown key is how an operator ends up certain they enabled \
         something they did not, so it has to survive the hop"
    );
    assert_eq!(
        liyasa_analytics::integrations::gated_without_a_provider(&enabled)
            .iter()
            .map(|c| c.key.as_str())
            .collect::<Vec<_>>(),
        ["ga4"],
        "ga4 wants consent and this site configures no provider"
    );
}

/// The other half: absent must not read as configured-and-empty.
///
/// `with_integrations` defaults to `Value::Null` and so did the literal, so
/// this passed before the change too — it is here because the mapping from
/// "no `integrations` key" to `Null` is now written out in `view()`, and
/// `unwrap_or(Value::Null)` is the kind of line a later edit turns into
/// `unwrap_or_default()`, which for `Value` is `Null` today and is not
/// guaranteed to stay that way.
#[tokio::test]
async fn a_site_that_configures_no_integrations_enables_none() {
    let (harness, _site) = Harness::new(Setup {
        analytics: true,
        site_config: Some(serde_json::json!({ "name": "docs" })),
        ..Setup::new("ana60-no-integrations")
    })
    .await;

    let view = liyasa_server::routes::analytics::view(&harness.state).expect("a view");
    assert!(view.integrations.is_null(), "{:?}", view.integrations);
    let (enabled, unknown) = liyasa_analytics::integrations::configure(&view.integrations);
    assert!(enabled.is_empty() && unknown.is_empty());
}

/// The last hop, over real HTTP with a real credential: the handler reads the
/// block off the state the builder set.
///
/// The two tests above stop at `view()`, which is where this package's code
/// ends — they would both pass if `vendors` read some other field. This one
/// asks the dashboard. It needs a `DashboardRead` grant to get past the
/// subtree's guard, which is why it was not the first thing written: the
/// mechanism for reaching a guarded route in a test is `tests/server/mount.rs`'s
/// and was not obvious from this file.
#[tokio::test]
async fn an_editor_reads_the_configured_integrations_over_http() {
    let (harness, _site) = Harness::new(Setup {
        analytics: true,
        site_config: Some(json!({
            "name": "Acme docs",
            "seo": { "canonicalOrigin": "https://docs.acme.com" },
            "auth": { "mode": "password" },
            "integrations": { "ga4": "G-XYZ", "notAVendor": true },
        })),
        ..Setup::new("ana60-http")
    })
    .await;

    let auth = harness.state.auth_state().expect("auth mounted").clone();
    let org = harness.state.org_state().expect("an organization");
    org.write()
        .org
        .add_member(Member::new(
            "an-editor",
            "editor@example.com",
            Grant::role(Role::Editor),
        ))
        .expect("the member is added");
    let token = auth
        .tokens
        .issue_personal(
            &Principal::new("an-editor"),
            "ci",
            &Default::default(),
            None,
        )
        .expect("a token");

    let response = harness
        .get_with(
            "/_liyasa/api/v1/analytics/integrations",
            &[("authorization", &format!("Bearer {}", token.secret))],
        )
        .await;
    assert_eq!(
        response.status(),
        StatusCode::OK,
        "`Editor` carries `DashboardRead` (AUTH-30's table) and the subtree is \
         guarded by it; a 401 here means the credential, not the integrations"
    );

    let body = body_json(response).await;
    assert_eq!(
        body["enabled"][0]["key"], "ga4",
        "the dashboard reports the configured vendor: {body}"
    );
    assert_eq!(
        body["enabled"][0]["name"], "Google Analytics 4",
        "resolved against the catalogue rather than echoed: {body}"
    );
    assert_eq!(body["unknown"][0], "notAVendor", "{body}");
    assert_eq!(
        body["stuck"][0], "ga4",
        "ga4 wants consent and this site configures no provider: {body}"
    );
}
