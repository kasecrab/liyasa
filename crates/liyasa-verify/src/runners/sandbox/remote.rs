//! The `remote` sandbox (VER-03).
//!
//! "A Liyasa runner service over HTTPS, managed in the cloud." The wire
//! format is not in the PRD; RFC 2102 fixes the one below and keeps it in one
//! place so changing it is one file.
//!
//! Every byte leaves through `HttpClient`, which is the workspace's only
//! socket (§30.2.3), and the policy this module builds allows exactly the
//! service's own host.

use std::sync::Arc;
use std::time::Duration;

use liyasa_core::net::{
    BoxFut, HostPattern, HostSet, HttpClient, HttpPolicy, HttpRequest, Method, NetError, Purpose,
    Url,
};
use liyasa_core::verify::{Sandbox, SandboxError, SandboxJob, SandboxOutput};
use liyasa_core::vfs::Bytes;
use serde::{Deserialize, Serialize};

/// Where jobs are posted, and what authorises them.
pub struct RemoteService {
    client: Arc<dyn HttpClient>,
    url: Url,
    /// Sent as `Authorization: Bearer …`. Held zeroing, and never in a job,
    /// an excerpt, or a digest.
    token: Option<zeroize::Zeroizing<String>>,
}

impl RemoteService {
    pub fn new(client: Arc<dyn HttpClient>, url: Url) -> Self {
        Self {
            client,
            url,
            token: None,
        }
    }

    #[must_use]
    pub fn with_token(mut self, token: impl Into<String>) -> Self {
        self.token = Some(zeroize::Zeroizing::new(token.into()));
        self
    }

    /// The service's own host and nothing else, so a redirect off it is denied
    /// on the hop that reaches it rather than followed.
    fn policy(&self, timeout: Duration) -> HttpPolicy {
        HttpPolicy {
            allow_hosts: HostSet(
                self.url
                    .host_str()
                    .map(|host| vec![HostPattern::Exact(host.to_owned())])
                    .unwrap_or_default(),
            ),
            deny_hosts: HostSet::default(),
            allow_private: false,
            max_redirects: 0,
            max_bytes: 16 * 1024 * 1024,
            timeout,
            purpose: Purpose::FactSource,
        }
    }
}

pub struct RemoteSandbox {
    service: RemoteService,
}

impl RemoteSandbox {
    pub fn new(service: RemoteService) -> Self {
        Self { service }
    }
}

impl Sandbox for RemoteSandbox {
    fn exec<'a>(&'a self, job: SandboxJob) -> BoxFut<'a, Result<SandboxOutput, SandboxError>> {
        Box::pin(async move {
            if job.digest.is_empty() {
                return Err(SandboxError::Image(job.image.clone()));
            }
            let mut headers = vec![("content-type".to_owned(), "application/json".to_owned())];
            if let Some(token) = &self.service.token {
                headers.push(("authorization".to_owned(), format!("Bearer {}", **token)));
            }
            let body = serde_json::to_vec(&Request::from(&job))
                .map_err(|error| SandboxError::Io(error.to_string()))?;
            let request = HttpRequest {
                method: Method::POST,
                url: self.service.url.clone(),
                headers,
                body: Some(Bytes::from(body)),
            };
            // The service enforces the job's own timeout; ours is longer so a
            // service that answers late is a late answer rather than a
            // Liyasa-side timeout that hides it.
            let policy = self.service.policy(job.timeout + GRACE);
            let response = self
                .service
                .client
                .fetch(request, &policy)
                .await
                .map_err(net)?;
            if response.status == 408 || response.status == 504 {
                return Err(SandboxError::Timeout);
            }
            if !(200..300).contains(&response.status) {
                return Err(SandboxError::Io(format!(
                    "the runner service answered {}",
                    response.status
                )));
            }
            let answer: Response = serde_json::from_slice(response.body.as_ref())
                .map_err(|error| SandboxError::Io(format!("the runner service: {error}")))?;
            answer.into_output()
        })
    }
}

const GRACE: Duration = Duration::from_secs(10);

fn net(error: NetError) -> SandboxError {
    match error {
        NetError::Timeout => SandboxError::Timeout,
        other => SandboxError::Io(other.to_string()),
    }
}

#[derive(Debug, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
struct Request {
    image: String,
    digest: String,
    cmd: Vec<String>,
    files: Vec<File>,
    env: Vec<Var>,
    timeout_ms: u64,
    network: bool,
    cpu_millis: u32,
    mem_bytes: u64,
}

#[derive(Debug, Serialize, Deserialize)]
struct File {
    path: String,
    /// base64, because a fixture is not always text.
    content: String,
}

#[derive(Debug, Serialize, Deserialize)]
struct Var {
    name: String,
    value: String,
}

impl From<&SandboxJob> for Request {
    fn from(job: &SandboxJob) -> Self {
        Self {
            image: job.image.clone(),
            digest: job.digest.clone(),
            cmd: job.cmd.clone(),
            files: job
                .files
                .iter()
                .map(|(path, bytes)| File {
                    path: path.as_str().to_owned(),
                    content: base64::encode(bytes.as_ref()),
                })
                .collect(),
            env: job
                .env
                .iter()
                .map(|(name, value)| Var {
                    name: name.clone(),
                    value: value.clone(),
                })
                .collect(),
            timeout_ms: job.timeout.as_millis().min(u128::from(u64::MAX)) as u64,
            network: job.network,
            cpu_millis: job.cpu_millis,
            mem_bytes: job.mem_bytes,
        }
    }
}

#[derive(Debug, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
struct Response {
    exit: i32,
    #[serde(default)]
    stdout: String,
    #[serde(default)]
    stderr: String,
    #[serde(default)]
    duration_ms: u64,
    /// The service's own word for "this job outran its timeout".
    #[serde(default)]
    timed_out: bool,
}

impl Response {
    fn into_output(self) -> Result<SandboxOutput, SandboxError> {
        if self.timed_out {
            return Err(SandboxError::Timeout);
        }
        Ok(SandboxOutput {
            exit: self.exit,
            stdout: Bytes::from(decode(&self.stdout)?),
            stderr: Bytes::from(decode(&self.stderr)?),
            duration: Duration::from_millis(self.duration_ms),
        })
    }
}

fn decode(text: &str) -> Result<Vec<u8>, SandboxError> {
    base64::decode(text).ok_or_else(|| {
        SandboxError::Io("the runner service sent output Liyasa cannot decode".to_owned())
    })
}

/// Standard base64 with padding. Thirty lines rather than a dependency row in
/// the PRD's table for one wire format.
mod base64 {
    const ALPHABET: &[u8; 64] = b"ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789+/";

    pub fn encode(bytes: &[u8]) -> String {
        let mut out = String::with_capacity(bytes.len().div_ceil(3) * 4);
        for chunk in bytes.chunks(3) {
            let b = [
                chunk[0],
                *chunk.get(1).unwrap_or(&0),
                *chunk.get(2).unwrap_or(&0),
            ];
            let n = (u32::from(b[0]) << 16) | (u32::from(b[1]) << 8) | u32::from(b[2]);
            for i in 0..4 {
                if i <= chunk.len() {
                    out.push(ALPHABET[(n >> (18 - 6 * i)) as usize & 63] as char);
                } else {
                    out.push('=');
                }
            }
        }
        out
    }

    pub fn decode(text: &str) -> Option<Vec<u8>> {
        let mut out = Vec::with_capacity(text.len() / 4 * 3);
        let mut acc = 0u32;
        let mut bits = 0u8;
        for byte in text.bytes() {
            if byte == b'=' || byte.is_ascii_whitespace() {
                continue;
            }
            let value = ALPHABET.iter().position(|c| *c == byte)? as u32;
            acc = (acc << 6) | value;
            bits += 6;
            if bits >= 8 {
                bits -= 8;
                out.push((acc >> bits) as u8);
            }
        }
        Some(out)
    }
}

#[cfg(test)]
mod tests;
