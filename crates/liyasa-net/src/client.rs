//! The `HttpClient` implementation (PRD §30.2.3).
//!
//! reqwest never follows a redirect and never resolves a name on its own: this
//! crate resolves with hickory, validates every address, pins the validated
//! set for the duration of one hop through a resolver reqwest consults, and
//! repeats that for every `Location` it decides to follow.

use std::collections::HashMap;
use std::net::{IpAddr, SocketAddr};
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};

use hickory_resolver::{Resolver, TokioResolver};
use liyasa_core::net::{
    BoxFut, DenyReason, HostSet, HttpClient, HttpPolicy, HttpRequest, HttpResponse, Method,
    NetError, Url,
};
use tokio::sync::Semaphore;

use crate::policy;

/// Operator-level settings; policy per call comes from `HttpPolicy`.
#[derive(Debug, Clone)]
pub struct ClientOptions {
    /// §30.2.3: connect 5 s.
    pub connect_timeout: Duration,
    pub user_agent: String,
    /// `network.allowInsecureHosts`: hosts an `http://` URL may reach for a
    /// purpose that otherwise requires TLS (CFG-86; `localhost` by default).
    pub allow_insecure_hosts: HostSet,
    /// Connections in flight per host; the rest queue.
    pub per_host_concurrency: usize,
    /// PEM certificates added to the platform roots, for a private CA.
    pub extra_root_certificates: Vec<Vec<u8>>,
}

impl Default for ClientOptions {
    fn default() -> Self {
        Self {
            connect_timeout: Duration::from_secs(5),
            user_agent: concat!("liyasa/", env!("CARGO_PKG_VERSION")).to_owned(),
            allow_insecure_hosts: HostSet(vec![liyasa_core::net::HostPattern::Exact(
                "localhost".to_owned(),
            )]),
            per_host_concurrency: 8,
            extra_root_certificates: Vec::new(),
        }
    }
}

/// The addresses reqwest may connect to right now, keyed by host. An entry
/// exists only while a validated hop is in flight, so a lookup that reqwest
/// makes on its own (it never should) fails rather than resolving.
#[derive(Default)]
struct Pinned(Mutex<HashMap<String, (Vec<SocketAddr>, usize)>>);

impl Pinned {
    fn pin(&self, host: &str, addrs: Vec<SocketAddr>) {
        let mut map = self.0.lock().unwrap_or_else(|e| e.into_inner());
        let entry = map
            .entry(host.to_owned())
            .or_insert_with(|| (Vec::new(), 0));
        entry.0 = addrs;
        entry.1 += 1;
    }

    fn unpin(&self, host: &str) {
        let mut map = self.0.lock().unwrap_or_else(|e| e.into_inner());
        if let Some(entry) = map.get_mut(host) {
            entry.1 = entry.1.saturating_sub(1);
            if entry.1 == 0 {
                map.remove(host);
            }
        }
    }
}

impl reqwest::dns::Resolve for Pinned {
    fn resolve(&self, name: reqwest::dns::Name) -> reqwest::dns::Resolving {
        let host = name.as_str().trim_end_matches('.').to_ascii_lowercase();
        let pinned = {
            let map = self.0.lock().unwrap_or_else(|e| e.into_inner());
            map.get(&host).map(|(addrs, _)| addrs.clone())
        };
        Box::pin(async move {
            match pinned {
                Some(addrs) => {
                    let iter: reqwest::dns::Addrs = Box::new(addrs.into_iter());
                    Ok(iter)
                }
                None => Err(format!("`{host}` was not validated before connecting").into()),
            }
        })
    }
}

pub struct Client {
    http: reqwest::Client,
    resolver: TokioResolver,
    pinned: Arc<Pinned>,
    hosts: Mutex<HashMap<String, Arc<Semaphore>>>,
    options: ClientOptions,
}

impl std::fmt::Debug for Client {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("Client")
            .field("options", &self.options)
            .finish_non_exhaustive()
    }
}

/// Installs ring as the process's TLS provider once; every later call is a
/// no-op, including when another crate installed it first.
pub fn install_crypto_provider() {
    let _ = rustls::crypto::ring::default_provider().install_default();
}

impl Client {
    pub fn new(options: ClientOptions) -> Result<Self, NetError> {
        install_crypto_provider();
        let pinned = Arc::new(Pinned::default());
        let mut builder = reqwest::Client::builder()
            .no_proxy()
            .redirect(reqwest::redirect::Policy::none())
            .connect_timeout(options.connect_timeout)
            .user_agent(options.user_agent.clone())
            .dns_resolver(Arc::clone(&pinned));
        for pem in &options.extra_root_certificates {
            let certificate = reqwest::Certificate::from_pem(pem)
                .map_err(|e| NetError::Tls(e.without_url().to_string()))?;
            builder = builder.add_root_certificate(certificate);
        }
        let http = builder
            .build()
            .map_err(|e| NetError::Io(e.without_url().to_string()))?;
        let resolver = Resolver::builder_tokio()
            .map_err(|e| NetError::Dns(e.to_string()))?
            .build()
            .map_err(|e| NetError::Dns(e.to_string()))?;
        Ok(Self {
            http,
            resolver,
            pinned,
            hosts: Mutex::new(HashMap::new()),
            options,
        })
    }

    fn host_permit(&self, host: &str) -> Arc<Semaphore> {
        let mut hosts = self.hosts.lock().unwrap_or_else(|e| e.into_inner());
        Arc::clone(
            hosts
                .entry(host.to_owned())
                .or_insert_with(|| Arc::new(Semaphore::new(self.options.per_host_concurrency))),
        )
    }

    async fn resolve(&self, url: &Url, deadline: Instant) -> Result<Vec<IpAddr>, NetError> {
        match url.host() {
            Some(url::Host::Ipv4(v4)) => Ok(vec![IpAddr::V4(v4)]),
            Some(url::Host::Ipv6(v6)) => Ok(vec![IpAddr::V6(v6)]),
            Some(url::Host::Domain(name)) => {
                let lookup =
                    tokio::time::timeout_at(deadline.into(), self.resolver.lookup_ip(name))
                        .await
                        .map_err(|_| NetError::Timeout)?
                        .map_err(|e| NetError::Dns(e.to_string()))?;
                let addrs: Vec<IpAddr> = lookup.iter().collect();
                if addrs.is_empty() {
                    return Err(NetError::Dns(format!("`{name}` has no address")));
                }
                Ok(addrs)
            }
            None => Err(NetError::PolicyDenied {
                reason: DenyReason::Scheme,
            }),
        }
    }

    async fn fetch_inner(
        &self,
        req: HttpRequest,
        policy: &HttpPolicy,
    ) -> Result<HttpResponse, NetError> {
        let deadline = Instant::now() + policy.timeout;
        let mut url = req.url;
        let mut method = req.method;
        let mut body = req.body;
        let mut hop: u8 = 0;
        loop {
            let insecure_ok = url.scheme() == "http"
                && url
                    .host_str()
                    .is_some_and(|h| self.options.allow_insecure_hosts.matches(h));
            if let Err(reason) = policy::check_url(&url, policy, hop)
                && !(reason == DenyReason::Scheme && insecure_ok)
            {
                return Err(NetError::PolicyDenied { reason });
            }
            let host = url
                .host_str()
                .ok_or(NetError::PolicyDenied {
                    reason: DenyReason::Scheme,
                })?
                .trim_end_matches('.')
                .to_ascii_lowercase();
            let port = url.port_or_known_default().unwrap_or(443);
            let addrs = self.resolve(&url, deadline).await?;
            policy::check_addresses(&addrs, policy, hop)
                .map_err(|reason| NetError::PolicyDenied { reason })?;
            let key = host
                .trim_start_matches('[')
                .trim_end_matches(']')
                .to_owned();
            self.pinned.pin(
                &key,
                addrs.iter().map(|ip| SocketAddr::new(*ip, port)).collect(),
            );
            let semaphore = self.host_permit(&key);
            let permit = tokio::time::timeout_at(deadline.into(), semaphore.acquire_owned())
                .await
                .map_err(|_| NetError::Timeout)?
                .map_err(|_| NetError::Io("host queue closed".to_owned()))?;

            let mut request = self.http.request(method.clone(), url.clone());
            for (name, value) in &req.headers {
                request = request.header(name, value);
            }
            if let Some(bytes) = &body {
                request = request.body(bytes.clone());
            }
            let sent = tokio::time::timeout_at(deadline.into(), request.send()).await;
            self.pinned.unpin(&key);
            let mut response = match sent {
                Err(_) => {
                    drop(permit);
                    return Err(NetError::Timeout);
                }
                Ok(Err(e)) => {
                    drop(permit);
                    return Err(map_error(e));
                }
                Ok(Ok(r)) => r,
            };
            let status = response.status();
            tracing::debug!(target: "liyasa_net", host = %host, hop, status = status.as_u16(), "fetch");

            if status.is_redirection()
                && let Some(location) = response.headers().get(http::header::LOCATION)
            {
                drop(permit);
                if hop >= policy.max_redirects {
                    return Err(NetError::PolicyDenied {
                        reason: DenyReason::TooManyRedirects(policy.max_redirects),
                    });
                }
                let location = location
                    .to_str()
                    .map_err(|_| NetError::Io("unreadable Location header".to_owned()))?;
                url = url
                    .join(location)
                    .map_err(|e| NetError::Io(format!("bad Location header: {e}")))?;
                hop += 1;
                // RFC 9110 §15.4: 303 always becomes GET; 301 and 302 do for
                // POST (what every browser and reqwest do); 307 and 308 keep
                // the method and body.
                let code = status.as_u16();
                if code == 303 || ((code == 301 || code == 302) && method == Method::POST) {
                    method = Method::GET;
                    body = None;
                }
                continue;
            }

            if let Some(length) = response.content_length()
                && length > policy.max_bytes
            {
                return Err(NetError::TooLarge);
            }
            let mut collected = Vec::new();
            loop {
                let next = tokio::time::timeout_at(deadline.into(), response.chunk()).await;
                match next {
                    Err(_) => return Err(NetError::Timeout),
                    Ok(Err(e)) => return Err(map_error(e)),
                    Ok(Ok(None)) => break,
                    Ok(Ok(Some(chunk))) => {
                        if collected.len() as u64 + chunk.len() as u64 > policy.max_bytes {
                            return Err(NetError::TooLarge);
                        }
                        collected.extend_from_slice(&chunk);
                    }
                }
            }
            drop(permit);
            let headers = response
                .headers()
                .iter()
                .map(|(name, value)| {
                    (
                        name.as_str().to_owned(),
                        String::from_utf8_lossy(value.as_bytes()).into_owned(),
                    )
                })
                .collect();
            return Ok(HttpResponse {
                status: status.as_u16(),
                headers,
                body: collected.into(),
                final_url: url,
            });
        }
    }
}

/// reqwest's errors carry the URL in their message; it is dropped before
/// anything reaches a log line (§30.2.3: never log query strings).
fn map_error(e: reqwest::Error) -> NetError {
    if e.is_timeout() {
        return NetError::Timeout;
    }
    let bare = e.without_url();
    let text = bare.to_string();
    if bare.is_connect() {
        if text.contains("certificate") || text.contains("tls") || text.contains("TLS") {
            NetError::Tls(text)
        } else {
            NetError::Io(text)
        }
    } else {
        NetError::Io(text)
    }
}

impl HttpClient for Client {
    fn fetch<'a>(
        &'a self,
        req: HttpRequest,
        policy: &'a HttpPolicy,
    ) -> BoxFut<'a, Result<HttpResponse, NetError>> {
        Box::pin(self.fetch_inner(req, policy))
    }
}
