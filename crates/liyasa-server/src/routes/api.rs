//! REST conventions (REST-11).
//!
//! Cursor pagination, idempotency keys on writes, rate-limit headers, and
//! deprecation headers, in one place so every endpoint spells them the same
//! way.

use std::collections::HashMap;
use std::sync::Mutex;
use std::time::{Duration, Instant};

use axum::response::{IntoResponse, Response};
use http::{HeaderValue, StatusCode, header};
use liyasa_core::store::Page;
use serde::{Deserialize, Serialize};
use serde_json::{Value, json};

pub const DEFAULT_LIMIT: u32 = 50;
pub const MAX_LIMIT: u32 = 200;

/// `?cursor=&limit=`, the only paging any list endpoint accepts.
#[derive(Debug, Clone, Default, Deserialize)]
pub struct PageParams {
    pub cursor: Option<String>,
    pub limit: Option<u32>,
}

impl PageParams {
    pub fn to_page(&self) -> Page {
        Page {
            cursor: self.cursor.clone(),
            limit: self.limit.unwrap_or(DEFAULT_LIMIT).clamp(1, MAX_LIMIT),
        }
    }
}

/// One page of results. `nextCursor` is null on the last page rather than
/// absent, so a client never has to tell the two apart.
#[derive(Debug, Clone, Serialize)]
pub struct Paged<T> {
    pub items: Vec<T>,
    #[serde(rename = "nextCursor")]
    pub next_cursor: Option<String>,
}

impl<T> Paged<T> {
    /// `cursor_of` reads the opaque cursor from the last item; the page is
    /// full only when it reached the requested limit, and only then is there
    /// another page to ask for.
    pub fn new(items: Vec<T>, limit: u32, cursor_of: impl Fn(&T) -> String) -> Self {
        let next_cursor = (items.len() as u32 >= limit)
            .then(|| items.last().map(&cursor_of))
            .flatten();
        Self { items, next_cursor }
    }
}

impl<T: Serialize> IntoResponse for Paged<T> {
    fn into_response(self) -> Response {
        Json(serde_json::to_value(&self).unwrap_or_else(|_| json!({}))).into_response()
    }
}

/// `application/json` with a status. axum's own `Json` always answers 200.
#[derive(Debug, Clone)]
pub struct Json(pub Value);

#[derive(Debug, Clone)]
pub struct JsonStatus(pub StatusCode, pub Value);

impl IntoResponse for Json {
    fn into_response(self) -> Response {
        JsonStatus(StatusCode::OK, self.0).into_response()
    }
}

impl IntoResponse for JsonStatus {
    fn into_response(self) -> Response {
        let body = serde_json::to_vec(&self.1).unwrap_or_else(|_| b"{}".to_vec());
        let mut response = Response::new(body.into());
        *response.status_mut() = self.0;
        response.headers_mut().insert(
            header::CONTENT_TYPE,
            HeaderValue::from_static("application/json"),
        );
        response
    }
}

/// Replays the response a write already produced for an `Idempotency-Key`,
/// so a client that retries a `POST` after a timeout does not create a second
/// record. Per instance and lossy, like every other cache in the serving
/// process (HOST-03).
#[derive(Debug)]
pub struct Idempotency {
    entries: Mutex<HashMap<String, (Instant, u16, Value)>>,
    ttl: Duration,
    capacity: usize,
}

impl Default for Idempotency {
    fn default() -> Self {
        Self {
            entries: Mutex::new(HashMap::new()),
            ttl: Duration::from_secs(24 * 3600),
            capacity: 4096,
        }
    }
}

impl Idempotency {
    /// The key is scoped by route so the same key on two endpoints is two
    /// operations, which is what every implementation of this does.
    fn scoped(route: &str, key: &str) -> String {
        format!("{route}\u{0}{key}")
    }

    pub fn get(&self, route: &str, key: &str) -> Option<(StatusCode, Value)> {
        let mut entries = self.entries.lock().unwrap_or_else(|e| e.into_inner());
        let scoped = Self::scoped(route, key);
        let (at, status, body) = entries.get(&scoped)?;
        if at.elapsed() > self.ttl {
            entries.remove(&scoped);
            return None;
        }
        Some((
            StatusCode::from_u16(*status).unwrap_or(StatusCode::OK),
            body.clone(),
        ))
    }

    pub fn put(&self, route: &str, key: &str, status: StatusCode, body: &Value) {
        let mut entries = self.entries.lock().unwrap_or_else(|e| e.into_inner());
        if entries.len() >= self.capacity {
            entries.retain(|_, (at, _, _)| at.elapsed() <= self.ttl);
            if entries.len() >= self.capacity {
                entries.clear();
            }
        }
        entries.insert(
            Self::scoped(route, key),
            (Instant::now(), status.as_u16(), body.clone()),
        );
    }
}

/// A route that is going away announces it (REST-11). `sunset` is an
/// HTTP-date.
pub fn deprecation(response: &mut Response, sunset: &str, link: &str) {
    let headers = response.headers_mut();
    headers.insert("Deprecation", HeaderValue::from_static("true"));
    if let Ok(value) = HeaderValue::from_str(sunset) {
        headers.insert("Sunset", value);
    }
    if let Ok(value) = HeaderValue::from_str(&format!("<{link}>; rel=\"deprecation\"")) {
        headers.insert("Link", value);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_full_page_offers_a_cursor_and_a_short_one_does_not() {
        let full = Paged::new(vec!["a".to_owned(), "b".to_owned()], 2, |s| s.clone());
        assert_eq!(full.next_cursor.as_deref(), Some("b"));
        let short = Paged::new(vec!["a".to_owned()], 2, |s| s.clone());
        assert_eq!(short.next_cursor, None);
        let empty = Paged::new(Vec::<String>::new(), 2, |s| s.clone());
        assert_eq!(empty.next_cursor, None);
    }

    #[test]
    fn a_limit_is_clamped_rather_than_refused() {
        assert_eq!(PageParams::default().to_page().limit, DEFAULT_LIMIT);
        assert_eq!(
            PageParams {
                limit: Some(100_000),
                ..PageParams::default()
            }
            .to_page()
            .limit,
            MAX_LIMIT
        );
        assert_eq!(
            PageParams {
                limit: Some(0),
                ..PageParams::default()
            }
            .to_page()
            .limit,
            1
        );
    }

    #[test]
    fn a_retried_write_replays_its_first_response() {
        let cache = Idempotency::default();
        assert!(cache.get("/feedback", "k1").is_none());
        cache.put(
            "/feedback",
            "k1",
            StatusCode::CREATED,
            &json!({"id": "fb_1"}),
        );

        let (status, body) = cache.get("/feedback", "k1").expect("the replay");
        assert_eq!(status, StatusCode::CREATED);
        assert_eq!(body["id"], "fb_1");
        assert!(
            cache.get("/other", "k1").is_none(),
            "the same key on another route is another operation"
        );
    }
}
