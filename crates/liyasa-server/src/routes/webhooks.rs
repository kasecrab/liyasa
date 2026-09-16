//! Webhook subscriptions and delivery (REST-10, §34.12).
//!
//! One envelope shape for every event type, signed with HMAC-SHA256 over the
//! raw body so a receiver verifies exactly the bytes it read. Deliveries are
//! rows, retried on a fixed ladder, and a subscription that fails twenty
//! times in a row is paused.

use std::sync::Arc;
use std::time::{Duration, SystemTime};

use axum::extract::{Path, State};
use axum::response::{IntoResponse, Response};
use hmac::{Hmac, KeyInit, Mac};
use http::StatusCode;
use liyasa_core::net::{HostSet, HttpClient, HttpPolicy, HttpRequest, Method, Purpose};
use liyasa_store::SqliteStore;
use liyasa_store::records::DeliveryStatus;
use serde::Deserialize;
use serde_json::{Value, json};
use sha2::Sha256;

use super::AppState;
use super::api::{Json, JsonStatus};
use super::httpdate;
use super::problem::Problem;

pub const SCHEMA_VERSION: &str = "1.0";

/// The retry ladder of REST-10: five attempts at 1 min, 5 min, 30 min, 2 h,
/// and 12 h.
pub const RETRIES: &[Duration] = liyasa_store::jobs::BACKOFF;
pub const MAX_ATTEMPTS: u32 = 5;

/// The event types a subscription may ask for (§34.12).
pub const EVENT_TYPES: &[&str] = &[
    "deployment.queued",
    "deployment.started",
    "deployment.succeeded",
    "deployment.failed",
    "deployment.rolled_back",
    "drift.created",
    "drift.updated",
    "drift.resolved",
    "proposal.created",
    "proposal.approved",
    "proposal.rejected",
    "proposal.merged",
    "feedback.received",
    "automation.started",
    "automation.succeeded",
    "automation.failed",
    "build.queue_full",
];

/// One payload. `id` is stable across retries, so a receiver de-duplicates on
/// it; `X-Liyasa-Delivery` is not, so a receiver can tell attempts apart.
pub fn envelope(event_type: &str, data: &Value) -> Value {
    json!({
        "schemaVersion": SCHEMA_VERSION,
        "id": format!("evt_{}", liyasa_store::new_ulid()),
        "type": event_type,
        "occurredAt": httpdate::iso8601(SystemTime::now()),
        "project": Value::Null,
        "data": data,
    })
}

/// `sha256=<hex>` over the raw body, which is what the receiver verifies.
pub fn sign(secret: &str, body: &[u8]) -> String {
    // HMAC accepts a key of any length, so this never fails for a secret the
    // subscription route already length-checked.
    let mut mac = <Hmac<Sha256> as KeyInit>::new_from_slice(secret.as_bytes())
        .unwrap_or_else(|_| <Hmac<Sha256> as KeyInit>::new_from_slice(&[]).expect("an empty key"));
    mac.update(body);
    let tag = mac.finalize().into_bytes();
    format!(
        "sha256={}",
        tag.iter().fold(String::new(), |mut out, b| {
            use std::fmt::Write as _;
            let _ = write!(out, "{b:02x}");
            out
        })
    )
}

/// Constant-time comparison: a receiver's verification is only as good as the
/// comparison, and ours is the reference an operator copies.
pub fn verify(secret: &str, body: &[u8], signature: &str) -> bool {
    let expected = sign(secret, body);
    if expected.len() != signature.len() {
        return false;
    }
    expected
        .bytes()
        .zip(signature.bytes())
        .fold(0u8, |acc, (a, b)| acc | (a ^ b))
        == 0
}

/// How long a delivery waits before its next attempt.
pub fn next_delay(attempt: u32) -> Duration {
    let index = (attempt.max(1) - 1) as usize;
    RETRIES[index.min(RETRIES.len() - 1)]
}

#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SubscribeInput {
    pub url: String,
    pub secret: String,
    #[serde(default)]
    pub events: Vec<String>,
}

fn to_json(subscription: &liyasa_store::records::WebhookSubscription) -> Value {
    json!({
        "id": subscription.id,
        "url": subscription.url,
        "events": subscription.events,
        "active": subscription.active,
        "failures": subscription.failures,
        "createdAt": subscription.created_at,
        // The secret is write-only: it is never read back, on any route.
        "secretSet": !subscription.secret.is_empty(),
    })
}

pub async fn subscribe(
    State(state): State<Arc<AppState>>,
    axum::Json(input): axum::Json<SubscribeInput>,
) -> Response {
    let Some(store) = state.store.clone() else {
        return Problem::not_found("store").into_response();
    };
    let Ok(url) = input.url.parse::<liyasa_core::net::Url>() else {
        return Problem::bad_request("`url` is not a URL").into_response();
    };
    if url.scheme() != "https" {
        return Problem::bad_request("a webhook receiver must be https").into_response();
    }
    if input.secret.len() < 16 {
        return Problem::bad_request("`secret` is at least 16 characters").into_response();
    }
    if let Some(unknown) = input
        .events
        .iter()
        .find(|e| *e != "*" && !EVENT_TYPES.contains(&e.as_str()))
    {
        return Problem::bad_request(format!("unknown event type `{unknown}`")).into_response();
    }
    match store
        .webhooks()
        .subscribe(None, url.as_str(), &input.secret, &input.events)
        .await
    {
        Ok(subscription) => JsonStatus(StatusCode::CREATED, to_json(&subscription)).into_response(),
        Err(error) => Problem::store(&error).into_response(),
    }
}

pub async fn list(State(state): State<Arc<AppState>>) -> Response {
    let Some(store) = state.store.clone() else {
        return Problem::not_found("store").into_response();
    };
    match store.webhooks().subscriptions(None).await {
        Ok(rows) => Json(json!({
            "items": rows.iter().map(to_json).collect::<Vec<_>>(),
            "nextCursor": Value::Null,
        }))
        .into_response(),
        Err(error) => Problem::store(&error).into_response(),
    }
}

pub async fn unsubscribe(State(state): State<Arc<AppState>>, Path(id): Path<String>) -> Response {
    let Some(store) = state.store.clone() else {
        return Problem::not_found("store").into_response();
    };
    match store.webhooks().unsubscribe(&id).await {
        Ok(()) => StatusCode::NO_CONTENT.into_response(),
        Err(liyasa_core::store::StoreError::NotFound) => {
            Problem::not_found("subscription").into_response()
        }
        Err(error) => Problem::store(&error).into_response(),
    }
}

/// Sends every delivery that is due. Returns how many were attempted, so a
/// test can drive the worker a batch at a time rather than waiting on a
/// timer.
pub async fn deliver_due(
    store: &SqliteStore,
    http: &dyn HttpClient,
    limit: i64,
) -> Result<usize, liyasa_core::store::StoreError> {
    let due = store.webhooks().due(limit).await?;
    let mut attempted = 0;
    for delivery in due {
        let Some(subscription) = store
            .webhooks()
            .get_subscription(&delivery.subscription)
            .await?
        else {
            continue;
        };
        let body = delivery.payload.clone().into_bytes();
        let signature = sign(&subscription.secret, &body);
        let timestamp = liyasa_store::now_ms() / 1000;
        let request = HttpRequest {
            method: Method::POST,
            url: match subscription.url.parse() {
                Ok(url) => url,
                Err(_) => continue,
            },
            headers: vec![
                ("content-type".to_owned(), "application/json".to_owned()),
                ("x-liyasa-signature".to_owned(), signature),
                ("x-liyasa-timestamp".to_owned(), timestamp.to_string()),
                (
                    "x-liyasa-delivery".to_owned(),
                    format!("{}-{}", delivery.id, delivery.attempt),
                ),
                ("x-liyasa-event".to_owned(), delivery.event_type.clone()),
            ],
            body: Some(body.into()),
        };
        let policy = HttpPolicy {
            allow_hosts: HostSet::default(),
            deny_hosts: HostSet::default(),
            allow_private: false,
            max_redirects: 0,
            max_bytes: 64 * 1024,
            timeout: Duration::from_secs(30),
            purpose: Purpose::Webhook,
        };
        attempted += 1;
        let result = http.fetch(request, &policy).await;
        let (status, outcome) = match result {
            Ok(response) if (200..300).contains(&response.status) => {
                (Some(response.status), DeliveryStatus::Delivered)
            }
            Ok(response) => (Some(response.status), retry_or_fail(delivery.attempt)),
            Err(error) => {
                tracing::warn!(target: "liyasa_server", %error, "a webhook delivery failed");
                (None, retry_or_fail(delivery.attempt))
            }
        };
        let next_at = if outcome == DeliveryStatus::Pending {
            liyasa_store::now_ms() + next_delay(delivery.attempt + 1).as_millis() as i64
        } else {
            liyasa_store::now_ms()
        };
        store
            .webhooks()
            .record_attempt(
                &delivery.id,
                &delivery.subscription,
                status,
                outcome,
                next_at,
            )
            .await?;
    }
    Ok(attempted)
}

fn retry_or_fail(attempt: u32) -> DeliveryStatus {
    if attempt + 1 >= MAX_ATTEMPTS {
        DeliveryStatus::Failed
    } else {
        DeliveryStatus::Pending
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn an_envelope_carries_the_documented_fields() {
        let payload = envelope("deployment.succeeded", &json!({ "buildId": "blake3:ab" }));
        assert_eq!(payload["schemaVersion"], SCHEMA_VERSION);
        assert_eq!(payload["type"], "deployment.succeeded");
        assert!(payload["id"].as_str().expect("an id").starts_with("evt_"));
        assert!(
            payload["occurredAt"]
                .as_str()
                .expect("a timestamp")
                .ends_with('Z')
        );
        assert_eq!(payload["data"]["buildId"], "blake3:ab");
    }

    #[test]
    fn a_signature_is_over_the_raw_body_and_verifies_only_that_body() {
        let body = br#"{"id":"evt_1"}"#;
        let signature = sign("a-secret-of-some-length", body);
        assert!(signature.starts_with("sha256="));
        assert!(verify("a-secret-of-some-length", body, &signature));
        assert!(!verify("another-secret-value", body, &signature));
        assert!(!verify("a-secret-of-some-length", b"{}", &signature));
        assert!(!verify("a-secret-of-some-length", body, "sha256=00"));
    }

    #[test]
    fn a_known_vector_pins_the_signature_format() {
        // A receiver in any language must compute this from the same inputs.
        // Cross-checked against Python's `hmac.new(b"key", b"body",
        // hashlib.sha256).hexdigest()`.
        assert_eq!(
            sign("key", b"body"),
            "sha256=515aae133b435d4000956731f68ae5cf5eb85d4f0dc6a546d2bfcd3595ec1ae1"
        );
    }

    #[test]
    fn the_ladder_is_one_minute_to_twelve_hours_over_five_attempts() {
        let minutes = |d: Duration| d.as_secs() / 60;
        assert_eq!(minutes(next_delay(1)), 1);
        assert_eq!(minutes(next_delay(5)), 720);
        assert_eq!(retry_or_fail(0), DeliveryStatus::Pending);
        assert_eq!(
            retry_or_fail(MAX_ATTEMPTS - 1),
            DeliveryStatus::Failed,
            "the last attempt does not schedule another"
        );
    }

    #[test]
    fn a_subscription_never_reads_its_secret_back() {
        let subscription = liyasa_store::records::WebhookSubscription {
            id: "whs_1".to_owned(),
            project: None,
            url: "https://example.com/hook".to_owned(),
            secret: "the-secret-value".to_owned(),
            events: vec!["deployment.succeeded".to_owned()],
            active: true,
            failures: 0,
            created_at: 1,
            updated_at: 1,
        };
        let row = to_json(&subscription);
        assert_eq!(row["secretSet"], true);
        assert!(!row.to_string().contains("the-secret-value"), "{row}");
    }
}
