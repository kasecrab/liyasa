//! The CLI's async runtime and the one HTTP client that runs on it (RFC 0909).
//!
//! §6.2 puts reqwest in `liyasa-net` and bans it everywhere else, so every
//! outbound request a command makes goes through `liyasa_net::Client`. That
//! client is asynchronous and needs a tokio reactor rather than an executor,
//! which is built here at the call site that needs it: a command that never
//! leaves the machine never constructs a resolver.

use std::path::Path;
use std::time::Duration;

use liyasa_core::diagnostics::{Diagnostic, code};
use liyasa_core::net::{
    HostPattern, HostSet, HttpClient, HttpPolicy, HttpRequest, HttpResponse, Method, NetError,
    Purpose, Url,
};
use liyasa_net::{Client, ClientOptions};
use serde_json::Value;

/// `network.*` (CFG-86), as much of it as a command needs.
#[derive(Debug, Clone, Default)]
pub struct Settings {
    /// Hosts that may be reached at a private address.
    allow_private: HostSet,
    allow_insecure_hosts: HostSet,
    deny_hosts: HostSet,
    /// `network.allowHosts.<purpose>`, empty meaning "any host".
    allow_hosts: Vec<(String, HostSet)>,
}

impl Settings {
    pub fn from_config(config: &Value) -> Self {
        let Some(network) = config.get("network") else {
            return Self::default();
        };
        let mut out = Self {
            allow_private: host_set(network.get("allowPrivate")),
            allow_insecure_hosts: host_set(network.get("allowInsecureHosts")),
            deny_hosts: host_set(network.get("denyHosts")),
            allow_hosts: Vec::new(),
        };
        if let Some(object) = network.get("allowHosts").and_then(Value::as_object) {
            for (key, value) in object {
                out.allow_hosts.push((key.clone(), host_set(Some(value))));
            }
        }
        out
    }

    /// The policy one request runs under.
    ///
    /// `allowPrivate` is a list of hosts and CIDRs while `HttpPolicy` carries a
    /// bool, so the list is matched against the host being asked for and a CIDR
    /// entry never matches. RFC 0910 has the reasoning.
    pub fn policy(&self, purpose: Purpose, url: &Url) -> HttpPolicy {
        let host = url.host_str().unwrap_or_default();
        HttpPolicy {
            allow_hosts: self.allow_for(purpose),
            deny_hosts: self.deny_hosts.clone(),
            allow_private: self.allow_private.matches(host),
            max_redirects: 5,
            max_bytes: DOCUMENT_BYTES,
            timeout: Duration::from_secs(30),
            purpose,
        }
    }

    fn allow_for(&self, purpose: Purpose) -> HostSet {
        let key = purpose_key(purpose);
        self.allow_hosts
            .iter()
            .find(|(name, _)| name == key)
            .map_or_else(HostSet::default, |(_, set)| set.clone())
    }

    fn client_options(&self) -> ClientOptions {
        let mut options = ClientOptions {
            user_agent: format!("liyasa/{}", crate::commands::version::VERSION),
            ..ClientOptions::default()
        };
        if !self.allow_insecure_hosts.is_empty() {
            options.allow_insecure_hosts = self.allow_insecure_hosts.clone();
        }
        options
    }
}

/// The config keys of `network.allowHosts`, which are the wire names of
/// [`Purpose`]. A purpose with no key of its own reaches any host the deny
/// list does not name.
const fn purpose_key(purpose: Purpose) -> &'static str {
    match purpose {
        Purpose::SpecRef => "specRefs",
        Purpose::FactSource => "factSources",
        Purpose::AgentFetch => "agentFetch",
        Purpose::Embed => "embeds",
        Purpose::LinkCheck => "linkCheck",
        Purpose::PlaygroundProxy => "playgroundProxy",
        Purpose::GitProvider => "gitProvider",
        Purpose::ModelProvider => "modelProvider",
        Purpose::Webhook => "webhooks",
    }
}

fn host_set(value: Option<&Value>) -> HostSet {
    let Some(items) = value.and_then(Value::as_array) else {
        return HostSet::default();
    };
    HostSet(
        items
            .iter()
            .filter_map(Value::as_str)
            .map(host_pattern)
            .collect(),
    )
}

/// The spelling `verify.links` already uses: `*` for anything, a leading `*.`
/// or `.` for a domain and its subdomains, anything else exact.
fn host_pattern(text: &str) -> HostPattern {
    match text.trim() {
        "*" => HostPattern::Any,
        rest => match rest.strip_prefix("*.").or_else(|| rest.strip_prefix('.')) {
            Some(suffix) => HostPattern::Suffix(suffix.to_owned()),
            None => HostPattern::Exact(rest.to_owned()),
        },
    }
}

/// The cap on a fetched document: a specification, a release index, a page.
pub const DOCUMENT_BYTES: u64 = 32 * 1024 * 1024;

/// A runtime and the client that runs on it, for the length of one command.
pub struct Network {
    runtime: tokio::runtime::Runtime,
    client: Client,
    settings: Settings,
}

impl Network {
    /// Builds the runtime and the client.
    ///
    /// The client is constructed inside the runtime because its resolver binds
    /// to the reactor it is created under, and `Client::new` outside one panics
    /// rather than returning an error.
    pub fn open(settings: Settings) -> Result<Self, Box<Diagnostic>> {
        // TODO(rfc-0909): a `blocking` feature on `liyasa-net` would put this
        // in the crate that owns the client, once more than one caller wants it.
        let runtime = tokio::runtime::Builder::new_current_thread()
            .enable_all()
            .build()
            .map_err(|error| {
                Box::new(
                    Diagnostic::new(
                        code::E0021,
                        format!("no async runtime for the request: {error}"),
                    )
                    .help("Retry, and run `liyasa doctor` if it keeps happening."),
                )
            })?;
        let options = settings.client_options();
        let client = runtime
            .block_on(async move { Client::new(options) })
            .map_err(|error| Box::new(failed("the HTTP client", &error)))?;
        Ok(Self {
            runtime,
            client,
            settings,
        })
    }

    /// Reads `network.*` out of the project's configuration and opens a client
    /// under it.
    pub fn for_project(config: &Value) -> Result<Self, Box<Diagnostic>> {
        Self::open(Settings::from_config(config))
    }

    pub const fn client(&self) -> &dyn HttpClient {
        &self.client
    }

    pub fn settings(&self) -> &Settings {
        &self.settings
    }

    pub fn block_on<F: Future>(&self, future: F) -> F::Output {
        self.runtime.block_on(future)
    }

    /// One GET, with the policy the purpose and the configuration imply.
    pub fn get(&self, url: &Url, purpose: Purpose) -> Result<HttpResponse, NetError> {
        self.get_within(url, purpose, DOCUMENT_BYTES, Duration::from_secs(30))
    }

    /// A GET for something that is not a document. A release artifact is the
    /// binary itself — CLI-35 budgets it at up to 110 MB — so the caps a
    /// specification is read under would refuse it as too large, and a
    /// thirty-second deadline would refuse it on any ordinary connection.
    pub fn get_within(
        &self,
        url: &Url,
        purpose: Purpose,
        max_bytes: u64,
        timeout: Duration,
    ) -> Result<HttpResponse, NetError> {
        let mut policy = self.settings.policy(purpose, url);
        policy.max_bytes = max_bytes;
        policy.timeout = timeout;
        let request = HttpRequest {
            method: Method::GET,
            url: url.clone(),
            headers: Vec::new(),
            body: None,
        };
        self.block_on(self.client.fetch(request, &policy))
    }

    /// A HEAD, falling back to GET for a host that does not answer one. Used
    /// where only reachability matters.
    pub fn reach(&self, url: &Url, purpose: Purpose) -> Result<u16, NetError> {
        let mut policy = self.settings.policy(purpose, url);
        policy.max_bytes = 64 * 1024;
        policy.timeout = Duration::from_secs(10);
        let head = self.block_on(self.client.fetch(
            HttpRequest {
                method: Method::HEAD,
                url: url.clone(),
                headers: Vec::new(),
                body: None,
            },
            &policy,
        ));
        match head {
            Ok(response) if response.status < 400 => Ok(response.status),
            Ok(_) | Err(NetError::Status(_)) => self
                .block_on(self.client.fetch(
                    HttpRequest {
                        method: Method::GET,
                        url: url.clone(),
                        headers: Vec::new(),
                        body: None,
                    },
                    &policy,
                ))
                .map(|response| response.status),
            Err(error) => Err(error),
        }
    }
}

/// E0021 for a request that did not come back, naming what was being reached.
pub fn failed(what: &str, error: &NetError) -> Diagnostic {
    let help = match error {
        NetError::PolicyDenied { .. } => {
            "Add the host to `network.allowHosts`, or `network.allowPrivate` if it is on this network."
        }
        NetError::Timeout => "Check the connection, or run again with `--offline` to skip it.",
        NetError::TooLarge => "The response is larger than the cap for this request.",
        NetError::Dns(_) => "Check the host name and this machine's resolver.",
        NetError::Tls(_) => {
            "Check the certificate chain; a private CA needs its root in the trust store."
        }
        NetError::Io(_) | NetError::Status(_) => {
            "Run `liyasa doctor` to see what this machine can reach."
        }
    };
    Diagnostic::new(code::E0021, format!("{what} could not be reached: {error}")).help(help)
}

/// The project's configuration as JSON, or `Null` when it cannot be read. The
/// network block is advisory: a config too broken to parse fails elsewhere,
/// with a better diagnostic than this one could give.
pub fn config_value(config: &Path) -> Value {
    std::fs::read_to_string(config)
        .ok()
        .and_then(|text| serde_json::from_str(&text).ok())
        .unwrap_or(Value::Null)
}
