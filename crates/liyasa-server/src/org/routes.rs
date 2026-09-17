//! The organization REST surface (ORG-01..03, ORG-20, ORG-21, ORG-30..33).
//!
//! Three routers rather than one, because reading the audit log, changing a
//! member's role and deleting the workspace are three different answers to
//! "who may". They are composed here and registered as one subtree rather
//! than as three, because all three read and write one [`OrgState`] and
//! RFC 1403's `mount` is a bare function pointer with nowhere to keep it —
//! three registrations would build three organizations. [`TABLE`] carries the
//! permissions instead, where they are still a list someone can read and
//! question.
//!
//! Every route that changes something writes an audit entry before it answers
//! (ORG-20). The actor on that entry is whoever the request was authenticated
//! as, and `System` when nothing authenticated it — which is every request
//! today, because nothing yet inserts a `Principal` into the request
//! extensions (RFC 1403 says so, and it is WP-15's layer to write). The entry
//! records what the server knows rather than a name it made up, and it starts
//! carrying real actors on the day that layer lands, with no change here.

use std::collections::BTreeMap;
use std::net::{IpAddr, Ipv4Addr, SocketAddr};
use std::sync::Arc;
use std::time::Duration;

use axum::Router;
use axum::extract::{ConnectInfo, Path, Request, State};
use axum::response::{IntoResponse, Response};
use axum::routing::{delete, get, patch, post, put};
use http::StatusCode;
use http::request::Parts;
use liyasa_core::diagnostics::Diagnostic;
use serde_json::{Value, json};

use crate::auth::roles::{Grant, Permission, Role};
use crate::auth::session::Principal;
use crate::routes::mount::guarded;
use crate::routes::problem::Problem;

use super::audit::{self, Actor, ActorKind, Area, Origin, Record};
use super::credits::{Access, Spend, TaskSize};
use super::model::{CredentialKind, InviteKind, Member, Settings};
use super::notify::{self, Channel, Endpoints, Event, Preferences, Subscriber};
use super::plan::{Plan, Resource, Tier};
use super::region::Region;
use super::state::OrgState;

const PREFIX: &str = "/_liyasa/api/v1/org";
/// Bodies here are settings objects, not documents.
const MAX_BODY: usize = 64 * 1024;
const DEFAULT_INVITE_TTL: Duration = Duration::from_secs(7 * 24 * 60 * 60);

/// What each group of routes requires. The seam applies one permission to a
/// whole subtree, so a subtree with three answers to "who may" applies them
/// here, with `routes::mount::guarded` — the same function `application` would
/// have used — rather than with a check of its own.
/// `None` is a surface that is deliberately public. HOST-10 publishes the
/// service levels in the SLA, and the people who read an SLA are deciding
/// whether to buy: behind `DashboardRead` it would be unreadable by exactly
/// the audience it exists for. Same shape as ANA-02's event schema in
/// RFC 1403, and it is one line here so someone can see it and argue with it.
pub struct Group {
    /// `None` is public.
    pub permission: Option<Permission>,
    pub build: fn(Arc<OrgState>) -> Router,
}

pub const TABLE: &[Group] = &[
    Group {
        permission: None,
        build: public_router,
    },
    Group {
        permission: Some(Permission::DashboardRead),
        build: read_router,
    },
    Group {
        permission: Some(Permission::SettingsWrite),
        build: settings_router,
    },
    Group {
        permission: Some(Permission::OwnerAct),
        build: owner_router,
    },
];

/// The whole organization subtree, guarded.
pub fn router(state: Arc<OrgState>) -> Router {
    TABLE.iter().fold(Router::new(), |router, group| {
        let routes = (group.build)(state.clone());
        router.merge(match group.permission {
            Some(permission) => guarded(routes, permission),
            None => routes,
        })
    })
}

/// HOST-10's published objectives. No credential: this is the SLA.
pub fn public_router(state: Arc<OrgState>) -> Router {
    Router::new()
        .route(&format!("{PREFIX}/slo"), get(slo))
        .with_state(state)
}

/// Everything a dashboard reads.
pub fn read_router(state: Arc<OrgState>) -> Router {
    Router::new()
        .route(PREFIX, get(overview))
        .route(&format!("{PREFIX}/projects"), get(list_projects))
        .route(&format!("{PREFIX}/members"), get(list_members))
        .route(&format!("{PREFIX}/invites"), get(list_invites))
        .route(&format!("{PREFIX}/credentials"), get(list_credentials))
        .route(&format!("{PREFIX}/audit"), get(read_audit))
        .route(&format!("{PREFIX}/usage"), get(usage))
        .route(&format!("{PREFIX}/notifications"), get(read_notifications))
        .with_state(state)
}

/// Everything an administrator changes.
pub fn settings_router(state: Arc<OrgState>) -> Router {
    Router::new()
        .route(PREFIX, patch(update_settings))
        .route(&format!("{PREFIX}/projects"), post(create_project))
        .route(
            &format!("{PREFIX}/projects/{{slug}}"),
            delete(remove_project),
        )
        .route(&format!("{PREFIX}/invites"), post(create_invite))
        .route(&format!("{PREFIX}/invites/{{id}}"), delete(revoke_invite))
        .route(
            &format!("{PREFIX}/invites/{{id}}/accept"),
            post(accept_invite),
        )
        .route(&format!("{PREFIX}/members/{{user}}"), patch(set_role))
        .route(&format!("{PREFIX}/members/{{user}}"), delete(remove_member))
        .route(
            &format!("{PREFIX}/members/{{user}}/notifications"),
            put(set_notifications),
        )
        .route(&format!("{PREFIX}/credentials"), post(issue_credential))
        .route(
            &format!("{PREFIX}/credentials/{{id}}"),
            delete(revoke_credential),
        )
        .route(
            &format!("{PREFIX}/notifications/endpoints"),
            put(set_endpoints),
        )
        .with_state(state)
}

/// Billing, the plan, and destroying the workspace.
pub fn owner_router(state: Arc<OrgState>) -> Router {
    Router::new()
        .route(&format!("{PREFIX}/plan"), put(set_plan))
        .route(&format!("{PREFIX}/credits/top-up"), post(top_up))
        .route(&format!("{PREFIX}/credits/overage"), put(set_overage))
        .route(&format!("{PREFIX}/credits/charge"), post(charge))
        .route(PREFIX, delete(request_deletion))
        .route(&format!("{PREFIX}/restore"), post(cancel_deletion))
        .route(&format!("{PREFIX}/export"), get(export))
        .with_state(state)
}

// ---- shared plumbing ----

fn problem(diagnostic: &Diagnostic, status: StatusCode) -> Problem {
    let mut problem = Problem::code(status, diagnostic.code).detail(diagnostic.message.clone());
    if let Some(help) = &diagnostic.help {
        problem = problem.extension("help", json!(help));
    }
    problem
}

/// A refusal the plan made is a `402`, not a `400`: nothing about the request
/// is malformed and retrying it unchanged after an upgrade is correct.
fn plan_problem(diagnostic: &Diagnostic) -> Problem {
    problem(diagnostic, StatusCode::PAYMENT_REQUIRED)
}

fn bad_request(diagnostic: &Diagnostic) -> Problem {
    problem(diagnostic, StatusCode::BAD_REQUEST)
}

/// ORG-20's "who". `System` when nothing authenticated the request, which is
/// honest rather than inventing a name.
fn actor_of(parts: &Parts) -> Actor {
    match parts.extensions.get::<Principal>() {
        Some(principal) => Actor {
            id: principal.subject.clone(),
            kind: ActorKind::User,
            email: principal
                .data
                .get("email")
                .and_then(Value::as_str)
                .map(str::to_owned),
        },
        None => Actor::system(),
    }
}

/// ORG-20's "from where". The address comes through `AppState::client_ip`, so
/// a forwarded header is believed only from a trusted proxy — a client that
/// sets `X-Forwarded-For` itself cannot choose what the audit log records.
fn origin_of(state: &OrgState, parts: &Parts) -> Origin {
    let ip = match state.app() {
        Some(app) => app.client_ip(parts),
        None => parts
            .extensions
            .get::<ConnectInfo<SocketAddr>>()
            .map(|ConnectInfo(addr)| addr.ip())
            .unwrap_or(IpAddr::V4(Ipv4Addr::LOCALHOST)),
    };
    let mut origin = Origin::from(ip);
    if let Some(agent) = parts
        .headers
        .get(http::header::USER_AGENT)
        .and_then(|value| value.to_str().ok())
    {
        origin = origin.with_user_agent(agent);
    }
    origin
}

fn query_of(parts: &Parts) -> BTreeMap<String, String> {
    url::form_urlencoded::parse(parts.uri.query().unwrap_or_default().as_bytes())
        .map(|(k, v)| (k.into_owned(), v.into_owned()))
        .collect()
}

async fn split(request: Request) -> Result<(Parts, Value), Problem> {
    let (parts, body) = request.into_parts();
    let bytes = axum::body::to_bytes(body, MAX_BODY)
        .await
        .map_err(|_| Problem::too_large(MAX_BODY as u64))?;
    if bytes.is_empty() {
        return Ok((parts, Value::Null));
    }
    let value = serde_json::from_slice(&bytes)
        .map_err(|error| Problem::bad_request(format!("the body is not JSON: {error}")))?;
    Ok((parts, value))
}

fn field<'a>(body: &'a Value, name: &str) -> Option<&'a str> {
    body.get(name).and_then(Value::as_str)
}

fn require<'a>(body: &'a Value, name: &str) -> Result<&'a str, Problem> {
    field(body, name).ok_or_else(|| Problem::bad_request(format!("`{name}` is required")))
}

fn grant_from(body: &Value) -> Result<Grant, Problem> {
    let role = require(body, "role")?;
    let role =
        Role::parse(role).ok_or_else(|| Problem::bad_request(format!("`{role}` is not a role")))?;
    Ok(Grant::role(role))
}

// ---- reads ----

async fn overview(State(state): State<Arc<OrgState>>) -> Response {
    let inner = state.read();
    let plan = &inner.org.plan;
    axum::Json(json!({
        "id": inner.org.id,
        "settings": inner.org.settings,
        "plan": {
            "tier": plan.tier.as_str(),
            "overage": plan.overage,
            "analyticsRetentionDays": plan.analytics_retention_days,
            "monthlyCredits": plan.monthly_credits,
            "previewLifetimeDays": plan.preview_lifetime_days,
            "features": plan.features().map(|f| f.as_str()).collect::<Vec<_>>(),
            "quotas": plan
                .quotas()
                .map(|(resource, limit)| json!({
                    "resource": resource.as_str(),
                    "limit": limit,
                    "unit": resource.cost_basis().unit,
                    "microsPerUnit": resource.cost_basis().micros_per_unit,
                }))
                .collect::<Vec<_>>(),
        },
        "seats": {
            "taken": inner.org.seats_taken(),
            "limit": plan.quota(Resource::Seats),
        },
        "projects": inner.org.projects().count(),
        "deletion": inner.org.deletion(),
    }))
    .into_response()
}

async fn list_projects(State(state): State<Arc<OrgState>>) -> Response {
    let inner = state.read();
    let projects: Vec<Value> = inner
        .org
        .projects()
        .map(|project| {
            json!({
                "slug": project.slug,
                "title": project.title,
                "region": project.region,
                "subdomain": project.subdomain(),
                "createdAt": project.created_ms,
            })
        })
        .collect();
    axum::Json(json!({ "projects": projects })).into_response()
}

async fn list_members(State(state): State<Arc<OrgState>>) -> Response {
    let inner = state.read();
    let members: Vec<Value> = inner
        .org
        .members()
        .map(|member| {
            json!({
                "user": member.user,
                "email": member.email,
                "role": member.grant.role,
                "projectRoles": member
                    .overrides()
                    .map(|(project, grant)| json!({ "project": project, "role": grant.role }))
                    .collect::<Vec<_>>(),
            })
        })
        .collect();
    axum::Json(json!({ "members": members })).into_response()
}

async fn list_invites(State(state): State<Arc<OrgState>>) -> Response {
    let inner = state.read();
    axum::Json(json!({ "invites": inner.org.invites().collect::<Vec<_>>() })).into_response()
}

async fn list_credentials(State(state): State<Arc<OrgState>>) -> Response {
    let inner = state.read();
    axum::Json(json!({ "credentials": inner.org.credentials().collect::<Vec<_>>() }))
        .into_response()
}

/// ORG-20: searchable, and exportable to CSV and JSON from the same endpoint
/// the dashboard and the CLI read.
async fn read_audit(State(state): State<Arc<OrgState>>, request: Request) -> Response {
    let (parts, _) = request.into_parts();
    let params = query_of(&parts);
    let mut query = audit::Query {
        actor: params.get("actor").cloned(),
        project: params.get("project").cloned(),
        object: params.get("object").cloned(),
        text: params.get("q").cloned(),
        ..audit::Query::default()
    };
    if let Some(area) = params.get("area") {
        match Area::parse(area) {
            Some(area) => query.area = Some(area),
            None => {
                return Problem::bad_request(format!("`{area}` is not an audit area"))
                    .into_response();
            }
        }
    }
    for (name, slot) in [
        ("since", &mut query.since_ms),
        ("until", &mut query.until_ms),
    ] {
        if let Some(raw) = params.get(name) {
            match raw.parse::<i64>() {
                Ok(value) => *slot = Some(value),
                Err(_) => {
                    return Problem::bad_request(format!(
                        "`{name}` is milliseconds since the epoch, not `{raw}`"
                    ))
                    .into_response();
                }
            }
        }
    }
    if let Some(raw) = params.get("limit") {
        match raw.parse::<usize>() {
            Ok(value) => query.limit = Some(value),
            Err(_) => {
                return Problem::bad_request(format!("`limit` is a number, not `{raw}`"))
                    .into_response();
            }
        }
    }

    let inner = state.read();
    match params.get("format").map(String::as_str) {
        Some("csv") => (
            [(http::header::CONTENT_TYPE, "text/csv; charset=utf-8")],
            inner.log.to_csv(&query),
        )
            .into_response(),
        None | Some("json") => axum::Json(inner.log.to_json(&query)).into_response(),
        Some(other) => {
            Problem::bad_request(format!("`{other}` is not a format; use `json` or `csv`"))
                .into_response()
        }
    }
}

/// ORG-31's usage dashboard and ORG-33's cost view in one document, because
/// they are two readings of the same consumption.
async fn usage(State(state): State<Arc<OrgState>>) -> Response {
    let inner = state.read();
    let projects: Vec<Value> = inner
        .meter
        .usage
        .projects()
        .into_iter()
        .map(|project| {
            json!({
                "project": project,
                "costMicros": inner.meter.usage.cost_micros_for(project),
            })
        })
        .collect();
    axum::Json(json!({
        "credits": {
            "pool": inner.ledger.pool,
            "carried": inner.ledger.carried,
            "toppedUp": inner.ledger.topped_up,
            "granted": inner.ledger.granted(),
            "spent": inner.ledger.spent(),
            "balance": inner.ledger.balance(),
            "percentUsed": inner.ledger.percent_used(),
            "overageEnabled": inner.ledger.overage_enabled,
            "overageSpent": inner.ledger.overage_spent,
            "bySurface": inner
                .ledger
                .by_surface()
                .map(|(surface, credits)| json!({ "surface": surface.as_str(), "credits": credits }))
                .collect::<Vec<_>>(),
            "byProject": inner
                .ledger
                .by_project()
                .map(|(project, credits)| json!({ "project": project, "credits": credits }))
                .collect::<Vec<_>>(),
        },
        "resources": inner
            .meter
            .usage
            .resources()
            .map(|(resource, units)| json!({
                "resource": resource.as_str(),
                "used": units,
                "limit": inner.org.plan.quota(resource),
                "unit": resource.cost_basis().unit,
                "costMicros": resource.cost_basis().micros_for(units),
                "paused": inner.meter.is_paused(resource),
            }))
            .collect::<Vec<_>>(),
        "paused": inner.meter.paused().map(|r| r.as_str()).collect::<Vec<_>>(),
        "costMicros": inner.meter.usage.cost_micros(),
        "costByProject": projects,
    }))
    .into_response()
}

/// HOST-10's published objectives, so the status page and the alert rules read
/// the same table.
async fn slo() -> Response {
    let indicators: Vec<Value> = super::slo::catalogue()
        .iter()
        .map(|sli| {
            json!({
                "key": sli.key,
                "title": sli.title,
                "measurement": sli.measurement,
                "objective": sli.objective.describe(),
                "recordingRule": sli.recording_rule,
                "runbook": sli.runbook,
                "errorBudgetSeconds": sli
                    .objective
                    .error_budget(super::slo::PERIOD)
                    .map(|budget| budget.as_secs()),
            })
        })
        .collect();
    axum::Json(json!({
        "period": "30d",
        "indicators": indicators,
        "alerts": super::slo::alerts(),
    }))
    .into_response()
}

async fn read_notifications(State(state): State<Arc<OrgState>>) -> Response {
    let inner = state.read();
    axum::Json(json!({
        "endpoints": inner
            .endpoints
            .configured()
            .map(|(channel, target)| json!({ "channel": channel.as_str(), "target": target }))
            .collect::<Vec<_>>(),
        "subscribers": inner.subscribers,
    }))
    .into_response()
}

// ---- settings ----

async fn update_settings(State(state): State<Arc<OrgState>>, request: Request) -> Response {
    let (parts, body) = match split(request).await {
        Ok(split) => split,
        Err(problem) => return problem.into_response(),
    };
    let actor = actor_of(&parts);
    let origin = origin_of(&state, &parts);

    let mut inner = state.write();
    let before = json!(inner.org.settings);
    let mut settings = inner.org.settings.clone();
    if let Some(name) = field(&body, "name") {
        settings.name = name.to_owned();
    }
    if let Some(icon) = body.get("icon") {
        settings.icon = icon.as_str().map(str::to_owned);
    }
    if let Some(region) = field(&body, "defaultRegion") {
        match Region::parse(region) {
            Some(region) => settings.default_region = region,
            None => {
                return Problem::bad_request(format!("`{region}` is not a region")).into_response();
            }
        }
    }
    inner.org.settings = settings;
    let after = json!(inner.org.settings);
    let at = state.clock().now_ms();
    inner.log.record(
        at,
        Record::new(actor, Area::Settings, "settings.update", "organization")
            .from(origin)
            .changing(before, after.clone()),
    );
    axum::Json(after).into_response()
}

async fn create_project(State(state): State<Arc<OrgState>>, request: Request) -> Response {
    let (parts, body) = match split(request).await {
        Ok(split) => split,
        Err(problem) => return problem.into_response(),
    };
    let slug = match require(&body, "slug") {
        Ok(slug) => slug.to_owned(),
        Err(problem) => return problem.into_response(),
    };
    let title = field(&body, "title").unwrap_or(&slug).to_owned();
    let region = match field(&body, "region") {
        Some(region) => match Region::parse(region) {
            Some(region) => Some(region),
            None => {
                return Problem::bad_request(format!("`{region}` is not a region")).into_response();
            }
        },
        None => None,
    };
    let actor = actor_of(&parts);
    let origin = origin_of(&state, &parts);

    let mut inner = state.write();
    let created = match inner.org.create_project(&slug, &title, region) {
        Ok(project) => json!({
            "slug": project.slug,
            "title": project.title,
            "region": project.region,
            "subdomain": project.subdomain(),
        }),
        Err(diagnostic) => {
            let status = match diagnostic.code.as_str() {
                "E0857" => StatusCode::BAD_REQUEST,
                _ => StatusCode::PAYMENT_REQUIRED,
            };
            return problem(&diagnostic, status).into_response();
        }
    };
    let at = state.clock().now_ms();
    inner.log.record(
        at,
        Record::new(actor, Area::Settings, "project.create", &slug)
            .in_project(&slug)
            .from(origin)
            .creating(created.clone()),
    );
    (StatusCode::CREATED, axum::Json(created)).into_response()
}

async fn remove_project(
    State(state): State<Arc<OrgState>>,
    Path(slug): Path<String>,
    request: Request,
) -> Response {
    let (parts, _) = request.into_parts();
    let actor = actor_of(&parts);
    let origin = origin_of(&state, &parts);

    let mut inner = state.write();
    let Some(project) = inner.org.delete_project(&slug) else {
        return Problem::not_found("project").into_response();
    };
    let at = state.clock().now_ms();
    inner.log.record(
        at,
        Record::new(actor, Area::Settings, "project.delete", &slug)
            .in_project(&slug)
            .from(origin)
            .removing(json!({ "slug": project.slug, "region": project.region })),
    );
    StatusCode::NO_CONTENT.into_response()
}

async fn create_invite(State(state): State<Arc<OrgState>>, request: Request) -> Response {
    let (parts, body) = match split(request).await {
        Ok(split) => split,
        Err(problem) => return problem.into_response(),
    };
    let grant = match grant_from(&body) {
        Ok(grant) => grant,
        Err(problem) => return problem.into_response(),
    };
    let kind = match field(&body, "email") {
        Some(address) => InviteKind::Email {
            address: address.to_owned(),
        },
        None => InviteKind::Link,
    };
    let ttl = body
        .get("validForSeconds")
        .and_then(Value::as_u64)
        .map(Duration::from_secs)
        .unwrap_or(DEFAULT_INVITE_TTL);
    let actor = actor_of(&parts);
    let origin = origin_of(&state, &parts);

    let mut inner = state.write();
    let invite = match inner.org.invite(kind, grant, ttl) {
        Ok(invite) => json!(invite),
        Err(diagnostic) => return plan_problem(&diagnostic).into_response(),
    };
    let id = invite["id"].as_str().unwrap_or_default().to_owned();
    let at = state.clock().now_ms();
    inner.log.record(
        at,
        Record::new(actor, Area::Members, "invite.create", &id)
            .from(origin)
            .creating(invite.clone()),
    );
    (StatusCode::CREATED, axum::Json(invite)).into_response()
}

async fn revoke_invite(
    State(state): State<Arc<OrgState>>,
    Path(id): Path<String>,
    request: Request,
) -> Response {
    let (parts, _) = request.into_parts();
    let actor = actor_of(&parts);
    let origin = origin_of(&state, &parts);

    let mut inner = state.write();
    if !inner.org.revoke_invite(&id) {
        return Problem::not_found("pending invitation").into_response();
    }
    let at = state.clock().now_ms();
    inner.log.record(
        at,
        Record::new(actor, Area::Members, "invite.revoke", &id)
            .from(origin)
            .removing(json!({ "id": id })),
    );
    StatusCode::NO_CONTENT.into_response()
}

async fn accept_invite(
    State(state): State<Arc<OrgState>>,
    Path(id): Path<String>,
    request: Request,
) -> Response {
    let (parts, body) = match split(request).await {
        Ok(split) => split,
        Err(problem) => return problem.into_response(),
    };
    let (user, email) = match (require(&body, "user"), require(&body, "email")) {
        (Ok(user), Ok(email)) => (user.to_owned(), email.to_owned()),
        (Err(problem), _) | (_, Err(problem)) => return problem.into_response(),
    };
    let actor = actor_of(&parts);
    let origin = origin_of(&state, &parts);

    let mut inner = state.write();
    let member = match inner.org.accept_invite(&id, &user, &email) {
        Ok(member) => {
            json!({ "user": member.user, "email": member.email, "role": member.grant.role })
        }
        Err(diagnostic) => return bad_request(&diagnostic).into_response(),
    };
    let at = state.clock().now_ms();
    inner.log.record(
        at,
        Record::new(actor, Area::Members, "invite.accept", &id)
            .from(origin)
            .creating(member.clone()),
    );
    axum::Json(member).into_response()
}

/// ORG-01's "change role" and ORG-02's project-level override, which is the
/// same operation with a project named.
async fn set_role(
    State(state): State<Arc<OrgState>>,
    Path(user): Path<String>,
    request: Request,
) -> Response {
    let (parts, body) = match split(request).await {
        Ok(split) => split,
        Err(problem) => return problem.into_response(),
    };
    let grant = match grant_from(&body) {
        Ok(grant) => grant,
        Err(problem) => return problem.into_response(),
    };
    let project = field(&body, "project").map(str::to_owned);
    let actor = actor_of(&parts);
    let origin = origin_of(&state, &parts);

    let mut inner = state.write();
    let Some(member) = inner.org.member(&user) else {
        return Problem::not_found("member").into_response();
    };
    let before = json!({
        "role": member.grant.role,
        "projectRole": project.as_deref().map(|p| member.grant_for(p).role),
    });
    let changed = match &project {
        Some(project) => inner.org.set_project_role(&user, project, grant),
        None => inner.org.set_role(&user, grant),
    };
    if !changed {
        return Problem::not_found("member").into_response();
    }
    let member = inner.org.member(&user).expect("just changed");
    let after = json!({
        "role": member.grant.role,
        "projectRole": project.as_deref().map(|p| member.grant_for(p).role),
    });
    let at = state.clock().now_ms();
    let mut record = Record::new(actor, Area::Members, "role.change", &user)
        .from(origin)
        .changing(before, after.clone());
    if let Some(project) = &project {
        record = record.in_project(project);
    }
    inner.log.record(at, record);
    axum::Json(after).into_response()
}

async fn remove_member(
    State(state): State<Arc<OrgState>>,
    Path(user): Path<String>,
    request: Request,
) -> Response {
    let (parts, _) = request.into_parts();
    let actor = actor_of(&parts);
    let origin = origin_of(&state, &parts);

    let mut inner = state.write();
    let Some(removed) = inner.org.remove_member(&user) else {
        return Problem::not_found("member").into_response();
    };
    let at = state.clock().now_ms();
    inner.log.record(
        at,
        Record::new(actor, Area::Members, "member.remove", &user)
            .from(origin)
            .removing(
                json!({ "user": removed.user, "email": removed.email, "role": removed.grant.role }),
            ),
    );
    StatusCode::NO_CONTENT.into_response()
}

async fn set_notifications(
    State(state): State<Arc<OrgState>>,
    Path(user): Path<String>,
    request: Request,
) -> Response {
    let (parts, body) = match split(request).await {
        Ok(split) => split,
        Err(problem) => return problem.into_response(),
    };
    let actor = actor_of(&parts);
    let origin = origin_of(&state, &parts);

    let mut preferences = Preferences::standard();
    if let Some(entries) = body.get("preferences").and_then(Value::as_array) {
        preferences = Preferences::default();
        for entry in entries {
            let Some(event) = field(entry, "event").and_then(Event::parse) else {
                return Problem::bad_request(format!(
                    "`{}` is not a notification event",
                    field(entry, "event").unwrap_or("(missing)")
                ))
                .into_response();
            };
            let mut channels = Vec::new();
            for raw in entry
                .get("channels")
                .and_then(Value::as_array)
                .unwrap_or(&Vec::new())
            {
                let Some(channel) = raw.as_str().and_then(Channel::parse) else {
                    return Problem::bad_request(format!("`{raw}` is not a channel"))
                        .into_response();
                };
                channels.push(channel);
            }
            match field(entry, "project") {
                Some(project) => preferences.set_for_project(project, event, channels),
                None => preferences.set(event, channels),
            }
        }
    }

    let mut inner = state.write();
    let Some(member) = inner.org.member(&user) else {
        return Problem::not_found("member").into_response();
    };
    let email = member.email.clone();
    let before = inner
        .subscribers
        .iter()
        .find(|s| s.user == user)
        .map(|s| json!(s.preferences))
        .unwrap_or(Value::Null);
    inner.subscribers.retain(|s| s.user != user);
    inner.subscribers.push(Subscriber {
        user: user.clone(),
        email,
        preferences: preferences.clone(),
    });
    let after = json!(preferences);
    let at = state.clock().now_ms();
    inner.log.record(
        at,
        Record::new(actor, Area::Settings, "notifications.update", &user)
            .from(origin)
            .changing(before, after.clone()),
    );
    axum::Json(after).into_response()
}

async fn set_endpoints(State(state): State<Arc<OrgState>>, request: Request) -> Response {
    let (parts, body) = match split(request).await {
        Ok(split) => split,
        Err(problem) => return problem.into_response(),
    };
    let actor = actor_of(&parts);
    let origin = origin_of(&state, &parts);

    let mut endpoints = Endpoints::default();
    for entry in body
        .get("endpoints")
        .and_then(Value::as_array)
        .unwrap_or(&Vec::new())
    {
        let Some(channel) = field(entry, "channel").and_then(Channel::parse) else {
            return Problem::bad_request(format!(
                "`{}` is not a channel",
                field(entry, "channel").unwrap_or("(missing)")
            ))
            .into_response();
        };
        let Some(target) = field(entry, "target") else {
            return Problem::bad_request("each endpoint needs a `target`").into_response();
        };
        if !channel.needs_endpoint() {
            return Problem::bad_request(
                "email goes to each member's own address and takes no endpoint",
            )
            .into_response();
        }
        endpoints.set(channel, target);
    }

    let mut inner = state.write();
    let before = json!(inner.endpoints);
    inner.endpoints = endpoints;
    let after = json!(inner.endpoints);
    let at = state.clock().now_ms();
    inner.log.record(
        at,
        Record::new(
            actor,
            Area::Settings,
            "notifications.endpoints",
            "organization",
        )
        .from(origin)
        .changing(before, after.clone()),
    );
    axum::Json(after).into_response()
}

async fn issue_credential(State(state): State<Arc<OrgState>>, request: Request) -> Response {
    let (parts, body) = match split(request).await {
        Ok(split) => split,
        Err(problem) => return problem.into_response(),
    };
    let label = match require(&body, "label") {
        Ok(label) => label.to_owned(),
        Err(problem) => return problem.into_response(),
    };
    let owner = match require(&body, "owner") {
        Ok(owner) => owner.to_owned(),
        Err(problem) => return problem.into_response(),
    };
    let kind = match field(&body, "kind").unwrap_or("apiKey") {
        "apiKey" => CredentialKind::ApiKey,
        "personalAccessToken" => CredentialKind::PersonalAccessToken,
        "deployKey" => CredentialKind::DeployKey,
        other => {
            return Problem::bad_request(format!("`{other}` is not a credential kind"))
                .into_response();
        }
    };
    let actor = actor_of(&parts);
    let origin = origin_of(&state, &parts);

    let mut inner = state.write();
    let credential = json!(inner.org.issue_credential(kind, &label, &owner));
    let id = credential["id"].as_str().unwrap_or_default().to_owned();
    let at = state.clock().now_ms();
    inner.log.record(
        at,
        Record::new(actor, Area::Keys, "key.issue", &id)
            .from(origin)
            .creating(credential.clone()),
    );
    (StatusCode::CREATED, axum::Json(credential)).into_response()
}

async fn revoke_credential(
    State(state): State<Arc<OrgState>>,
    Path(id): Path<String>,
    request: Request,
) -> Response {
    let (parts, _) = request.into_parts();
    let actor = actor_of(&parts);
    let origin = origin_of(&state, &parts);

    let mut inner = state.write();
    if !inner.org.revoke_credential(&id) {
        return Problem::not_found("live credential").into_response();
    }
    let at = state.clock().now_ms();
    inner.log.record(
        at,
        Record::new(actor, Area::Keys, "key.revoke", &id)
            .from(origin)
            .removing(json!({ "id": id })),
    );
    StatusCode::NO_CONTENT.into_response()
}

// ---- owner ----

async fn set_plan(State(state): State<Arc<OrgState>>, request: Request) -> Response {
    let (parts, body) = match split(request).await {
        Ok(split) => split,
        Err(problem) => return problem.into_response(),
    };
    let tier = match require(&body, "tier") {
        Ok(tier) => match Tier::parse(tier) {
            Some(tier) => tier,
            None => {
                return Problem::bad_request(format!("`{tier}` is not a tier")).into_response();
            }
        },
        Err(problem) => return problem.into_response(),
    };
    let actor = actor_of(&parts);
    let origin = origin_of(&state, &parts);

    let mut inner = state.write();
    let before = json!({ "tier": inner.org.plan.tier.as_str() });
    let plan = Plan::of(tier);
    inner.org.plan = plan.clone();
    inner.meter.retier(plan.clone());
    // The pool moves with the plan; what has been spent this period does not,
    // because it was spent.
    inner.ledger.pool = plan.monthly_credits;
    let after = json!({ "tier": tier.as_str() });
    let at = state.clock().now_ms();
    inner.log.record(
        at,
        Record::new(actor, Area::Settings, "plan.change", "organization")
            .from(origin)
            .changing(before, after.clone()),
    );
    axum::Json(after).into_response()
}

async fn top_up(State(state): State<Arc<OrgState>>, request: Request) -> Response {
    let (parts, body) = match split(request).await {
        Ok(split) => split,
        Err(problem) => return problem.into_response(),
    };
    let Some(credits) = body.get("credits").and_then(Value::as_u64) else {
        return Problem::bad_request("`credits` is required and is a number").into_response();
    };
    let actor = actor_of(&parts);
    let origin = origin_of(&state, &parts);

    let mut inner = state.write();
    let before = json!({ "granted": inner.ledger.granted() });
    inner.ledger.top_up(credits);
    let after = json!({ "granted": inner.ledger.granted(), "toppedUp": inner.ledger.topped_up });
    let at = state.clock().now_ms();
    inner.log.record(
        at,
        Record::new(actor, Area::Settings, "credits.topUp", "organization")
            .from(origin)
            .changing(before, after.clone()),
    );
    axum::Json(after).into_response()
}

async fn set_overage(State(state): State<Arc<OrgState>>, request: Request) -> Response {
    let (parts, body) = match split(request).await {
        Ok(split) => split,
        Err(problem) => return problem.into_response(),
    };
    let Some(enabled) = body.get("enabled").and_then(Value::as_bool) else {
        return Problem::bad_request("`enabled` is required and is a boolean").into_response();
    };
    let actor = actor_of(&parts);
    let origin = origin_of(&state, &parts);

    let mut inner = state.write();
    let before = json!({ "overageEnabled": inner.ledger.overage_enabled });
    inner.ledger.overage_enabled = enabled;
    let after = json!({ "overageEnabled": enabled });
    let at = state.clock().now_ms();
    inner.log.record(
        at,
        Record::new(actor, Area::Settings, "credits.overage", "organization")
            .from(origin)
            .changing(before, after.clone()),
    );
    axum::Json(after).into_response()
}

/// ORG-31 and HOST-13: spending credits for a managed model call. An
/// assistant or agent surface calls this rather than reaching into the ledger,
/// so the refusal and its code are decided in one place.
async fn charge(State(state): State<Arc<OrgState>>, request: Request) -> Response {
    let (parts, body) = match split(request).await {
        Ok(split) => split,
        Err(problem) => return problem.into_response(),
    };
    let project = field(&body, "project").unwrap_or("").to_owned();
    let access = match field(&body, "access").unwrap_or("managed") {
        "managed" => Access::Managed,
        "byok" => Access::Byok,
        other => {
            return Problem::bad_request(format!("`{other}` is not `managed` or `byok`"))
                .into_response();
        }
    };
    let spend = match field(&body, "spend").unwrap_or("assistantAnswer") {
        "assistantAnswer" => Spend::AssistantAnswer,
        "agentTask" => match field(&body, "size").and_then(TaskSize::parse) {
            Some(size) => Spend::AgentTask { size },
            None => {
                return Problem::bad_request("an `agentTask` needs a `size` from the price table")
                    .into_response();
            }
        },
        other => {
            return Problem::bad_request(format!("`{other}` is not a kind of spend"))
                .into_response();
        }
    };
    let actor = actor_of(&parts);
    let origin = origin_of(&state, &parts);

    let mut inner = state.write();
    let charge = inner.ledger.charge(spend, &project, access);
    // The thresholds are read after the refusal is handled, not before: a
    // threshold fires once per period, so asking on a path that then returns
    // early would mark it fired and never deliver it.
    if let super::credits::Charge::Refused { needed, balance } = charge {
        let code = liyasa_core::diagnostics::Code::new("E0852").expect("E0852 is registered");
        return Problem::code(StatusCode::PAYMENT_REQUIRED, code)
            .detail(format!(
                "this task costs {needed} credits and {balance} are left"
            ))
            .extension("needed", json!(needed))
            .extension("balance", json!(balance))
            .extension(
                "help",
                json!("buy a top-up, turn overage on, or use your own model key"),
            )
            .into_response();
    }
    let alerts = inner.ledger.alerts();
    let at = state.clock().now_ms();
    inner.log.record(
        at,
        Record::new(actor, Area::AutomationRuns, "credits.charge", "model")
            .from(origin)
            .creating(json!(charge)),
    );
    axum::Json(json!({
        "charge": charge,
        "balance": inner.ledger.balance(),
        "alerts": alerts,
    }))
    .into_response()
}

/// ORG-03: deletion with a cooling-off period. Nothing is destroyed here — the
/// workspace stays readable and exportable until the period runs out.
async fn request_deletion(State(state): State<Arc<OrgState>>, request: Request) -> Response {
    let (parts, _) = request.into_parts();
    let actor = actor_of(&parts);
    let origin = origin_of(&state, &parts);
    let by = actor.id.clone();

    let mut inner = state.write();
    let deletion = json!(inner.org.request_deletion(&by));
    let at = state.clock().now_ms();
    inner.log.record(
        at,
        Record::new(actor, Area::Settings, "organization.delete", "organization")
            .from(origin)
            .creating(deletion.clone()),
    );
    (StatusCode::ACCEPTED, axum::Json(deletion)).into_response()
}

async fn cancel_deletion(State(state): State<Arc<OrgState>>, request: Request) -> Response {
    let (parts, _) = request.into_parts();
    let actor = actor_of(&parts);
    let origin = origin_of(&state, &parts);

    let mut inner = state.write();
    if !inner.org.cancel_deletion() {
        return Problem::not_found("scheduled deletion").into_response();
    }
    let at = state.clock().now_ms();
    inner.log.record(
        at,
        Record::new(
            actor,
            Area::Settings,
            "organization.restore",
            "organization",
        )
        .from(origin)
        .creating(json!({ "restored": true })),
    );
    axum::Json(json!({ "deletion": Value::Null })).into_response()
}

/// ORG-03's export, which is the other half of the deletion clause.
async fn export(State(state): State<Arc<OrgState>>, request: Request) -> Response {
    let (parts, _) = request.into_parts();
    let actor = actor_of(&parts);
    let origin = origin_of(&state, &parts);

    let mut inner = state.write();
    let mut document = inner.org.export();
    document["audit"] = inner.log.to_json(&audit::Query::default());
    let at = state.clock().now_ms();
    let entries = inner.log.len();
    inner.log.record(
        at,
        Record::new(actor, Area::Settings, "organization.export", "organization")
            .from(origin)
            .creating(json!({ "entries": entries })),
    );
    axum::Json(document).into_response()
}

/// Sends a notification to everyone who asked for it (ORG-21). Not a route:
/// the surfaces that raise these live in other packages and call this.
pub fn announce(state: &OrgState, notification: &notify::Notification) -> notify::Routed {
    let inner = state.read();
    notify::route(&inner.subscribers, &inner.endpoints, notification)
}

/// Adds a member and subscribes them on the standard preferences, which is
/// what every path that creates a member wants.
pub fn add_member(state: &OrgState, member: Member) -> Result<(), Diagnostic> {
    let mut inner = state.write();
    let subscriber = Subscriber::new(member.user.clone(), member.email.clone());
    inner.org.add_member(member)?;
    inner.subscribers.retain(|s| s.user != subscriber.user);
    inner.subscribers.push(subscriber);
    Ok(())
}

/// ORG-03's settings, for a caller building a state rather than a request.
pub fn settings(name: &str, region: Region) -> Settings {
    let mut settings = Settings::new(name);
    settings.default_region = region;
    settings
}
