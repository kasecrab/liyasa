//! The job queue over REST (HOST-07).
//!
//! `liyasa jobs list|retry|cancel` and the dashboard page are both this.

use std::sync::Arc;

use axum::extract::{Path, Query, State};
use axum::response::{IntoResponse, Response};
use http::StatusCode;
use liyasa_core::ids::JobId;
use liyasa_core::store::{JobQuery, JobState};
use liyasa_store::records::JobRecord;
use serde::Deserialize;
use serde_json::{Value, json};

use super::AppState;
use super::api::{Json, PageParams, Paged};
use super::problem::Problem;

pub fn state_text(state: JobState) -> &'static str {
    match state {
        JobState::Queued => "queued",
        JobState::Leased => "leased",
        JobState::Done => "done",
        JobState::Failed => "failed",
        JobState::Dead => "dead",
    }
}

fn parse_state(text: &str) -> Option<JobState> {
    match text {
        "queued" => Some(JobState::Queued),
        "leased" => Some(JobState::Leased),
        "done" => Some(JobState::Done),
        "failed" => Some(JobState::Failed),
        "dead" => Some(JobState::Dead),
        _ => None,
    }
}

pub fn to_json(job: &JobRecord) -> Value {
    json!({
        "id": job.id.to_string(),
        "name": job.name,
        "key": job.key,
        "priority": job.priority,
        "state": state_text(job.state),
        "attempts": job.attempts,
        "maxAttempts": job.max_attempts,
        "runAt": job.run_at,
        "leaseUntil": job.lease_until,
        "worker": job.worker,
        "result": job.result,
        "error": job.error,
        "createdAt": job.created_at,
        "updatedAt": job.updated_at,
    })
}

#[derive(Debug, Clone, Default, Deserialize)]
pub struct ListParams {
    pub cursor: Option<String>,
    pub limit: Option<u32>,
    pub name: Option<String>,
    pub state: Option<String>,
}

pub async fn list(
    State(state): State<Arc<AppState>>,
    Query(params): Query<ListParams>,
) -> Response {
    let Some(store) = state.store.clone() else {
        return Problem::not_found("store").into_response();
    };
    let page = PageParams {
        cursor: params.cursor.clone(),
        limit: params.limit,
    }
    .to_page();
    let query = JobQuery {
        name: params.name.clone(),
        state: match params.state.as_deref() {
            Some(text) => match parse_state(text) {
                Some(state) => Some(state),
                None => return Problem::bad_request("unknown `state`").into_response(),
            },
            None => None,
        },
        project: None,
    };
    let limit = page.limit;
    match store.jobs_typed().list(&query, page).await {
        Ok(rows) => Paged::new(rows.iter().map(to_json).collect::<Vec<_>>(), limit, |row| {
            row["id"].as_str().unwrap_or_default().to_owned()
        })
        .into_response(),
        Err(error) => Problem::store(&error).into_response(),
    }
}

async fn with_job<F, Fut>(state: &AppState, id: &str, action: F) -> Response
where
    F: FnOnce(Arc<liyasa_store::SqliteStore>, JobId) -> Fut,
    Fut: Future<Output = Result<(), liyasa_core::store::StoreError>>,
{
    let Some(store) = state.store.clone() else {
        return Problem::not_found("store").into_response();
    };
    let Some(job_id) = JobId::parse(id) else {
        return Problem::bad_request("`id` is a ULID").into_response();
    };
    match action(store.clone(), job_id).await {
        Ok(()) => match store.jobs_typed().get(&job_id).await {
            Ok(Some(job)) => Json(to_json(&job)).into_response(),
            Ok(None) => Problem::not_found("job").into_response(),
            Err(error) => Problem::store(&error).into_response(),
        },
        Err(liyasa_core::store::StoreError::NotFound) => {
            Problem::new(StatusCode::CONFLICT, "Not in that state")
                .detail("no job with that id is in a state this action applies to")
                .into_response()
        }
        Err(error) => Problem::store(&error).into_response(),
    }
}

pub async fn get(State(state): State<Arc<AppState>>, Path(id): Path<String>) -> Response {
    let Some(store) = state.store.clone() else {
        return Problem::not_found("store").into_response();
    };
    let Some(job_id) = JobId::parse(&id) else {
        return Problem::bad_request("`id` is a ULID").into_response();
    };
    match store.jobs_typed().get(&job_id).await {
        Ok(Some(job)) => Json(to_json(&job)).into_response(),
        Ok(None) => Problem::not_found("job").into_response(),
        Err(error) => Problem::store(&error).into_response(),
    }
}

pub async fn retry(State(state): State<Arc<AppState>>, Path(id): Path<String>) -> Response {
    with_job(&state, &id, |store, id| async move {
        store.jobs_typed().retry(&id).await
    })
    .await
}

pub async fn cancel(State(state): State<Arc<AppState>>, Path(id): Path<String>) -> Response {
    with_job(&state, &id, |store, id| async move {
        store.jobs_typed().cancel(&id).await
    })
    .await
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn every_state_round_trips_through_its_wire_name() {
        for state in [
            JobState::Queued,
            JobState::Leased,
            JobState::Done,
            JobState::Failed,
            JobState::Dead,
        ] {
            assert_eq!(parse_state(state_text(state)), Some(state));
        }
        assert_eq!(parse_state("running"), None);
    }

    #[test]
    fn a_job_row_names_its_lease_and_its_attempts() {
        let job = JobRecord {
            id: JobId(liyasa_store::new_ulid()),
            name: "verify.nightly".to_owned(),
            key: "site".to_owned(),
            priority: 0,
            state: JobState::Leased,
            project: None,
            payload: json!({}),
            attempts: 2,
            max_attempts: 5,
            run_at: 10,
            lease_ms: 60_000,
            lease_until: Some(70_000),
            worker: Some("replica-a".to_owned()),
            result: None,
            error: None,
            created_at: 1,
            updated_at: 2,
            version: 1,
        };
        let row = to_json(&job);
        assert_eq!(row["state"], "leased");
        assert_eq!(row["attempts"], 2);
        assert_eq!(row["worker"], "replica-a");
        assert_eq!(row["leaseUntil"], 70_000);
    }
}
