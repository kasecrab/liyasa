//! Problem details, RFC 9457 (REST-11).
//!
//! Every failure the API returns has the same shape, carries the Liyasa error
//! code, and links to that code's documentation page. Nothing that fails is
//! ever an HTML page: an agent parsing the body finds JSON whatever went wrong
//! (AUTH-14).

use axum::response::{IntoResponse, Response};
use http::{HeaderName, HeaderValue, StatusCode, header};
use liyasa_core::diagnostics::Code;
use serde_json::{Map, Value, json};

pub const CONTENT_TYPE: &str = "application/problem+json";

#[derive(Debug, Clone)]
pub struct Problem {
    pub status: StatusCode,
    pub code: Option<Code>,
    pub title: String,
    pub detail: Option<String>,
    pub instance: Option<String>,
    pub extensions: Map<String, Value>,
    pub headers: Vec<(HeaderName, HeaderValue)>,
}

impl Problem {
    pub fn new(status: StatusCode, title: impl Into<String>) -> Self {
        Self {
            status,
            code: None,
            title: title.into(),
            detail: None,
            instance: None,
            extensions: Map::new(),
            headers: Vec::new(),
        }
    }

    /// The common case: a registered code decides the title and the `type`.
    pub fn code(status: StatusCode, code: Code) -> Self {
        Self {
            code: Some(code),
            ..Self::new(status, code.title())
        }
    }

    pub fn detail(mut self, detail: impl Into<String>) -> Self {
        self.detail = Some(detail.into());
        self
    }

    pub fn instance(mut self, instance: impl Into<String>) -> Self {
        self.instance = Some(instance.into());
        self
    }

    pub fn extension(mut self, key: &str, value: Value) -> Self {
        self.extensions.insert(key.to_owned(), value);
        self
    }

    pub fn header(mut self, name: HeaderName, value: HeaderValue) -> Self {
        self.headers.push((name, value));
        self
    }

    pub fn body(&self) -> Value {
        let mut map = Map::new();
        map.insert(
            "type".to_owned(),
            json!(
                self.code
                    .map(|c| c.url())
                    .unwrap_or_else(|| "about:blank".to_owned())
            ),
        );
        map.insert("title".to_owned(), json!(self.title));
        map.insert("status".to_owned(), json!(self.status.as_u16()));
        if let Some(code) = self.code {
            map.insert("code".to_owned(), json!(code.as_str()));
        }
        if let Some(detail) = &self.detail {
            map.insert("detail".to_owned(), json!(detail));
        }
        if let Some(instance) = &self.instance {
            map.insert("instance".to_owned(), json!(instance));
        }
        for (key, value) in &self.extensions {
            map.insert(key.clone(), value.clone());
        }
        Value::Object(map)
    }

    // ---- the failures every route shares ----

    pub fn not_found(what: &str) -> Self {
        Self::new(StatusCode::NOT_FOUND, "Not found").detail(format!("no such {what}"))
    }

    pub fn bad_request(detail: impl Into<String>) -> Self {
        Self::new(StatusCode::BAD_REQUEST, "Bad request").detail(detail)
    }

    pub fn too_large(limit: u64) -> Self {
        Self::new(StatusCode::PAYLOAD_TOO_LARGE, "Request body too large")
            .detail(format!("the limit is {limit} bytes"))
            .extension("limit", json!(limit))
    }

    /// AUTH-14: a refusal is a `429` with `Retry-After`, never a challenge
    /// page and never a `200`.
    pub fn rate_limited(retry_after_seconds: u64) -> Self {
        let code = Code::new("E0807").expect("E0807 is registered");
        Self::code(StatusCode::TOO_MANY_REQUESTS, code)
            .detail("the request rate for this pool is exhausted")
            .extension("retryAfter", json!(retry_after_seconds))
            .header(
                header::RETRY_AFTER,
                HeaderValue::from_str(&retry_after_seconds.to_string())
                    .unwrap_or(HeaderValue::from_static("60")),
            )
    }

    pub fn internal(detail: impl Into<String>) -> Self {
        Self::new(StatusCode::INTERNAL_SERVER_ERROR, "Internal error").detail(detail)
    }

    pub fn store(error: &liyasa_core::store::StoreError) -> Self {
        use liyasa_core::store::StoreError;
        match error {
            StoreError::NotFound => Self::not_found("record"),
            StoreError::Conflict => Self::new(StatusCode::CONFLICT, "Version conflict")
                .detail("the record changed since it was read"),
            other => Self::internal(other.to_string()),
        }
    }
}

impl IntoResponse for Problem {
    fn into_response(self) -> Response {
        let body = serde_json::to_vec(&self.body()).unwrap_or_else(|_| b"{}".to_vec());
        let mut response = Response::new(body.into());
        *response.status_mut() = self.status;
        response
            .headers_mut()
            .insert(header::CONTENT_TYPE, HeaderValue::from_static(CONTENT_TYPE));
        for (name, value) in self.headers {
            response.headers_mut().insert(name, value);
        }
        response
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_problem_carries_its_code_and_the_page_that_explains_it() {
        let problem = Problem::rate_limited(30);
        let body = problem.body();
        assert_eq!(body["status"], 429);
        assert_eq!(body["code"], "E0807");
        assert_eq!(body["retryAfter"], 30);
        assert!(
            body["type"].as_str().expect("a type").ends_with("/E0807"),
            "{body}"
        );
        assert_eq!(
            problem
                .headers
                .iter()
                .find(|(n, _)| n == header::RETRY_AFTER)
                .map(|(_, v)| v.to_str().unwrap_or_default()),
            Some("30")
        );
    }

    #[test]
    fn a_problem_without_a_code_is_still_well_formed() {
        let body = Problem::bad_request("missing `route`").body();
        assert_eq!(body["type"], "about:blank");
        assert_eq!(body["status"], 400);
        assert_eq!(body["detail"], "missing `route`");
        assert!(body.get("code").is_none());
    }
}
