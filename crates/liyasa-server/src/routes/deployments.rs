//! Deployments over REST (REST-01).
//!
//! A deployment is a pointer from an environment to a build. Pointing it is
//! the deploy, pointing it back is the rollback, and neither touches what is
//! being served until the swap commits (NFR-30).

use std::sync::Arc;

use axum::extract::{Path, Query, State};
use axum::response::{IntoResponse, Response};
use http::StatusCode;
use liyasa_core::ids::{BuildId, Fingerprint, ProjectId};
use liyasa_store::records::DeploymentRecord;
use serde::Deserialize;
use serde_json::{Value, json};

use super::AppState;
use super::api::{Json, JsonStatus};
use super::problem::Problem;

fn to_json(record: &DeploymentRecord) -> Value {
    json!({
        "project": record.project.to_string(),
        "env": record.env,
        "buildId": record.build.to_string(),
        "createdAt": record.created_at,
        "updatedAt": record.updated_at,
    })
}

#[derive(Debug, Clone, Default, Deserialize)]
pub struct ListParams {
    pub project: Option<String>,
    pub env: Option<String>,
}

pub async fn list(
    State(state): State<Arc<AppState>>,
    Query(params): Query<ListParams>,
) -> Response {
    let Some(store) = state.store.clone() else {
        return Problem::not_found("store").into_response();
    };
    let project = match params.project.as_deref().map(ProjectId::parse) {
        Some(None) => return Problem::bad_request("`project` is a ULID").into_response(),
        other => other.flatten(),
    };
    match store
        .deployments_typed()
        .list(project.as_ref(), params.env.as_deref())
        .await
    {
        Ok(rows) => Json(json!({
            "items": rows.iter().map(to_json).collect::<Vec<_>>(),
            "nextCursor": Value::Null,
        }))
        .into_response(),
        Err(error) => Problem::store(&error).into_response(),
    }
}

#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct DeployInput {
    pub project: String,
    pub env: String,
    /// The build to point at. A branch-triggered deploy queues a build first;
    /// that half is the deploy package's (WP-16).
    pub build_id: String,
}

pub async fn create(
    State(state): State<Arc<AppState>>,
    axum::Json(input): axum::Json<DeployInput>,
) -> Response {
    let Some(store) = state.store.clone() else {
        return Problem::not_found("store").into_response();
    };
    let Some(project) = ProjectId::parse(&input.project) else {
        return Problem::bad_request("`project` is a ULID").into_response();
    };
    let Some(build) = Fingerprint::parse(&input.build_id).map(BuildId) else {
        return Problem::bad_request("`buildId` is a blake3 digest").into_response();
    };
    match store.builds_typed().get(&build).await {
        Ok(None) => return Problem::not_found("build").into_response(),
        Err(error) => return Problem::store(&error).into_response(),
        Ok(Some(record)) if record.status != liyasa_core::store::BuildStatus::Succeeded => {
            return Problem::new(StatusCode::CONFLICT, "Build did not succeed")
                .detail("only a succeeded build may be deployed")
                .into_response();
        }
        Ok(Some(_)) => {}
    }
    match store
        .deployments_typed()
        .point(&project, &input.env, &build)
        .await
    {
        Ok(()) => {
            let body = json!({
                "project": project.to_string(),
                "env": input.env,
                "buildId": build.to_string(),
            });
            state.notify_webhook("deployment.succeeded", &body);
            JsonStatus(StatusCode::CREATED, body).into_response()
        }
        Err(error) => Problem::store(&error).into_response(),
    }
}

pub async fn current(
    State(state): State<Arc<AppState>>,
    Path(env): Path<String>,
    Query(params): Query<ListParams>,
) -> Response {
    let Some(store) = state.store.clone() else {
        return Problem::not_found("store").into_response();
    };
    let Some(project) = params.project.as_deref().and_then(ProjectId::parse) else {
        return Problem::bad_request("`project` is required and is a ULID").into_response();
    };
    match store.deployments_typed().current(&project, &env).await {
        Ok(Some(record)) => Json(to_json(&record)).into_response(),
        Ok(None) => Problem::not_found("deployment").into_response(),
        Err(error) => Problem::store(&error).into_response(),
    }
}

pub async fn rollback(
    State(state): State<Arc<AppState>>,
    Path(env): Path<String>,
    Query(params): Query<ListParams>,
) -> Response {
    let Some(store) = state.store.clone() else {
        return Problem::not_found("store").into_response();
    };
    let Some(project) = params.project.as_deref().and_then(ProjectId::parse) else {
        return Problem::bad_request("`project` is required and is a ULID").into_response();
    };
    let previous = match store.deployments_typed().previous(&project, &env).await {
        Ok(Some(build)) => build,
        Ok(None) => {
            let code = liyasa_core::diagnostics::Code::new("E0805").expect("E0805 is registered");
            return Problem::code(StatusCode::CONFLICT, code)
                .detail("this environment has nothing to roll back to")
                .into_response();
        }
        Err(error) => return Problem::store(&error).into_response(),
    };
    match store
        .deployments_typed()
        .point(&project, &env, &previous)
        .await
    {
        Ok(()) => {
            let body = json!({
                "project": project.to_string(),
                "env": env,
                "buildId": previous.to_string(),
            });
            state.notify_webhook("deployment.rolled_back", &body);
            Json(body).into_response()
        }
        Err(error) => Problem::store(&error).into_response(),
    }
}

pub async fn delete(
    State(state): State<Arc<AppState>>,
    Path(env): Path<String>,
    Query(params): Query<ListParams>,
) -> Response {
    let Some(store) = state.store.clone() else {
        return Problem::not_found("store").into_response();
    };
    let Some(project) = params.project.as_deref().and_then(ProjectId::parse) else {
        return Problem::bad_request("`project` is required and is a ULID").into_response();
    };
    match store.deployments_typed().delete(&project, &env).await {
        Ok(()) => StatusCode::NO_CONTENT.into_response(),
        Err(error) => Problem::store(&error).into_response(),
    }
}
