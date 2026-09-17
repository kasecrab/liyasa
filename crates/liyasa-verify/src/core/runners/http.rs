//! The `http` runner (VER-02.2, VER-10).
//!
//! An `http` block is a request in HTTP request syntax. It runs against
//! `verify.http.target`: a mock generated from the spec, a staging base URL,
//! or both. Every outbound call goes through `HttpClient`, which is the only
//! socket in the workspace (§6.2, §30.2.3).
//!
//! VER-10's rule about bodies is the shape of this module: the response body
//! is read, the assertions are evaluated against it, and then only pass or
//! fail, a scrubbed excerpt of at most 512 bytes, and a digest survive. The
//! body never reaches the cache, the report, or a log.

use std::sync::Arc;
use std::time::Instant;

use liyasa_core::diagnostics::{Diagnostic, code};
use liyasa_core::net::{
    BoxFut, HttpClient, HttpPolicy, HttpRequest, HttpResponse, Method, NetError, Purpose, Url,
};
use liyasa_core::verify::{
    CheckInput, CheckOutcome, CheckResult, CheckSpec, Expectation, Isolation, Runner, Sandbox,
    SecretSource,
};
use serde_json::Value;

use super::{fail, finish, scrubber_for, skip};
use crate::core::config::{HttpConfig, HttpTarget, StagingTarget};
use crate::core::scrub::Scrubber;
use crate::runners::staging;

/// The in-process mock server VER-10 describes, generated from the OpenAPI
/// document. `liyasa-openapi` builds it; this runner only calls it.
pub trait MockTarget: Send + Sync {
    fn respond<'a>(
        &'a self,
        request: &'a HttpRequest,
    ) -> BoxFut<'a, Result<HttpResponse, Diagnostic>>;
}

pub struct HttpRunner {
    client: Arc<dyn HttpClient>,
    config: HttpConfig,
    mock: Option<Arc<dyn MockTarget>>,
    policy: HttpPolicy,
}

impl HttpRunner {
    pub const ID: &'static str = "http";

    pub fn new(client: Arc<dyn HttpClient>, config: HttpConfig, policy: HttpPolicy) -> Self {
        Self {
            client,
            config,
            mock: None,
            policy,
        }
    }

    #[must_use]
    pub fn with_mock(mut self, mock: Arc<dyn MockTarget>) -> Self {
        self.mock = Some(mock);
        self
    }

    /// The policy an `http` check runs under. `Purpose::FactSource` is the
    /// closest of the nine purposes: a verification request reads a system of
    /// record, it is not a link check and not an agent fetch.
    pub fn default_policy() -> HttpPolicy {
        HttpPolicy {
            allow_hosts: liyasa_core::net::HostSet::default(),
            deny_hosts: liyasa_core::net::HostSet::default(),
            allow_private: false,
            max_redirects: 3,
            max_bytes: 4 * 1024 * 1024,
            timeout: std::time::Duration::from_secs(30),
            purpose: Purpose::FactSource,
        }
    }
}

impl Runner for HttpRunner {
    fn id(&self) -> &'static str {
        Self::ID
    }

    fn languages(&self) -> &'static [&'static str] {
        &["http"]
    }

    fn isolation(&self) -> Isolation {
        Isolation::InProcess
    }

    fn run<'a>(
        &'a self,
        spec: &'a CheckSpec,
        _sandbox: &'a dyn Sandbox,
        secrets: &'a dyn SecretSource,
    ) -> BoxFut<'a, CheckResult> {
        Box::pin(async move {
            let started = Instant::now();
            let scrubber = scrubber_for(spec, secrets);
            let outcome = self.execute(spec, &scrubber, secrets).await;
            finish(spec, Self::ID, outcome, started)
        })
    }
}

impl HttpRunner {
    async fn execute(
        &self,
        spec: &CheckSpec,
        scrubber: &Scrubber,
        secrets: &dyn SecretSource,
    ) -> CheckOutcome {
        let text = match &spec.input {
            CheckInput::Http { request } => request.as_str(),
            CheckInput::Code { lang, source, .. } if lang.eq_ignore_ascii_case("http") => {
                source.as_str()
            }
            CheckInput::Code { lang, .. } => {
                return skip(format!("the `http` runner does not claim `{lang}`"));
            }
            _ => return skip("the `http` runner reads http blocks only"),
        };
        let request = match parse_request(
            text,
            self.config.staging.as_ref().map(|s| s.base_url.as_str()),
        ) {
            Ok(request) => request,
            Err(message) => {
                return CheckOutcome::Error(Diagnostic::new(
                    code::E0601,
                    format!("the `http` block is not a request: {message}"),
                ));
            }
        };

        let mut outcomes = Vec::new();
        for target in self.targets() {
            outcomes.push(
                self.against(target, &request, spec, scrubber, secrets)
                    .await,
            );
        }
        match outcomes.len() {
            0 => skip("no verification target is configured for `http` blocks"),
            _ => combine(outcomes),
        }
    }

    fn targets(&self) -> Vec<HttpTarget> {
        match self.config.target {
            HttpTarget::Mock => vec![HttpTarget::Mock],
            HttpTarget::Staging => vec![HttpTarget::Staging],
            HttpTarget::Both => vec![HttpTarget::Mock, HttpTarget::Staging],
        }
    }

    async fn against(
        &self,
        target: HttpTarget,
        request: &HttpRequest,
        spec: &CheckSpec,
        scrubber: &Scrubber,
        secrets: &dyn SecretSource,
    ) -> CheckOutcome {
        let response = match target {
            HttpTarget::Mock => match &self.mock {
                Some(mock) => mock.respond(request).await.map_err(Failure::Authoring),
                None => {
                    return skip("`verify.http.target` is `mock` and no mock server is wired up");
                }
            },
            HttpTarget::Staging | HttpTarget::Both => {
                let Some(staging) = self.config.staging.as_ref() else {
                    return skip("`verify.http.target` is `staging` and none is configured");
                };
                // The credential goes on the staging arm alone, and only after
                // the request has been cloned for it: a mock server must never
                // be handed a real one, and a recorded fixture must never pick
                // it up.
                let request = match authorized(request, staging, secrets) {
                    Ok(request) => request,
                    Err(diagnostic) => return CheckOutcome::Error(diagnostic),
                };
                self.client
                    .fetch(request, &self.policy)
                    .await
                    .map_err(Failure::Net)
            }
        };
        match response {
            Ok(response) => assert_all(&spec.expect, &response, scrubber),
            Err(Failure::Authoring(diagnostic)) => CheckOutcome::Error(diagnostic),
            Err(Failure::Net(error)) => fail(scrubber, format!("{target:?} target: {error}")),
        }
    }
}

/// The staging request, with `verify.http.staging.auth` resolved onto it.
///
/// The injection itself is `runners::staging` (WP-21); what is decided here is
/// what happens when it cannot be done. Nothing is sent: an unauthenticated
/// request to a staging endpoint answers `401`, and a block asserting
/// `status=401` would then pass having never exercised the credential, which
/// is the defect this call site exists to close.
///
/// The two failures are different problems and carry different codes. A value
/// that is not `secret:<name>` is a setting Liyasa refuses to read, which is
/// `secret_name`'s own `E0635`. Past that point the reference is well formed
/// and the store simply has no such secret, which is `E0637`.
fn authorized(
    request: &HttpRequest,
    staging: &StagingTarget,
    secrets: &dyn SecretSource,
) -> Result<HttpRequest, Diagnostic> {
    staging::secret_name(staging)?;
    let mut request = request.clone();
    match staging::authorize(&mut request, staging, secrets) {
        Ok(_) => Ok(request),
        Err(unresolved) => Err(Diagnostic::new(code::E0637, unresolved.message).help(
            "add it to the secret store, or clear `verify.http.staging.auth`; \
             nothing was sent, so an assertion on `401` would have passed \
             without the credential ever being used",
        )),
    }
}

enum Failure {
    Authoring(Diagnostic),
    Net(NetError),
}

/// `both` means both must pass; the first failure is the one reported.
fn combine(outcomes: Vec<CheckOutcome>) -> CheckOutcome {
    outcomes
        .iter()
        .find(|o| matches!(o, CheckOutcome::Error(_)))
        .or_else(|| {
            outcomes
                .iter()
                .find(|o| matches!(o, CheckOutcome::Fail { .. }))
        })
        .or_else(|| outcomes.iter().find(|o| matches!(o, CheckOutcome::Pass)))
        .cloned()
        .unwrap_or_else(|| {
            outcomes.into_iter().next().unwrap_or(CheckOutcome::Skip {
                reason: "no target ran".to_owned(),
            })
        })
}

fn assert_all(
    expectations: &[Expectation],
    response: &HttpResponse,
    scrubber: &Scrubber,
) -> CheckOutcome {
    let body: Option<Value> = serde_json::from_slice(response.body.as_ref()).ok();
    let mut problems = Vec::new();
    for expectation in expectations {
        match expectation {
            Expectation::Status(want) => {
                if response.status != *want {
                    problems.push(format!("status is {}, not {want}", response.status));
                }
            }
            Expectation::Header { name, value } => {
                let found = response
                    .headers
                    .iter()
                    .find(|(key, _)| key.eq_ignore_ascii_case(name))
                    .map(|(_, v)| v.as_str());
                match found {
                    Some(found) if found == value => {}
                    Some(found) => {
                        problems.push(format!("header `{name}` is `{found}`, not `{value}`"))
                    }
                    None => problems.push(format!("header `{name}` is absent")),
                }
            }
            Expectation::JsonPath { path, value } => match &body {
                None => problems.push(format!("`{path}`: the response body is not JSON")),
                Some(body) => match json_path(body, path) {
                    Some(found) if found == value => {}
                    Some(found) => problems.push(format!("`{path}` is {found}, not {value}")),
                    None => problems.push(format!("`{path}` is not in the response")),
                },
            },
            Expectation::Stdout(text) => {
                let body = String::from_utf8_lossy(response.body.as_ref());
                if !body.contains(text.as_str()) {
                    problems.push(format!("the body does not contain `{text}`"));
                }
            }
            // The spec-derived checks need the OpenAPI document, which reaches
            // this runner through the mock target rather than the expectation.
            Expectation::ResponseSchema { .. }
            | Expectation::Exit(_)
            | Expectation::StdoutFile(_)
            | Expectation::Equals(_)
            | Expectation::Tolerance(_) => {}
            // `Expectation` is `#[non_exhaustive]`; a variant added after this
            // was written asserts nothing here rather than passing silently.
            other => problems.push(format!("this runner does not evaluate {other:?}")),
        }
    }
    if problems.is_empty() {
        CheckOutcome::Pass
    } else {
        fail(scrubber, problems.join("\n"))
    }
}

/// The request syntax VER-10 names: a request line, headers, a blank line,
/// then the body.
fn parse_request(text: &str, base_url: Option<&str>) -> Result<HttpRequest, String> {
    let mut lines = text.lines().map(str::trim_end).skip_while(|l| l.is_empty());
    let request_line = lines.next().ok_or("the block is empty")?;
    let mut parts = request_line.split_whitespace();
    let method = parts.next().ok_or("no method")?;
    let target = parts.next().ok_or("no request target")?;
    let method = Method::from_bytes(method.as_bytes())
        .map_err(|_| format!("`{method}` is not an HTTP method"))?;
    let url = resolve(target, base_url)?;

    let mut headers = Vec::new();
    let mut body = String::new();
    let mut in_body = false;
    for line in lines {
        if !in_body && line.is_empty() {
            in_body = true;
            continue;
        }
        if in_body {
            if !body.is_empty() {
                body.push('\n');
            }
            body.push_str(line);
        } else {
            let (name, value) = line
                .split_once(':')
                .ok_or_else(|| format!("`{line}` is not a header"))?;
            headers.push((name.trim().to_owned(), value.trim().to_owned()));
        }
    }

    Ok(HttpRequest {
        method,
        url,
        headers,
        body: (!body.is_empty()).then(|| liyasa_core::vfs::Bytes::from(body.into_bytes())),
    })
}

fn resolve(target: &str, base_url: Option<&str>) -> Result<Url, String> {
    if let Ok(url) = Url::parse(target)
        && url.has_host()
    {
        return Ok(url);
    }
    let base = base_url.ok_or_else(|| {
        format!("`{target}` is relative and `verify.http.staging.baseUrl` is not set")
    })?;
    Url::parse(base)
        .map_err(|e| format!("`verify.http.staging.baseUrl` is not a URL: {e}"))?
        .join(target)
        .map_err(|e| format!("`{target}` does not join to the base URL: {e}"))
}

/// The JSON-path subset `expect-json="$.id"` uses: `$`, `.key`, `["key"]`, and
/// `[0]`. Filters and wildcards are not read (RFC 1303).
pub fn json_path<'a>(value: &'a Value, path: &str) -> Option<&'a Value> {
    let mut current = value;
    let mut rest = path.trim();
    rest = rest.strip_prefix('$').unwrap_or(rest);
    while !rest.is_empty() {
        if let Some(tail) = rest.strip_prefix('.') {
            let end = tail.find(['.', '[']).unwrap_or(tail.len());
            let (key, next) = tail.split_at(end);
            if key.is_empty() {
                return None;
            }
            current = current.get(key)?;
            rest = next;
        } else {
            let tail = rest.strip_prefix('[')?;
            let end = tail.find(']')?;
            let (inner, next) = tail.split_at(end);
            let inner = inner.trim();
            current = match inner.strip_prefix(['"', '\'']) {
                Some(quoted) => current.get(quoted.trim_end_matches(['"', '\'']))?,
                None => current.get(inner.parse::<usize>().ok()?)?,
            };
            rest = &next[1..];
        }
    }
    Some(current)
}

#[cfg(test)]
mod tests;
