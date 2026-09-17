//! The deploy REST surface (GIT-21, GIT-24, GIT-33, GIT-40).
//!
//! A `Router` of its own, merged onto the server's. Nothing under `routes/`
//! changes, and a server built without a store simply never merges it.

use std::sync::Arc;

use axum::Router;
use axum::extract::{Path, Query, State};
use axum::response::{IntoResponse, Response};
use axum::routing::{get, post};
use http::StatusCode;
use liyasa_core::diagnostics::code::{E0804, E0805};
use liyasa_core::ids::{BuildId, Fingerprint, JobId, ProjectId};
use liyasa_core::store::Page;
use serde::Deserialize;
use serde_json::{Value, json};

use super::environment::EnvironmentKind;
use super::queue::{self, Class, Trigger};
use super::rollback::{Actor, RollbackError};
use super::service::DeployState;
use crate::routes::api::Json;
use crate::routes::jobs;
use crate::routes::problem::Problem;

/// The actor an authentication layer put on the request, or an unprivileged
/// one. Defaulting to a member rather than an admin is what makes
/// `Policy { admins_only: true }` mean anything on a server with no auth layer
/// wired up yet.
fn actor(parts: &http::request::Parts) -> Actor {
    parts
        .extensions
        .get::<Actor>()
        .cloned()
        .unwrap_or_else(|| Actor::member("anonymous"))
}

#[derive(Debug, Clone, Default, Deserialize)]
pub struct ProjectParam {
    pub project: Option<String>,
}

fn project_of(params: &ProjectParam) -> Option<ProjectId> {
    params.project.as_deref().and_then(ProjectId::parse)
}

fn missing_project() -> Response {
    Problem::bad_request("`project` is required and is a ULID").into_response()
}

fn queue_error(error: super::queue::QueueError) -> Response {
    match error {
        super::queue::QueueError::Full { depth, cap } => Problem::code(
            StatusCode::SERVICE_UNAVAILABLE,
            liyasa_core::diagnostics::code::E0809,
        )
        .detail(format!(
            "the build queue holds {depth} jobs, at its cap of {cap}"
        ))
        .into_response(),
        super::queue::QueueError::Store(error) => {
            tracing::error!(target: "liyasa_server", %error, "the build queue could not be read");
            Problem::code(StatusCode::INTERNAL_SERVER_ERROR, E0804)
                .detail("the build queue could not be read")
                .into_response()
        }
    }
}

#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct TriggerInput {
    pub project: String,
    pub branch: String,
    pub commit: String,
    /// Overrides the environment the branch would otherwise deploy to.
    #[serde(default)]
    pub env: Option<String>,
    #[serde(default)]
    pub message: Option<String>,
    /// A preview of a pull request rather than of a branch (GIT-33).
    #[serde(default)]
    pub pull_request: Option<u64>,
}

/// `POST /_liyasa/api/v1/builds` — the dashboard, the CLI and the REST API all
/// arrive here (GIT-21, GIT-33).
pub async fn trigger(
    State(state): State<Arc<DeployState>>,
    axum::Json(input): axum::Json<TriggerInput>,
) -> Response {
    let Some(project) = ProjectId::parse(&input.project) else {
        return Problem::bad_request("`project` is a ULID").into_response();
    };
    let Some(binding) = state.binding_of(&project) else {
        return Problem::not_found("project").into_response();
    };
    let environment = match &input.env {
        Some(name) => match super::environment::by_name(&binding.environments, name) {
            Some(environment) => environment,
            None => return Problem::not_found("environment").into_response(),
        },
        None => binding.environment_for(&input.branch),
    };
    let class = match input.pull_request {
        Some(_) => Class::Preview,
        None => Class::for_environment(environment.kind),
    };
    let mut request = super::queue::BuildRequest::new(
        project,
        &environment.name,
        class,
        binding.repo.full_name(),
        &input.branch,
        &input.commit,
    )
    .with_trigger(Trigger::Manual);
    if let Some(message) = input.message {
        request = request.with_message(message);
    }
    if let Some(number) = input.pull_request {
        request = request.for_pull_request(number);
    }
    // A manually triggered build of a branch outside the trusted set is held
    // to the same rules as one a webhook triggered: who pressed the button
    // does not change what the branch contains.
    if environment.kind != EnvironmentKind::Production
        && !super::untrusted::trusted_branch(
            &input.branch,
            &binding.deploy_branch,
            &binding.trusted_branches,
        )
    {
        request = request.untrusted();
    }
    super::hooks::submit(&state, binding, &request).await
}

#[derive(Debug, Clone, Default, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct QueueParams {
    /// The measured average build duration, so the estimate reflects this
    /// installation rather than a guess baked into the server.
    pub average_build_ms: Option<i64>,
    pub workers: Option<u32>,
}

/// `GET /_liyasa/api/v1/builds` — the queue with each build's position and
/// estimated start (§6.13, GIT-24).
pub async fn queued(
    State(state): State<Arc<DeployState>>,
    Query(params): Query<QueueParams>,
) -> Response {
    let (pending, running) = match state.queue.pending().await {
        Ok(pair) => pair,
        Err(error) => return queue_error(error),
    };
    let depth = pending.len() as u64 + running.len() as u64;
    let workers = params.workers.unwrap_or(1);
    let items: Vec<Value> = queue::order(&pending)
        .into_iter()
        .enumerate()
        .map(|(index, id)| {
            let position = u32::try_from(index + 1).unwrap_or(u32::MAX);
            let project = pending
                .iter()
                .find(|job| job.id == id)
                .map(|job| job.project.to_string());
            json!({
                "jobId": id.to_string(),
                "project": project,
                "position": position,
                "estimatedStartMs": queue::estimated_start_ms(
                    position,
                    params.average_build_ms,
                    workers,
                ),
            })
        })
        .collect();
    Json(json!({
        "items": items,
        "depth": depth,
        "running": running.len(),
        "cap": state.queue.limits().queue,
        "concurrencyPerProject": state.queue.limits().concurrency_per_project,
    }))
    .into_response()
}

/// `GET /_liyasa/api/v1/builds/{id}` — status polling (GIT-21).
pub async fn status(State(state): State<Arc<DeployState>>, Path(id): Path<String>) -> Response {
    let Some(job_id) = JobId::parse(&id) else {
        return Problem::bad_request("`id` is a ULID").into_response();
    };
    let Some(store) = state.app.store.clone() else {
        return Problem::not_found("store").into_response();
    };
    let job = match store.jobs_typed().get(&job_id).await {
        Ok(Some(job)) => job,
        Ok(None) => return Problem::not_found("build").into_response(),
        Err(error) => return Problem::store(&error).into_response(),
    };
    let position = match state.queue.position(job_id).await {
        Ok(position) => position,
        Err(error) => return queue_error(error),
    };
    let mut body = jobs::to_json(&job);
    body["position"] = json!(position);
    body["class"] = json!(Class::from_priority(job.priority).as_str());
    Json(body).into_response()
}

/// `POST /_liyasa/api/v1/builds/{id}/deploy` — makes the build a finished job
/// produced live (GIT-20).
///
/// The pointer moves, then the embedding job is queued and not waited on, so a
/// network-bound model provider is never on the deploy path (AST-01).
pub async fn activate(State(state): State<Arc<DeployState>>, Path(id): Path<String>) -> Response {
    let Some(job_id) = JobId::parse(&id) else {
        return Problem::bad_request("`id` is a ULID").into_response();
    };
    let Some(store) = state.app.store.clone() else {
        return Problem::not_found("store").into_response();
    };
    let job = match store.jobs_typed().get(&job_id).await {
        Ok(Some(job)) => job,
        Ok(None) => return Problem::not_found("build").into_response(),
        Err(error) => return Problem::store(&error).into_response(),
    };
    let Some(project) = job.project else {
        return Problem::not_found("project").into_response();
    };
    let outcome = job
        .result
        .clone()
        .and_then(|result| serde_json::from_value::<super::queue::BuildOutcome>(result).ok());
    let Some(outcome) = outcome.filter(|outcome| !outcome.build_id.is_empty()) else {
        return Problem::new(StatusCode::CONFLICT, "Build has no outcome")
            .detail("this build job has not recorded the build it produced")
            .into_response();
    };
    let Some(build) = Fingerprint::parse(&outcome.build_id).map(BuildId) else {
        return Problem::code(StatusCode::INTERNAL_SERVER_ERROR, E0804)
            .detail("the build job recorded a build id that is not a digest")
            .into_response();
    };
    let env = job.payload["env"]
        .as_str()
        .unwrap_or("production")
        .to_owned();
    if let Err(error) = store
        .deployments_typed()
        .point(&project, &env, &build)
        .await
    {
        tracing::error!(target: "liyasa_server", %error, "a deployment pointer could not be moved");
        return Problem::code(StatusCode::INTERNAL_SERVER_ERROR, E0804)
            .detail("the deployment pointer could not be moved")
            .into_response();
    }
    let embedding = state
        .queue
        .queue_embedding(&project, &outcome.build_id)
        .await
        .ok()
        .flatten()
        .map(|id| id.to_string());
    let body = json!({
        "project": project.to_string(),
        "env": env,
        "buildId": outcome.build_id,
        "commit": job.payload["commit"].as_str(),
        "embeddingJobId": embedding,
    });
    state.app.notify_webhook("deployment.succeeded", &body);
    Json(body).into_response()
}

/// `GET /_liyasa/api/v1/deployments/{env}/history` (GIT-21).
pub async fn history(
    State(state): State<Arc<DeployState>>,
    Path(env): Path<String>,
    Query(params): Query<ProjectParam>,
) -> Response {
    let Some(project) = project_of(&params) else {
        return missing_project();
    };
    let Some(store) = state.app.store.clone() else {
        return Problem::not_found("store").into_response();
    };
    let records = match store
        .deployments_typed()
        .history(
            &project,
            &env,
            &Page {
                cursor: None,
                limit: super::rollback::HISTORY_PAGE,
            },
        )
        .await
    {
        Ok(records) => records,
        Err(error) => return Problem::store(&error).into_response(),
    };
    // The build record holds no commit, logs, diagnostics or verification
    // report; the job that produced it holds all four. Joining them here is
    // what makes the history GIT-21 describes (RFC 1605).
    let outcomes = state
        .queue
        .outcomes(&project, super::rollback::HISTORY_PAGE)
        .await
        .unwrap_or_default();
    let mut items = Vec::with_capacity(records.len());
    for record in &records {
        let build = store.builds_typed().get(&record.build).await.ok().flatten();
        let id = record.build.to_string();
        let job = outcomes.iter().find(|(_, outcome)| outcome.build_id == id);
        items.push(json!({
            "buildId": id,
            "env": record.env,
            "deployedAt": record.created_at,
            "status": build.as_ref().map(|b| format!("{:?}", b.status).to_lowercase()),
            "durationMs": build
                .as_ref()
                .map(|b| b.updated_at.saturating_sub(b.created_at)),
            "dist": build.as_ref().map(|b| b.dist.clone()),
            "commit": job.and_then(|(job, _)| job.payload["commit"].as_str()),
            "branch": job.and_then(|(job, _)| job.payload["branch"].as_str()),
            "message": job.and_then(|(job, _)| job.payload["message"].as_str()),
            "trigger": job.and_then(|(job, _)| job.payload["trigger"].as_str()),
            "logsUrl": job.and_then(|(_, outcome)| outcome.logs_url.clone()),
            "errors": job.map(|(_, outcome)| outcome.errors),
            "warnings": job.map(|(_, outcome)| outcome.warnings),
            "verification": job.and_then(|(_, outcome)| outcome.verification.clone()),
        }));
    }
    Json(json!({ "items": items, "nextCursor": Value::Null })).into_response()
}

/// `GET /_liyasa/api/v1/deployments/{env}/retained` — what a rollback may
/// target (GIT-40).
pub async fn retained(
    State(state): State<Arc<DeployState>>,
    Path(env): Path<String>,
    Query(params): Query<ProjectParam>,
) -> Response {
    let Some(project) = project_of(&params) else {
        return missing_project();
    };
    match state.rollback.retained(&project, &env).await {
        Ok(builds) => Json(json!({
            "items": builds.iter().map(|id| id.to_string()).collect::<Vec<_>>(),
        }))
        .into_response(),
        Err(error) => rollback_error(&error),
    }
}

/// `POST /_liyasa/api/v1/deployments/{env}/rollback/{buildId}` (GIT-40, GIT-41).
pub async fn rollback_to(
    State(state): State<Arc<DeployState>>,
    Path((env, build)): Path<(String, String)>,
    Query(params): Query<ProjectParam>,
    request: http::Request<axum::body::Body>,
) -> Response {
    let Some(project) = project_of(&params) else {
        return missing_project();
    };
    let Some(build) = Fingerprint::parse(&build).map(BuildId) else {
        return Problem::bad_request("`buildId` is a blake3 digest").into_response();
    };
    let (parts, _) = request.into_parts();
    match state
        .rollback
        .to(&actor(&parts), &project, &env, &build)
        .await
    {
        Ok(outcome) => {
            let body = outcome_json(&outcome);
            state.app.notify_webhook("deployment.rolled_back", &body);
            Json(body).into_response()
        }
        Err(error) => rollback_error(&error),
    }
}

/// `POST /_liyasa/api/v1/deployments/{env}/latest` — one action back to the
/// newest build (GIT-40).
pub async fn return_to_latest(
    State(state): State<Arc<DeployState>>,
    Path(env): Path<String>,
    Query(params): Query<ProjectParam>,
    request: http::Request<axum::body::Body>,
) -> Response {
    let Some(project) = project_of(&params) else {
        return missing_project();
    };
    let (parts, _) = request.into_parts();
    match state.rollback.latest(&actor(&parts), &project, &env).await {
        Ok(outcome) => {
            let body = outcome_json(&outcome);
            state.app.notify_webhook("deployment.succeeded", &body);
            Json(body).into_response()
        }
        Err(error) => rollback_error(&error),
    }
}

fn outcome_json(outcome: &super::rollback::Outcome) -> Value {
    json!({
        "project": outcome.project.to_string(),
        "env": outcome.env,
        "buildId": outcome.build.to_string(),
        "previousBuildId": outcome.previous.map(|id| id.to_string()),
        "purgedTag": outcome.purged_tag,
        "switchMs": outcome.took.as_millis() as u64,
    })
}

fn rollback_error(error: &RollbackError) -> Response {
    match error {
        RollbackError::NotPermitted(_) => Problem::new(StatusCode::FORBIDDEN, "Not permitted")
            .detail(error.to_string())
            .into_response(),
        RollbackError::NotRetained(_) => Problem::code(StatusCode::CONFLICT, E0805)
            .detail(error.to_string())
            .into_response(),
        RollbackError::NeverDeployed(_) => Problem::not_found("deployment").into_response(),
        RollbackError::AlreadyLatest(_) => Problem::new(StatusCode::CONFLICT, "Already latest")
            .detail(error.to_string())
            .into_response(),
        RollbackError::Store(detail) => {
            tracing::error!(target: "liyasa_server", %detail, "a rollback could not be read");
            Problem::code(StatusCode::INTERNAL_SERVER_ERROR, E0804)
                .detail("the rollback could not be completed")
                .into_response()
        }
        RollbackError::Purge(detail) => Problem::code(StatusCode::BAD_GATEWAY, E0804)
            .detail(format!(
                "the environment now serves the requested build, but the edge purge failed: {detail}"
            ))
            .into_response(),
    }
}

/// Every route this package adds. Merged onto the server's router.
pub fn router(state: Arc<DeployState>) -> Router {
    Router::new()
        .route("/_liyasa/hooks", get(super::hooks::configured))
        .route("/_liyasa/hooks/{provider}", post(super::hooks::receive))
        .route("/_liyasa/api/v1/builds", get(queued).post(trigger))
        .route("/_liyasa/api/v1/builds/{id}", get(status))
        .route("/_liyasa/api/v1/builds/{id}/deploy", post(activate))
        .route("/_liyasa/api/v1/deployments/{env}/history", get(history))
        .route("/_liyasa/api/v1/deployments/{env}/retained", get(retained))
        .route(
            "/_liyasa/api/v1/deployments/{env}/rollback/{buildId}",
            post(rollback_to),
        )
        .route(
            "/_liyasa/api/v1/deployments/{env}/latest",
            post(return_to_latest),
        )
        .with_state(state)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_request_with_no_authenticated_actor_is_not_an_admin() {
        let request = http::Request::builder()
            .uri("/")
            .body(())
            .expect("a request");
        let (parts, ()) = request.into_parts();
        let actor = actor(&parts);
        assert!(
            !actor.admin,
            "an unauthenticated caller must not be an admin"
        );
        assert_eq!(actor.id, "anonymous");
    }

    #[test]
    fn an_authenticated_admin_is_read_from_the_request() {
        let mut request = http::Request::builder()
            .uri("/")
            .body(())
            .expect("a request");
        request.extensions_mut().insert(Actor::admin("root"));
        let (parts, ()) = request.into_parts();
        assert!(actor(&parts).admin);
    }

    #[test]
    fn a_rollback_to_a_build_that_is_gone_is_the_rollback_target_code() {
        let response = rollback_error(&RollbackError::NotRetained("blake3:aa".to_owned()));
        assert_eq!(response.status(), StatusCode::CONFLICT);
    }

    #[test]
    fn a_failed_purge_reports_that_the_pointer_did_move() {
        let response = rollback_error(&RollbackError::Purge("502 from the edge".to_owned()));
        assert_eq!(
            response.status(),
            StatusCode::BAD_GATEWAY,
            "the caller must know the switch happened and the edge is stale"
        );
    }

    #[test]
    fn a_member_asking_for_an_admin_only_rollback_is_forbidden() {
        let response = rollback_error(&RollbackError::NotPermitted("production".to_owned()));
        assert_eq!(response.status(), StatusCode::FORBIDDEN);
    }

    #[test]
    fn a_missing_project_parameter_is_named_rather_than_guessed() {
        assert_eq!(project_of(&ProjectParam::default()), None);
        assert_eq!(missing_project().status(), StatusCode::BAD_REQUEST);
        assert_eq!(
            project_of(&ProjectParam {
                project: Some("not-a-ulid".to_owned())
            }),
            None
        );
    }
}
