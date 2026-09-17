//! The organization API as the server composes it (ORG-01..03, ORG-20, ORG-21,
//! ORG-30..33, HOST-10, HOST-11, HOST-13).
//!
//! Every request here goes through `org::routes::router`, which is what
//! `org::mount` returns and therefore what `routes::application` would mount —
//! guard layer included. A principal is put into the extensions by a layer in
//! front of it, which is exactly where WP-15's session layer will put one
//! (RFC 1403). So these assertions are about the routed, guarded product
//! rather than about a router assembled for the test.
//!
//! What is not covered here, and cannot be: `org` is not in
//! `routes::mount::subtrees()` yet, because that file is WP-14's. Until the
//! one line in RFC 2800 lands, `liyasa serve` routes none of this.

use std::sync::Arc;

use axum::Router;
use axum::body::Body;
use http::{Request, StatusCode};
use liyasa_server::auth::roles::{Grant, Role};
use liyasa_server::auth::session::Principal;
use liyasa_server::org::audit::Area;
use liyasa_server::org::model::{Member, Settings};
use liyasa_server::org::plan::{Plan, Tier};
use liyasa_server::org::region::Region;
use liyasa_server::org::state::OrgState;
use liyasa_server::org::{routes, state};
use serde_json::{Value, json};
use tower::ServiceExt as _;

const ORG: &str = "/_liyasa/api/v1/org";

fn state_with(tier: Tier) -> Arc<OrgState> {
    Arc::new(OrgState::with_clock(
        "acme",
        Settings::new("Acme"),
        Plan::of(tier),
        liyasa_server::auth::clock::Clock::manual(),
    ))
}

/// The guarded subtree with a signed-in caller in front of it, which is the
/// shape the product takes once a session layer exists.
fn as_role(state: &Arc<OrgState>, role: Role) -> Router {
    let principal = Principal {
        subject: "u1".to_owned(),
        role,
        ..Principal::default()
    };
    routes::router(state.clone()).layer(axum::middleware::from_fn(
        move |mut request: axum::extract::Request, next: axum::middleware::Next| {
            let principal = principal.clone();
            async move {
                request.extensions_mut().insert(principal);
                next.run(request).await
            }
        },
    ))
}

async fn send(
    router: &Router,
    method: &str,
    path: &str,
    body: Option<Value>,
) -> (StatusCode, Value) {
    let builder = Request::builder().method(method).uri(path);
    let request = match body {
        Some(body) => builder
            .header("content-type", "application/json")
            .body(Body::from(body.to_string())),
        None => builder.body(Body::empty()),
    }
    .expect("a request");
    let response = router.clone().oneshot(request).await.expect("a response");
    let status = response.status();
    let bytes = axum::body::to_bytes(response.into_body(), 8 * 1024 * 1024)
        .await
        .expect("a body");
    let value = serde_json::from_slice(&bytes).unwrap_or(Value::Null);
    (status, value)
}

async fn text(router: &Router, path: &str) -> String {
    let response = router
        .clone()
        .oneshot(
            Request::builder()
                .uri(path)
                .body(Body::empty())
                .expect("a request"),
        )
        .await
        .expect("a response");
    let bytes = axum::body::to_bytes(response.into_body(), 8 * 1024 * 1024)
        .await
        .expect("a body");
    String::from_utf8_lossy(&bytes).into_owned()
}

#[tokio::test]
async fn an_anonymous_caller_is_told_to_sign_in() {
    // 401 rather than 404: a route that exists and refused has to be
    // distinguishable from one that was never mounted, which is the absence
    // RFC 1403 exists to make visible.
    let state = state_with(Tier::Pro);
    let router = routes::router(state);
    for (method, path) in [
        ("GET", ORG),
        ("GET", "/_liyasa/api/v1/org/audit"),
        ("PATCH", ORG),
        ("POST", "/_liyasa/api/v1/org/projects"),
        ("DELETE", ORG),
        ("GET", "/_liyasa/api/v1/org/export"),
    ] {
        let (status, body) = send(&router, method, path, None).await;
        assert_eq!(status, StatusCode::UNAUTHORIZED, "{method} {path}");
        assert!(
            body["detail"]
                .as_str()
                .is_some_and(|d| d.contains("session")),
            "{method} {path}: {body}"
        );
    }
}

#[tokio::test]
async fn the_subtree_does_not_answer_for_a_path_it_does_not_own() {
    // `mount::guarded` uses `Router::layer`, which wraps the router's fallback
    // as well as its routes — so the subtree on its own answers 401 to
    // everything, including paths nobody registered. That is only safe
    // because `routes::router` sets a custom fallback and `merge` keeps it,
    // and "only safe because of something in another file" is worth an
    // assertion rather than a comment.
    let state = state_with(Tier::Pro);
    let merged = Router::new()
        .route("/open", axum::routing::get(|| async { "open" }))
        .fallback(|| async { (StatusCode::NOT_FOUND, "the site's own 404") })
        .merge(routes::router(state));

    let (status, _) = send(&merged, "GET", "/_liyasa/api/v1/org/nothing", None).await;
    assert_eq!(
        status,
        StatusCode::NOT_FOUND,
        "the org subtree swallowed a stray path"
    );
    let (status, _) = send(&merged, "GET", "/nowhere", None).await;
    assert_eq!(status, StatusCode::NOT_FOUND);
    let (status, _) = send(&merged, "GET", "/open", None).await;
    assert_eq!(status, StatusCode::OK);
    let (status, _) = send(&merged, "GET", ORG, None).await;
    assert_eq!(status, StatusCode::UNAUTHORIZED, "and still guards its own");
}

#[tokio::test]
async fn a_viewer_reads_and_cannot_write() {
    // The permission table in `routes::TABLE`, exercised through the seam's
    // own guard rather than through a check written for the test.
    let state = state_with(Tier::Pro);
    let viewer = as_role(&state, Role::Viewer);
    let (status, body) = send(&viewer, "GET", ORG, None).await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(body["plan"]["tier"], "pro");

    let (status, body) = send(
        &viewer,
        "POST",
        "/_liyasa/api/v1/org/projects",
        Some(json!({ "slug": "docs" })),
    )
    .await;
    assert_eq!(status, StatusCode::FORBIDDEN);
    assert!(
        body["detail"]
            .as_str()
            .is_some_and(|d| d.contains("SettingsWrite")),
        "{body}"
    );

    // An admin may change settings and still not delete the workspace.
    let admin = as_role(&state, Role::Admin);
    let (status, _) = send(&admin, "PATCH", ORG, Some(json!({ "name": "Acme docs" }))).await;
    assert_eq!(status, StatusCode::OK);
    let (status, _) = send(&admin, "DELETE", ORG, None).await;
    assert_eq!(status, StatusCode::FORBIDDEN);
}

#[tokio::test]
async fn a_project_is_created_with_a_region_and_a_subdomain() {
    // ORG-02, HOST-10, HOST-11.
    let state = state_with(Tier::Pro);
    let owner = as_role(&state, Role::Owner);
    let (status, body) = send(
        &owner,
        "POST",
        "/_liyasa/api/v1/org/projects",
        Some(json!({ "slug": "acme-docs", "title": "Acme Docs", "region": "eu" })),
    )
    .await;
    assert_eq!(status, StatusCode::CREATED, "{body}");
    assert_eq!(body["subdomain"], "acme-docs.liyasa.site");
    assert_eq!(body["region"], "eu");

    let (_, listed) = send(&owner, "GET", "/_liyasa/api/v1/org/projects", None).await;
    assert_eq!(listed["projects"][0]["slug"], "acme-docs");

    let (status, body) = send(
        &owner,
        "POST",
        "/_liyasa/api/v1/org/projects",
        Some(json!({ "slug": "Not A Label" })),
    )
    .await;
    assert_eq!(status, StatusCode::BAD_REQUEST);
    assert_eq!(body["code"], "E0857");
}

#[tokio::test]
async fn the_free_plan_refuses_the_second_project_with_a_payment_required() {
    // ORG-30. A quota refusal is not a malformed request: the same body
    // succeeds after an upgrade, so it is 402 rather than 400.
    let state = state_with(Tier::Free);
    let owner = as_role(&state, Role::Owner);
    let (status, _) = send(
        &owner,
        "POST",
        "/_liyasa/api/v1/org/projects",
        Some(json!({ "slug": "docs" })),
    )
    .await;
    assert_eq!(status, StatusCode::CREATED);

    let (status, body) = send(
        &owner,
        "POST",
        "/_liyasa/api/v1/org/projects",
        Some(json!({ "slug": "guides" })),
    )
    .await;
    assert_eq!(status, StatusCode::PAYMENT_REQUIRED);
    assert_eq!(body["code"], "E0856");
    assert!(body["help"].as_str().is_some(), "{body}");

    let (status, _) = send(
        &owner,
        "PUT",
        "/_liyasa/api/v1/org/plan",
        Some(json!({ "tier": "pro" })),
    )
    .await;
    assert_eq!(status, StatusCode::OK);
    let (status, _) = send(
        &owner,
        "POST",
        "/_liyasa/api/v1/org/projects",
        Some(json!({ "slug": "guides" })),
    )
    .await;
    assert_eq!(
        status,
        StatusCode::CREATED,
        "the same request after an upgrade"
    );
}

#[tokio::test]
async fn every_state_change_leaves_an_audit_entry() {
    // ORG-20 says "every state change". This walks one route from each area
    // the organization API owns and asserts the log grew each time, rather
    // than asserting it grew once.
    let state = state_with(Tier::Pro);
    let owner = as_role(&state, Role::Owner);
    let changes: Vec<(&str, &str, Option<Value>)> = vec![
        ("PATCH", ORG, Some(json!({ "name": "Acme docs" }))),
        (
            "POST",
            "/_liyasa/api/v1/org/projects",
            Some(json!({ "slug": "docs" })),
        ),
        (
            "POST",
            "/_liyasa/api/v1/org/invites",
            Some(json!({ "role": "editor", "email": "bo@acme.com" })),
        ),
        (
            "POST",
            "/_liyasa/api/v1/org/credentials",
            Some(json!({ "label": "ci", "owner": "u1" })),
        ),
        (
            "PUT",
            "/_liyasa/api/v1/org/credits/overage",
            Some(json!({ "enabled": true })),
        ),
        (
            "POST",
            "/_liyasa/api/v1/org/credits/top-up",
            Some(json!({ "credits": 10 })),
        ),
        ("DELETE", ORG, None),
        ("POST", "/_liyasa/api/v1/org/restore", None),
    ];
    let mut seen = 0usize;
    for (method, path, body) in changes {
        let (status, response) = send(&owner, method, path, body).await;
        assert!(status.is_success(), "{method} {path}: {status} {response}");
        let (_, log) = send(&owner, "GET", "/_liyasa/api/v1/org/audit", None).await;
        let entries = log["entries"].as_array().expect("entries").len();
        assert!(
            entries > seen,
            "{method} {path} changed something and wrote no audit entry"
        );
        seen = entries;
    }
}

#[tokio::test]
async fn an_audit_entry_carries_who_what_and_from_where() {
    // ORG-20 names six things. The address is the one a handler cannot make
    // up, so it is asserted against the connection rather than against a
    // header the caller chose.
    let state = state_with(Tier::Pro);
    let owner = as_role(&state, Role::Owner);
    let request = Request::builder()
        .method("PATCH")
        .uri(ORG)
        .header("content-type", "application/json")
        .header("user-agent", "liyasa-cli/0.1")
        .header("x-forwarded-for", "9.9.9.9")
        .body(Body::from(json!({ "name": "Acme docs" }).to_string()))
        .expect("a request");
    let response = owner.clone().oneshot(request).await.expect("a response");
    assert_eq!(response.status(), StatusCode::OK);

    let (_, log) = send(&owner, "GET", "/_liyasa/api/v1/org/audit", None).await;
    let entry = &log["entries"][0];
    assert_eq!(entry["action"], "settings.update");
    assert_eq!(entry["actor"]["id"], "u1");
    assert_eq!(entry["actor"]["kind"], "user");
    assert_eq!(entry["before"]["name"], "Acme");
    assert_eq!(entry["after"]["name"], "Acme docs");
    assert_ne!(
        entry["origin"]["ip"], "9.9.9.9",
        "a forwarded header from an untrusted peer must not decide what the audit log records"
    );
    assert_eq!(entry["origin"]["user_agent"], "liyasa-cli/0.1");
}

#[tokio::test]
async fn the_audit_log_is_searchable_and_exports_as_csv_and_json() {
    // ORG-20's export clause, from the REST API.
    let state = state_with(Tier::Pro);
    let owner = as_role(&state, Role::Owner);
    send(&owner, "PATCH", ORG, Some(json!({ "name": "Acme docs" }))).await;
    send(
        &owner,
        "POST",
        "/_liyasa/api/v1/org/credentials",
        Some(json!({ "label": "ci", "owner": "u1" })),
    )
    .await;

    let (_, all) = send(&owner, "GET", "/_liyasa/api/v1/org/audit", None).await;
    assert_eq!(all["entries"].as_array().expect("entries").len(), 2);
    assert_eq!(all["complete"], true);

    let (_, keys) = send(
        &owner,
        "GET",
        &format!("/_liyasa/api/v1/org/audit?area={}", Area::Keys.as_str()),
        None,
    )
    .await;
    assert_eq!(keys["entries"].as_array().expect("entries").len(), 1);
    assert_eq!(keys["entries"][0]["action"], "key.issue");

    let (_, searched) = send(&owner, "GET", "/_liyasa/api/v1/org/audit?q=settings", None).await;
    assert_eq!(searched["entries"].as_array().expect("entries").len(), 1);

    let csv = text(&owner, "/_liyasa/api/v1/org/audit?format=csv").await;
    assert!(
        csv.starts_with("id,at,actor,actorKind,area,action,object,project,origin,before,after")
    );
    assert_eq!(csv.lines().count(), 3, "a header and two rows:\n{csv}");

    let (status, _) = send(
        &owner,
        "GET",
        "/_liyasa/api/v1/org/audit?area=nonsense",
        None,
    )
    .await;
    assert_eq!(status, StatusCode::BAD_REQUEST);
}

#[tokio::test]
async fn credits_are_spent_refused_and_bypassed_by_byok() {
    // ORG-31, HOST-13.
    let state = state_with(Tier::Free);
    let owner = as_role(&state, Role::Owner);
    let (status, body) = send(
        &owner,
        "POST",
        "/_liyasa/api/v1/org/credits/charge",
        Some(json!({ "spend": "assistantAnswer", "project": "docs" })),
    )
    .await;
    assert_eq!(status, StatusCode::OK, "{body}");
    assert_eq!(body["charge"]["credits"], 1);
    assert_eq!(body["balance"], 49);

    // Free carries fifty credits, so the largest agent task will not fit.
    let (status, body) = send(
        &owner,
        "POST",
        "/_liyasa/api/v1/org/credits/charge",
        Some(json!({ "spend": "agentTask", "size": "maximum", "project": "docs" })),
    )
    .await;
    assert_eq!(status, StatusCode::PAYMENT_REQUIRED);
    assert_eq!(body["code"], "E0852");
    assert_eq!(body["needed"], 320);

    // The same task under a key of your own costs nothing.
    let (status, body) = send(
        &owner,
        "POST",
        "/_liyasa/api/v1/org/credits/charge",
        Some(json!({ "spend": "agentTask", "size": "maximum", "access": "byok" })),
    )
    .await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(body["charge"]["outcome"], "bypassed");

    let (_, usage) = send(&owner, "GET", "/_liyasa/api/v1/org/usage", None).await;
    assert_eq!(usage["credits"]["spent"], 1);
    assert_eq!(usage["credits"]["byProject"][0]["project"], "docs");
}

#[tokio::test]
async fn a_top_up_unblocks_a_task_the_pool_would_not_cover() {
    let state = state_with(Tier::Free);
    let owner = as_role(&state, Role::Owner);
    let (status, _) = send(
        &owner,
        "POST",
        "/_liyasa/api/v1/org/credits/top-up",
        Some(json!({ "credits": 500 })),
    )
    .await;
    assert_eq!(status, StatusCode::OK);
    let (status, body) = send(
        &owner,
        "POST",
        "/_liyasa/api/v1/org/credits/charge",
        Some(json!({ "spend": "agentTask", "size": "maximum", "project": "docs" })),
    )
    .await;
    assert_eq!(status, StatusCode::OK, "{body}");
    assert_eq!(body["charge"]["credits"], 320);
}

#[tokio::test]
async fn the_overview_publishes_the_quota_and_the_cost_basis_of_every_resource() {
    // ORG-33: "every metered resource has a cost basis and a plan quota", and
    // the operator team reads both from the same document.
    let state = state_with(Tier::Free);
    let owner = as_role(&state, Role::Owner);
    let (_, body) = send(&owner, "GET", ORG, None).await;
    let quotas = body["plan"]["quotas"].as_array().expect("quotas");
    assert_eq!(quotas.len(), 8);
    for quota in quotas {
        assert!(quota["unit"].as_str().is_some(), "{quota}");
        assert!(quota["microsPerUnit"].as_u64().is_some(), "{quota}");
    }
    assert_eq!(body["seats"]["limit"], 3);
    assert_eq!(body["plan"]["analyticsRetentionDays"], 90);
}

#[tokio::test]
async fn an_oss_instance_has_every_feature_and_no_quota() {
    // ORG-32, through the routed product rather than through the table.
    let state = state_with(Tier::Unlimited);
    let owner = as_role(&state, Role::Owner);
    let (_, body) = send(&owner, "GET", ORG, None).await;
    assert_eq!(body["plan"]["tier"], "unlimited");
    assert_eq!(body["seats"]["limit"], Value::Null);
    assert_eq!(body["plan"]["monthlyCredits"], Value::Null);
    for quota in body["plan"]["quotas"].as_array().expect("quotas") {
        assert_eq!(quota["limit"], Value::Null, "{quota}");
    }
    let (status, body) = send(
        &owner,
        "POST",
        "/_liyasa/api/v1/org/credits/charge",
        Some(json!({ "spend": "agentTask", "size": "maximum" })),
    )
    .await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(body["charge"]["outcome"], "bypassed");
}

#[tokio::test]
async fn an_invitation_holds_a_seat_until_it_is_accepted_or_revoked() {
    // ORG-01.
    let state = state_with(Tier::Free);
    let owner = as_role(&state, Role::Owner);
    let (status, invite) = send(
        &owner,
        "POST",
        "/_liyasa/api/v1/org/invites",
        Some(json!({ "role": "editor", "email": "bo@acme.com" })),
    )
    .await;
    assert_eq!(status, StatusCode::CREATED);
    let id = invite["id"].as_str().expect("an id").to_owned();

    let (_, overview) = send(&owner, "GET", ORG, None).await;
    assert_eq!(overview["seats"]["taken"], 1);

    let (status, member) = send(
        &owner,
        "POST",
        &format!("/_liyasa/api/v1/org/invites/{id}/accept"),
        Some(json!({ "user": "u2", "email": "bo@acme.com" })),
    )
    .await;
    assert_eq!(status, StatusCode::OK, "{member}");
    assert_eq!(member["role"], "editor");
    let (_, overview) = send(&owner, "GET", ORG, None).await;
    assert_eq!(
        overview["seats"]["taken"], 1,
        "the held seat became the member"
    );

    let (status, _) = send(
        &owner,
        "POST",
        &format!("/_liyasa/api/v1/org/invites/{id}/accept"),
        Some(json!({ "user": "u3", "email": "cy@acme.com" })),
    )
    .await;
    assert_eq!(
        status,
        StatusCode::BAD_REQUEST,
        "an invitation is single use"
    );
}

#[tokio::test]
async fn a_project_role_overrides_the_organization_role() {
    // ORG-02, over the wire.
    let state = state_with(Tier::Pro);
    let owner = as_role(&state, Role::Owner);
    send(
        &owner,
        "POST",
        "/_liyasa/api/v1/org/projects",
        Some(json!({ "slug": "secret" })),
    )
    .await;
    state::OrgState::write(&state)
        .org
        .add_member(Member::new("u2", "bo@acme.com", Grant::role(Role::Editor)))
        .expect("a seat");

    let (status, body) = send(
        &owner,
        "PATCH",
        "/_liyasa/api/v1/org/members/u2",
        Some(json!({ "role": "viewer", "project": "secret" })),
    )
    .await;
    assert_eq!(status, StatusCode::OK, "{body}");
    assert_eq!(body["role"], "editor", "the organization role is untouched");
    assert_eq!(body["projectRole"], "viewer");

    let (_, members) = send(&owner, "GET", "/_liyasa/api/v1/org/members", None).await;
    assert_eq!(
        members["members"][0]["projectRoles"][0]["project"],
        "secret"
    );
}

#[tokio::test]
async fn deleting_the_workspace_waits_and_can_be_undone_and_exports_meanwhile() {
    // ORG-03.
    let state = state_with(Tier::Pro);
    let owner = as_role(&state, Role::Owner);
    send(
        &owner,
        "POST",
        "/_liyasa/api/v1/org/projects",
        Some(json!({ "slug": "docs" })),
    )
    .await;

    let (status, deletion) = send(&owner, "DELETE", ORG, None).await;
    assert_eq!(status, StatusCode::ACCEPTED);
    assert!(
        deletion["effective_ms"].as_i64().expect("an instant")
            > deletion["requested_ms"].as_i64().expect("an instant")
    );

    let (status, export) = send(&owner, "GET", "/_liyasa/api/v1/org/export", None).await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(export["projects"][0]["slug"], "docs", "still exportable");
    assert!(export["audit"]["entries"].as_array().is_some());

    let (status, _) = send(&owner, "POST", "/_liyasa/api/v1/org/restore", None).await;
    assert_eq!(status, StatusCode::OK);
    let (_, overview) = send(&owner, "GET", ORG, None).await;
    assert_eq!(overview["deletion"], Value::Null);
}

#[tokio::test]
async fn a_notification_channel_with_no_endpoint_is_reported_rather_than_dropped() {
    // ORG-21. `announce` is what another package calls; the endpoints and the
    // preferences come from the routed API.
    use liyasa_server::org::notify::{Event, Notification};

    let state = state_with(Tier::Pro);
    let owner = as_role(&state, Role::Owner);
    routes::add_member(
        &state,
        Member::new("u2", "bo@acme.com", Grant::role(Role::Editor)),
    )
    .expect("a seat");

    let (status, _) = send(
        &owner,
        "PUT",
        "/_liyasa/api/v1/org/members/u2/notifications",
        Some(json!({
            "preferences": [
                { "event": "deployment", "channels": ["email", "teams"] }
            ]
        })),
    )
    .await;
    assert_eq!(status, StatusCode::OK);

    let routed = routes::announce(&state, &Notification::new(Event::Deployment, "deployed"));
    assert_eq!(routed.deliveries.len(), 1);
    assert_eq!(routed.deliveries[0].target, "bo@acme.com");
    assert_eq!(routed.skipped.len(), 1, "{routed:?}");

    let (status, _) = send(
        &owner,
        "PUT",
        "/_liyasa/api/v1/org/notifications/endpoints",
        Some(
            json!({ "endpoints": [{ "channel": "teams", "target": "https://example.com/hook" }] }),
        ),
    )
    .await;
    assert_eq!(status, StatusCode::OK);
    let routed = routes::announce(&state, &Notification::new(Event::Deployment, "deployed"));
    assert_eq!(routed.deliveries.len(), 2);
    assert!(routed.skipped.is_empty());
}

#[tokio::test]
async fn the_published_service_levels_are_served_with_their_runbooks() {
    // HOST-10. The status page and the alert rules read the same table, and
    // the SLA is readable without a credential: the people who read one are
    // deciding whether to buy.
    let state = state_with(Tier::Enterprise);
    let anonymous = routes::router(state.clone());
    let (status, body) = send(&anonymous, "GET", "/_liyasa/api/v1/org/slo", None).await;
    assert_eq!(status, StatusCode::OK, "the published SLA needs no session");
    let indicators = body["indicators"].as_array().expect("indicators");
    assert_eq!(indicators.len(), 6);
    for indicator in indicators {
        assert!(
            indicator["runbook"]
                .as_str()
                .is_some_and(|r| r.contains("runbooks")),
            "{indicator}"
        );
        assert!(indicator["recordingRule"].as_str().is_some(), "{indicator}");
    }
    assert_eq!(body["alerts"].as_array().expect("alerts").len(), 24);

    // And nothing else in the subtree opened up with it.
    let (status, _) = send(&anonymous, "GET", ORG, None).await;
    assert_eq!(status, StatusCode::UNAUTHORIZED);
    let (status, _) = send(&anonymous, "GET", "/_liyasa/api/v1/org/usage", None).await;
    assert_eq!(status, StatusCode::UNAUTHORIZED);
}

#[tokio::test]
async fn an_upgrade_lifts_the_pause_it_was_bought_to_lift() {
    // ORG-33 pauses a Free project's metered feature; ORG-30 sells the way
    // out. A plan endpoint that answers 200 while the feature stays paused is
    // the shape of defect this fleet has been finding all day.
    use liyasa_server::org::plan::Resource;

    let state = state_with(Tier::Free);
    let owner = as_role(&state, Role::Owner);
    state
        .write()
        .meter
        .request("docs", Resource::SandboxMinutes, 201);
    let (_, usage) = send(&owner, "GET", "/_liyasa/api/v1/org/usage", None).await;
    assert_eq!(usage["paused"][0], "sandboxMinutes");

    let (status, _) = send(
        &owner,
        "PUT",
        "/_liyasa/api/v1/org/plan",
        Some(json!({ "tier": "pro" })),
    )
    .await;
    assert_eq!(status, StatusCode::OK);
    let (_, usage) = send(&owner, "GET", "/_liyasa/api/v1/org/usage", None).await;
    assert!(
        usage["paused"].as_array().expect("paused").is_empty(),
        "{usage}"
    );
}

#[tokio::test]
async fn a_region_is_chosen_at_creation_and_the_workspace_default_applies() {
    // HOST-11.
    let state = state_with(Tier::Pro);
    let owner = as_role(&state, Role::Owner);
    let (status, _) = send(
        &owner,
        "PATCH",
        ORG,
        Some(json!({ "defaultRegion": Region::Apac.as_str() })),
    )
    .await;
    assert_eq!(status, StatusCode::OK);
    let (_, created) = send(
        &owner,
        "POST",
        "/_liyasa/api/v1/org/projects",
        Some(json!({ "slug": "docs" })),
    )
    .await;
    assert_eq!(created["region"], "apac");

    let (status, body) = send(
        &owner,
        "PATCH",
        ORG,
        Some(json!({ "defaultRegion": "moon" })),
    )
    .await;
    assert_eq!(status, StatusCode::BAD_REQUEST, "{body}");
}
