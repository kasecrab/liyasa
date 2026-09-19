//! The inbound webhook endpoint (GIT-01, GIT-04, GIT-20, GIT-30).
//!
//! Every rule in GIT-01 runs before anything else touches the payload, and a
//! failure is `E0808` with nothing queued. The handler takes the body as raw
//! bytes on purpose: a signature is over the bytes that arrived, and a body
//! that has been through `serde` and back is a different sequence of bytes.

use std::sync::Arc;

use axum::body::Bytes;
use axum::extract::{Path, State};
use axum::response::{IntoResponse, Response};
use http::{HeaderMap, StatusCode};
use liyasa_core::diagnostics::code::{E0804, E0808, E0809};
use liyasa_git::event::{Event, PullRequestEvent, Push};
use liyasa_git::webhook::{Delivery, Provider, Rejection};
use serde_json::json;

use super::environment::EnvironmentKind;
use super::queue::{Accepted, BuildRequest, Class, QueueError, Trigger};
use super::service::{Binding, DeployState};
use super::untrusted::classify;
use crate::routes::api::{Json, JsonStatus};
use crate::routes::problem::Problem;

fn header_pairs(headers: &HeaderMap) -> Vec<(String, String)> {
    headers
        .iter()
        .filter_map(|(name, value)| {
            value
                .to_str()
                .ok()
                .map(|text| (name.as_str().to_owned(), text.to_owned()))
        })
        .collect()
}

/// `POST /_liyasa/hooks/{provider}`.
pub async fn receive(
    State(state): State<Arc<DeployState>>,
    Path(provider): Path<String>,
    headers: HeaderMap,
    body: Bytes,
) -> Response {
    let Some(provider) = Provider::parse(&provider) else {
        return Problem::not_found("provider").into_response();
    };
    let Some(verifier) = state.hooks.verifier(provider) else {
        // No secret configured for this provider means the endpoint is not in
        // service; saying so is better than a signature failure an operator
        // would spend an afternoon on.
        return Problem::new(StatusCode::NOT_FOUND, "Provider not configured")
            .detail(format!(
                "no webhook secret is configured for `{}`",
                provider.as_str()
            ))
            .into_response();
    };

    let pairs = header_pairs(&headers);
    let delivery = Delivery {
        provider,
        headers: &pairs,
        body: &body,
    };
    let now = liyasa_store::now_ms() / 1000;
    let verified = match verifier.verify(&delivery, now) {
        Ok(verified) => verified,
        Err(rejection) => return refuse(provider, &rejection),
    };

    let event = match liyasa_git::event::parse(&verified, &body) {
        Ok(event) => event,
        Err(error) => {
            return Problem::new(StatusCode::BAD_REQUEST, "Unreadable payload")
                .detail(error.to_string())
                .into_response();
        }
    };

    match &event {
        Event::Ignored { kind } => ignored(kind),
        Event::Push(push) => on_push(&state, &event, push).await,
        Event::PullRequest(pull) => on_pull_request(&state, &event, pull).await,
    }
}

/// GIT-01: a rejected delivery is `E0808`, logged, and triggers no build.
fn refuse(provider: Provider, rejection: &Rejection) -> Response {
    tracing::warn!(
        target: "liyasa_server",
        provider = provider.as_str(),
        reason = %rejection,
        "a webhook delivery was refused"
    );
    let status = match rejection {
        Rejection::Replayed { .. } => StatusCode::CONFLICT,
        _ => StatusCode::UNAUTHORIZED,
    };
    Problem::code(status, E0808)
        .detail(rejection.to_string())
        .into_response()
}

fn ignored(kind: &str) -> Response {
    JsonStatus(
        StatusCode::ACCEPTED,
        json!({ "status": "ignored", "reason": kind }),
    )
    .into_response()
}

async fn on_push(state: &DeployState, event: &Event, push: &Push) -> Response {
    let Some(binding) = state.binding_for(&push.repo) else {
        return ignored(&format!("no project is bound to {}", push.repo));
    };
    if push.deleted {
        return ignored("the branch was deleted");
    }
    if !binding.touches(&push.changed) {
        return ignored(&format!("nothing under `{}` changed", binding.root));
    }
    let environment = binding.environment_for(&push.branch);
    let reason = classify(event, &binding.deploy_branch, &binding.trusted_patterns());
    // GIT-20: the previous build of this environment warms the artifact cache,
    // and the commit this push moved from is what `verify --changed` diffs
    // against. Both are best-effort: a first deploy has neither.
    let cache_from = match state.app.store.as_ref() {
        Some(store) => store
            .deployments_typed()
            .current(&binding.project, &environment.name)
            .await
            .ok()
            .flatten()
            .map(|record| record.build.to_string()),
        None => None,
    };
    let mut request = BuildRequest::new(
        binding.project,
        &environment.name,
        Class::for_environment(environment.kind),
        push.repo.full_name(),
        &push.branch,
        &push.head,
    )
    .with_trigger(Trigger::Push)
    .incremental_from(push.before.clone(), cache_from)
    .in_workspace(binding.workspace.clone());
    if reason.untrusted() && environment.kind != EnvironmentKind::Production {
        request = request.untrusted();
    }
    submit(state, binding, &request).await
}

async fn on_pull_request(state: &DeployState, event: &Event, pull: &PullRequestEvent) -> Response {
    let Some(binding) = state.binding_for(&pull.repo) else {
        return ignored(&format!("no project is bound to {}", pull.repo));
    };
    if pull.action.retires() {
        // GIT-30: the preview is deleted per the lifetime policy rather than
        // at once, so a reviewer who follows a link the day after a merge
        // still sees what was reviewed.
        return JsonStatus(
            StatusCode::ACCEPTED,
            json!({
                "status": "retiring",
                "pullRequest": pull.number,
                "retireAt": super::preview::LIFETIME_DAYS,
            }),
        )
        .into_response();
    }
    if !pull.action.builds() {
        return ignored("the pull request action changes nothing to build");
    }
    let reason = classify(event, &binding.deploy_branch, &binding.trusted_patterns());
    let mut request = BuildRequest::new(
        binding.project,
        super::environment::PREVIEW,
        Class::Preview,
        pull.repo.full_name(),
        &pull.head_branch,
        &pull.head_sha,
    )
    .with_trigger(Trigger::PullRequest)
    .for_pull_request(pull.number)
    .in_workspace(binding.workspace.clone());
    if reason.untrusted() {
        request = request.untrusted();
    }
    submit(state, binding, &request).await
}

pub(crate) async fn submit(
    state: &DeployState,
    binding: &Binding,
    request: &BuildRequest,
) -> Response {
    match state.queue.submit(request).await {
        Ok(accepted) => {
            let body = json!({
                "jobId": accepted.id().to_string(),
                "project": request.project.to_string(),
                "env": request.env,
                "branch": request.branch,
                "commit": request.commit,
                "untrusted": request.untrusted,
                "supersededJobId": accepted.superseded().map(|id| id.to_string()),
                "previewHost": request.pull_request.map(|number| {
                    super::environment::pull_request_host(
                        &binding.repo.name,
                        number,
                        &binding.preview_domain,
                    )
                }),
            });
            state.app.notify_webhook("deployment.queued", &body);
            let status = match accepted {
                Accepted::Queued(_) => StatusCode::ACCEPTED,
                Accepted::Superseded { .. } => StatusCode::OK,
            };
            JsonStatus(status, body).into_response()
        }
        Err(QueueError::Full { depth, cap }) => {
            let body = json!({
                "project": request.project.to_string(),
                "depth": depth,
                "cap": cap,
            });
            state.app.notify_webhook("build.queue_full", &body);
            Problem::code(StatusCode::SERVICE_UNAVAILABLE, E0809)
                .detail(format!(
                    "the build queue holds {depth} jobs, at its cap of {cap}"
                ))
                .extension("depth", json!(depth))
                .extension("cap", json!(cap))
                .into_response()
        }
        Err(QueueError::Store(error)) => {
            tracing::error!(target: "liyasa_server", %error, "a build could not be queued");
            Problem::code(StatusCode::INTERNAL_SERVER_ERROR, E0804)
                .detail("the build could not be queued")
                .into_response()
        }
    }
}

/// `GET /_liyasa/hooks` — which providers this server accepts deliveries from,
/// so an operator setting a webhook up can check the URL before saving it.
pub async fn configured(State(state): State<Arc<DeployState>>) -> Response {
    let providers: Vec<&str> = [
        Provider::GitHub,
        Provider::GitLab,
        Provider::Bitbucket,
        Provider::Generic,
    ]
    .into_iter()
    .filter(|provider| state.hooks.verifier(*provider).is_some())
    .map(Provider::as_str)
    .collect();
    Json(json!({ "providers": providers })).into_response()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_codes_this_module_raises_are_the_ones_the_requirements_name() {
        assert_eq!(E0808.as_str(), "E0808", "a refused webhook delivery");
        assert_eq!(E0809.as_str(), "E0809", "a full build queue");
        assert_eq!(E0804.as_str(), "E0804", "a deploy that could not be queued");
    }

    #[test]
    fn a_replay_is_a_conflict_and_a_bad_signature_is_unauthorized() {
        let replayed = refuse(
            Provider::GitHub,
            &Rejection::Replayed {
                id: "d-1".to_owned(),
            },
        );
        assert_eq!(replayed.status(), StatusCode::CONFLICT);
        let bad = refuse(Provider::GitHub, &Rejection::BadSignature);
        assert_eq!(bad.status(), StatusCode::UNAUTHORIZED);
    }

    #[test]
    fn a_header_map_becomes_the_pairs_the_verifier_reads() {
        let mut headers = HeaderMap::new();
        headers.insert("x-github-event", "push".parse().expect("a value"));
        let pairs = header_pairs(&headers);
        assert_eq!(
            pairs,
            vec![("x-github-event".to_owned(), "push".to_owned())]
        );
    }
}
