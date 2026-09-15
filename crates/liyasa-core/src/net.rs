//! The outbound-HTTP seam and its policy (PRD §30.2.3, §34.9).
//!
//! Only `liyasa-net` implements this. Policy is enforced at connect time, not
//! by inspecting the URL, so a redirect to a private address is denied on the
//! hop that reaches it.

use std::net::IpAddr;
use std::time::Duration;

use serde::{Deserialize, Serialize};

use crate::vfs::Bytes;

pub type BoxFut<'a, T> = std::pin::Pin<Box<dyn Future<Output = T> + Send + 'a>>;
pub type BoxStream<'a, T> = std::pin::Pin<Box<dyn futures_core::Stream<Item = T> + Send + 'a>>;

pub type Method = http::Method;
pub type Url = url::Url;

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, schemars::JsonSchema)]
#[serde(rename_all = "camelCase")]
pub enum HostPattern {
    Exact(String),
    /// Matches the host itself and any subdomain of it.
    Suffix(String),
    Any,
}

impl HostPattern {
    pub fn matches(&self, host: &str) -> bool {
        let host = host.trim_end_matches('.').to_ascii_lowercase();
        match self {
            Self::Any => true,
            Self::Exact(want) => host == want.to_ascii_lowercase(),
            Self::Suffix(want) => {
                let want = want.trim_start_matches('.').to_ascii_lowercase();
                host == want || host.ends_with(&format!(".{want}"))
            }
        }
    }
}

#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize, schemars::JsonSchema)]
#[serde(transparent)]
pub struct HostSet(pub Vec<HostPattern>);

impl HostSet {
    pub fn matches(&self, host: &str) -> bool {
        self.0.iter().any(|p| p.matches(host))
    }

    pub fn is_empty(&self) -> bool {
        self.0.is_empty()
    }
}

/// Why an outbound call is being made. Every caller declares one so an
/// operator can allow specification fetches without allowing agent fetches.
#[derive(
    Debug,
    Clone,
    Copy,
    PartialEq,
    Eq,
    PartialOrd,
    Ord,
    Hash,
    Serialize,
    Deserialize,
    schemars::JsonSchema,
)]
#[serde(rename_all = "camelCase")]
pub enum Purpose {
    SpecRef,
    FactSource,
    LinkCheck,
    AgentFetch,
    Embed,
    PlaygroundProxy,
    GitProvider,
    ModelProvider,
    Webhook,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct HttpPolicy {
    pub allow_hosts: HostSet,
    pub deny_hosts: HostSet,
    /// Loopback, link-local, and RFC 1918 destinations. Off by default; on only
    /// for an operator who has said so per purpose.
    pub allow_private: bool,
    pub max_redirects: u8,
    pub max_bytes: u64,
    pub timeout: Duration,
    pub purpose: Purpose,
}

#[derive(Debug, Clone)]
pub struct HttpRequest {
    pub method: Method,
    pub url: Url,
    pub headers: Vec<(String, String)>,
    pub body: Option<Bytes>,
}

#[derive(Debug, Clone)]
pub struct HttpResponse {
    pub status: u16,
    pub headers: Vec<(String, String)>,
    pub body: Bytes,
    /// After redirects; what the caller must treat as the base for relative
    /// references.
    pub final_url: Url,
}

#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
pub enum DenyReason {
    #[error("scheme is not https")]
    Scheme,
    #[error("host `{0}` is not in the allow list")]
    HostNotAllowed(String),
    #[error("address {0} is in a denied class")]
    AddressClass(IpAddr),
    #[error("redirect hop {0} left the allow list")]
    RedirectHop(u8),
    #[error("url carries credentials")]
    Credentials,
    #[error("more than {0} redirects")]
    TooManyRedirects(u8),
}

#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
pub enum NetError {
    #[error("blocked by network policy: {reason}")]
    PolicyDenied { reason: DenyReason },
    #[error("timed out")]
    Timeout,
    #[error("response exceeds the configured size cap")]
    TooLarge,
    #[error("dns: {0}")]
    Dns(String),
    #[error("tls: {0}")]
    Tls(String),
    #[error("i/o: {0}")]
    Io(String),
    #[error("http status {0}")]
    Status(u16),
}

pub trait HttpClient: Send + Sync {
    fn fetch<'a>(
        &'a self,
        req: HttpRequest,
        policy: &'a HttpPolicy,
    ) -> BoxFut<'a, Result<HttpResponse, NetError>>;
}
