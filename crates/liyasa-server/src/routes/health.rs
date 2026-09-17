//! Health, readiness, and metrics (HOST-05).
//!
//! Liveness says the process is running. Readiness says it can serve: a
//! bundle is loaded and, when one is configured, the store answers. A
//! Kubernetes probe uses the first to decide whether to restart and the
//! second to decide whether to route (HOST-03).

use std::sync::Arc;

use axum::extract::State;
use axum::response::{IntoResponse, Response};
use http::{HeaderValue, StatusCode, header};
use serde_json::json;

use super::AppState;
use super::api::JsonStatus;

pub const VERSION: &str = env!("CARGO_PKG_VERSION");

pub async fn health(State(state): State<Arc<AppState>>) -> Response {
    JsonStatus(
        StatusCode::OK,
        json!({
            "status": "ok",
            "version": VERSION,
            "uptimeSeconds": state.started.elapsed().as_secs(),
        }),
    )
    .into_response()
}

pub async fn ready(State(state): State<Arc<AppState>>) -> Response {
    let subtrees: Vec<serde_json::Value> = state
        .mounted()
        .iter()
        .map(|record| {
            json!({
                "name": record.name,
                "mounted": record.mounted,
                "skipped": record.skipped,
            })
        })
        .collect();
    let mut checks = Vec::new();
    let mut ready = true;

    if state.config.collector_only {
        checks.push(json!({ "name": "mode", "status": "pass", "detail": "collector-only" }));
    } else if state.bundle.is_some() {
        checks.push(json!({ "name": "bundle", "status": "pass" }));
    } else {
        ready = false;
        checks.push(
            json!({ "name": "bundle", "status": "fail", "detail": "no deployment is loaded" }),
        );
    }

    match &state.store {
        Some(store) => match store.jobs_typed().depth(None).await {
            Ok(depth) => {
                checks.push(json!({ "name": "store", "status": "pass", "queueDepth": depth }))
            }
            Err(error) => {
                ready = false;
                checks.push(
                    json!({ "name": "store", "status": "fail", "detail": error.to_string() }),
                );
            }
        },
        None => {
            checks.push(json!({ "name": "store", "status": "pass", "detail": "not configured" }))
        }
    }

    // RFC 1404: a job name in the queue with no handler is work nobody can
    // do, and it is invisible unless something says so.
    let mut orphaned_jobs: Vec<String> = Vec::new();
    if let Ok(registered) = super::work::registered(&state, super::work::kinds()).await {
        orphaned_jobs = registered.orphaned;
        if !orphaned_jobs.is_empty() {
            checks.push(json!({
                "name": "jobs",
                "status": "warn",
                "detail": format!(
                    "queued with no handler: {}",
                    orphaned_jobs.join(", ")
                ),
            }));
        }
    }

    if state.draining() {
        // A draining replica finishes what it has and takes no new traffic.
        ready = false;
        checks.push(json!({ "name": "drain", "status": "fail", "detail": "shutting down" }));
    }

    JsonStatus(
        if ready {
            StatusCode::OK
        } else {
            StatusCode::SERVICE_UNAVAILABLE
        },
        json!({
            "status": if ready { "ready" } else { "unready" },
            "version": VERSION,
            "checks": checks,
            // RFC 1403: which subtrees this instance mounted, and why not.
            "subtrees": subtrees,
            // RFC 1404: job names queued that nothing here can run.
            "orphanedJobs": orphaned_jobs,
        }),
    )
    .into_response()
}

pub async fn metrics(State(state): State<Arc<AppState>>) -> Response {
    // Gauges are sampled at scrape time rather than written on every request.
    let ingest = &state.ingest.metrics;
    use std::sync::atomic::Ordering;
    state.metrics.set(
        "liyasa_ingest_queue_depth",
        &[],
        state.ingest.depth() as u64,
    );
    for (disposition, value) in [
        ("received", ingest.received.load(Ordering::Relaxed)),
        ("written", ingest.written.load(Ordering::Relaxed)),
        ("spilled", ingest.spilled.load(Ordering::Relaxed)),
        ("dropped", ingest.dropped()),
    ] {
        state.metrics.set(
            "liyasa_ingest_events_total",
            &[("disposition", disposition)],
            value,
        );
    }
    if let Some(store) = &state.store
        && let Ok(depth) = store.jobs_typed().depth(None).await
    {
        state.metrics.set("liyasa_jobs_queue_depth", &[], depth);
    }
    state
        .metrics
        .set("liyasa_build_info", &[("version", VERSION)], 1);

    let body = state.metrics.render();
    let mut response = Response::new(body.into());
    response.headers_mut().insert(
        header::CONTENT_TYPE,
        HeaderValue::from_static("text/plain; version=0.0.4; charset=utf-8"),
    );
    response
}
