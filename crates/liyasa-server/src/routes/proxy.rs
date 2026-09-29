//! `POST /_liyasa/proxy` — the playground's request forwarder (API-41).
//!
//! The machinery has existed in `liyasa-openapi` since it was written and
//! nothing routed to it: `ProxySource::of`, `allow_list`, `forwardable` and
//! `ProxyEvent` were complete, unit-tested and unreachable, and the rate-limit
//! pool for this prefix pointed at nothing (defect 150). API-41 was unreachable
//! from BOTH ends — no route here and no client in the reader — so mounting
//! this closes one of two gaps and the feature still needs the other half.
//!
//! The envelope is `liyasa_openapi::sample::Request`, the same value every one
//! of the sixteen code generators renders from, agreed with WP-08 for the
//! reason that matters: "copy as curl" and the proxied request are then
//! provably one request. A second envelope would let them drift, and the drift
//! would be invisible to a reader debugging against a curl command that is not
//! what the playground sent.

use std::sync::Arc;
use std::time::Instant;

use axum::Json;
use axum::extract::State;
use axum::response::{IntoResponse, Response};
use http::StatusCode;
use liyasa_core::net::{HostSet, HttpPolicy, HttpRequest, Method, Purpose, Url};
use liyasa_openapi::config::ProxyConfig;
use liyasa_openapi::playground::{self, Deployment, ProxySource};
use liyasa_openapi::sample;

use super::AppState;
use super::problem::Problem;

/// How long a proxied call may take and how much it may return.
const TIMEOUT: std::time::Duration = std::time::Duration::from_secs(30);
const MAX_BYTES: u64 = 8 * 1024 * 1024;

/// What this instance will forward, decided once at startup.
///
/// `allow` is `None` when the answer is "nothing", which is not the same as an
/// empty set and must never compile to the same behaviour: an empty `HostSet`
/// read as "no restriction" is the fork-pull-request hole GIT-31 exists to
/// close. The reason is carried with it because the three causes want three
/// different things from an operator.
#[derive(Debug, Clone)]
pub struct ProxyState {
    pub allow: Option<HostSet>,
    pub refusal: &'static str,
}

impl ProxyState {
    /// Reads the processed specs the build published and derives the hosts.
    ///
    /// The specs come from `dist/openapi/`, which the build writes for
    /// `Audience::public()` — so this reads what every reader can already
    /// fetch, and nothing reader-dependent reaches the allow list.
    pub fn open(dist: &std::path::Path, config: &ProxyConfig, env: &str) -> Self {
        if !config.enabled {
            return Self {
                allow: None,
                refusal: "the playground proxy is disabled for this site",
            };
        }
        let own = Self::source(dist, config);

        // A preview nobody has vouched for gets production's list, never its
        // own `servers` block — otherwise a fork's pull request points the
        // proxy wherever it likes by editing the spec (GIT-31). This process
        // serves one deployment and cannot see production's, so a preview has
        // no list to inherit and forwards nothing.
        let deployment = match env {
            "production" => Deployment::Production,
            _ => Deployment::Preview { trusted: false },
        };
        match playground::allow_list(None, &own, deployment, config) {
            Some(allow) => Self {
                allow: Some(allow),
                refusal: "",
            },
            None if deployment == Deployment::Production => Self {
                allow: None,
                refusal: "no host could be derived from this site's specs or `playground.proxy.allow`",
            },
            None => Self {
                allow: None,
                refusal: "a preview cannot proxy: its allow list comes from the production \
                          deployment, which this process cannot see",
            },
        }
    }

    fn source(dist: &std::path::Path, config: &ProxyConfig) -> ProxySource {
        let mut servers = Vec::new();
        if let Ok(entries) = std::fs::read_dir(dist.join("openapi")) {
            for entry in entries.flatten() {
                let path = entry.path();
                if path.extension().and_then(|e| e.to_str()) != Some("json") {
                    continue;
                }
                let id = path.file_stem().and_then(|s| s.to_str()).unwrap_or("spec");
                let Ok(bytes) = std::fs::read(&path) else {
                    continue;
                };
                match liyasa_openapi::load::from_bytes(id, "", &bytes) {
                    Ok(loaded) => {
                        let source = ProxySource::of(&loaded.spec, config, None);
                        servers.extend(source.servers);
                    }
                    Err(error) => tracing::warn!(
                        target: "liyasa_server",
                        spec = id,
                        reason = %error.message,
                        "a published spec did not load; its servers are not in the proxy's allow list"
                    ),
                }
            }
        }
        ProxySource {
            servers,
            base_url: None,
            configured: config.allow.clone(),
        }
    }
}

/// The wrapper a reader gets back.
///
/// Wrapped rather than verbatim because API-40's viewer shows status, headers,
/// timing and body: latency measured in the browser would include this hop,
/// and a verbatim response cannot distinguish an upstream header from one this
/// proxy added. The analytics event is derived from these same numbers rather
/// than measured separately, so the reader and the event cannot disagree.
#[derive(Debug, serde::Serialize)]
#[serde(rename_all = "camelCase")]
struct Forwarded {
    status: u16,
    headers: Vec<[String; 2]>,
    body: String,
    latency_ms: u64,
}

pub async fn handler(
    State(state): State<Arc<AppState>>,
    Json(request): Json<sample::Request>,
) -> Response {
    // HOST-08 first: an offline instance makes no outbound request of any
    // kind, and this is one. Before the allow list, so the reason an operator
    // is given is the mode they chose rather than a host they did not list.
    if state.config.offline {
        return Problem::new(StatusCode::FORBIDDEN, "the proxy is unavailable")
            .detail("this instance is offline and makes no outbound request (HOST-08)")
            .into_response();
    }
    // Not 404. A 404 here is indistinguishable from the route not existing,
    // which is the state this endpoint was built to end — an operator checking
    // whether their proxy is on would get the same answer either way.
    let Some(proxy) = state.proxy() else {
        return Problem::new(StatusCode::FORBIDDEN, "the proxy forwards nothing")
            .detail("the playground proxy is disabled: this instance composed no proxy state")
            .into_response();
    };
    let Some(allow) = proxy.allow.clone() else {
        return Problem::new(StatusCode::FORBIDDEN, "the proxy forwards nothing")
            .detail(proxy.refusal)
            .into_response();
    };
    let Some(client) = state.http() else {
        return Problem::new(StatusCode::SERVICE_UNAVAILABLE, "the proxy is unavailable")
            .detail("this instance has no outbound client")
            .into_response();
    };

    let Ok(url) = Url::parse(&request.url) else {
        return Problem::new(StatusCode::BAD_REQUEST, "the request is not addressable")
            .detail("`url` is not a URL")
            .into_response();
    };
    let Ok(method) = Method::try_from(request.method.as_str()) else {
        return Problem::new(StatusCode::BAD_REQUEST, "the request is not addressable")
            .detail("`method` is not an HTTP method")
            .into_response();
    };

    // `forwardable` rather than a deny list written here: it is an allow list
    // of what the form produces, and a deny list is a list of the headers
    // somebody thought of. It strips the identity headers and everything
    // `sec-`, and it never ADDS one — so a JSON body's content type comes from
    // the form's own header list, which is what the code sample shows.
    let pairs: Vec<(String, String)> = request
        .headers
        .iter()
        .map(|pair| (pair.name.clone(), pair.value.clone()))
        .collect();
    let headers = playground::forwardable(&pairs);

    let policy = HttpPolicy {
        allow_hosts: allow,
        deny_hosts: HostSet(Vec::new()),
        allow_private: false,
        max_redirects: 0,
        max_bytes: MAX_BYTES,
        timeout: TIMEOUT,
        purpose: Purpose::PlaygroundProxy,
    };
    let outbound = HttpRequest {
        method,
        url,
        headers,
        body: request
            .body
            .as_ref()
            .map(|body| liyasa_core::vfs::Bytes::from(body.text.clone().into_bytes())),
    };

    let started = Instant::now();
    let result = client.fetch(outbound, &policy).await;
    let latency_ms = started.elapsed().as_millis() as u64;

    match result {
        Ok(response) => {
            // ANA-01: an operation id, which class of status came back, and
            // how long it took. No URL, no headers, no body — the type is
            // narrow on purpose and wanting a field for debugging is the
            // constraint working.
            let _event = playground::ProxyEvent::new(
                request.operation_id.clone().unwrap_or_default(),
                response.status,
                latency_ms,
            );
            let forwarded = Forwarded {
                status: response.status,
                headers: response
                    .headers
                    .iter()
                    .map(|(name, value)| [name.clone(), value.clone()])
                    .collect(),
                body: String::from_utf8_lossy(&response.body).into_owned(),
                latency_ms,
            };
            (StatusCode::OK, Json(forwarded)).into_response()
        }
        // The upstream refusing is not this server failing, and a reader needs
        // to see which it was.
        Err(error) => Problem::new(
            StatusCode::BAD_GATEWAY,
            "the upstream call did not complete",
        )
        .detail(error.to_string())
        .into_response(),
    }
}
