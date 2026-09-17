//! A recorded HTTP client, so a provider test drives real request and response
//! bytes without a network (GIT-01).
//!
//! A recording is a list of expectations in order. Each names the method and
//! the path it expects and the response it answers with; a call that does not
//! match is a failure rather than a default, because "the request we did not
//! expect" is the bug these tests exist to catch.

use std::sync::Mutex;

use liyasa_core::net::{
    BoxFut, HttpClient, HttpPolicy, HttpRequest, HttpResponse, Method, NetError,
};

#[derive(Debug, Clone)]
pub struct Exchange {
    pub method: Method,
    /// Matched against the request URL's path, exactly.
    pub path: String,
    pub status: u16,
    pub body: String,
}

impl Exchange {
    pub fn get(path: &str, status: u16, body: &str) -> Self {
        Self::new(Method::GET, path, status, body)
    }

    pub fn post(path: &str, status: u16, body: &str) -> Self {
        Self::new(Method::POST, path, status, body)
    }

    pub fn patch(path: &str, status: u16, body: &str) -> Self {
        Self::new(Method::PATCH, path, status, body)
    }

    pub fn new(method: Method, path: &str, status: u16, body: &str) -> Self {
        Self {
            method,
            path: path.to_owned(),
            status,
            body: body.to_owned(),
        }
    }
}

/// What a test asserts against afterwards: every request that was made, in
/// order, with its body.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Made {
    pub method: Method,
    pub url: String,
    pub headers: Vec<(String, String)>,
    pub body: Option<String>,
}

impl Made {
    pub fn json(&self) -> serde_json::Value {
        self.body
            .as_deref()
            .and_then(|text| serde_json::from_str(text).ok())
            .unwrap_or(serde_json::Value::Null)
    }

    pub fn header(&self, name: &str) -> Option<&str> {
        self.headers
            .iter()
            .find(|(key, _)| key.eq_ignore_ascii_case(name))
            .map(|(_, value)| value.as_str())
    }
}

#[derive(Debug)]
pub struct Recorded {
    remaining: Mutex<Vec<Exchange>>,
    made: Mutex<Vec<Made>>,
}

impl Recorded {
    pub fn new(exchanges: Vec<Exchange>) -> Self {
        let mut remaining = exchanges;
        remaining.reverse();
        Self {
            remaining: Mutex::new(remaining),
            made: Mutex::new(Vec::new()),
        }
    }

    pub fn made(&self) -> Vec<Made> {
        self.made
            .lock()
            .expect("the recorder is not poisoned")
            .clone()
    }

    /// How many recorded exchanges were never reached. A test that ends with
    /// leftovers made fewer calls than it meant to.
    pub fn unused(&self) -> usize {
        self.remaining
            .lock()
            .expect("the recorder is not poisoned")
            .len()
    }
}

impl HttpClient for Recorded {
    fn fetch<'a>(
        &'a self,
        req: HttpRequest,
        _policy: &'a HttpPolicy,
    ) -> BoxFut<'a, Result<HttpResponse, NetError>> {
        let made = Made {
            method: req.method.clone(),
            url: req.url.to_string(),
            headers: req.headers.clone(),
            body: req
                .body
                .as_ref()
                .map(|bytes| String::from_utf8_lossy(bytes).into_owned()),
        };
        self.made
            .lock()
            .expect("the recorder is not poisoned")
            .push(made);
        let next = self
            .remaining
            .lock()
            .expect("the recorder is not poisoned")
            .pop();
        Box::pin(async move {
            let Some(expected) = next else {
                return Err(NetError::Io(format!(
                    "no recorded exchange left for {} {}",
                    req.method,
                    req.url.path()
                )));
            };
            if expected.method != req.method || expected.path != req.url.path() {
                return Err(NetError::Io(format!(
                    "expected {} {}, got {} {}",
                    expected.method,
                    expected.path,
                    req.method,
                    req.url.path()
                )));
            }
            Ok(HttpResponse {
                status: expected.status,
                headers: vec![("content-type".to_owned(), "application/json".to_owned())],
                body: expected.body.into_bytes().into(),
                final_url: req.url,
            })
        })
    }
}
