//! The source kinds, as one [`TruthSource`] that dispatches on the declaration
//! (VER-22, VER-24, VER-25, VER-26).
//!
//! The kinds differ in where the bytes come from and in what they are allowed
//! to do to get them; what happens afterwards — schema, pointers, types,
//! scrubbing, digest — is the same for all of them, so it is written once.
//!
//! Three of the rules here are refusals rather than features, and each is a
//! refusal because the alternative reports success while being wrong: a
//! `command` the server's allow list does not name is not run (`E0621`), a
//! plain-HTTP fetch is not made (`E0806`), and an untrusted build does not
//! refresh anything that leaves the machine (VER-25).

use std::collections::BTreeMap;
use std::sync::Arc;
use std::time::{Duration, SystemTime};

use liyasa_core::ai::TrustLevel;
use liyasa_core::diagnostics::{Diagnostic, code};
use liyasa_core::ids::FactId;
use liyasa_core::net::{BoxFut, HostSet, HttpClient, HttpPolicy, HttpRequest, Method, Purpose};
use liyasa_core::verify::{
    FactValue, Sandbox, SandboxJob, SecretSource, Snapshot, SourceError, SourceKind, TruthSource,
};
use liyasa_core::vfs::{Vfs, VfsPath};
use serde_json::Value;

use crate::core::config::AllowedCommand;
use crate::core::scrub::Scrubber;

use super::snapshot;
use super::spec::{SourceSpec, kind_name, read_facts};
use super::trust::{TransportPolicy, trust_of};

/// How much a `command` source may use. `verify.budget.perCheck` bounds a
/// check; a refresh is not a check, so it carries its own.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SandboxLimits {
    pub image: String,
    pub digest: String,
    pub timeout: Duration,
    /// A refresh that fetches over the network is a `url` source. A `command`
    /// gets no network unless the operator's image needs one.
    pub network: bool,
    pub cpu_millis: u32,
    pub mem_bytes: u64,
}

impl Default for SandboxLimits {
    fn default() -> Self {
        Self {
            image: String::new(),
            digest: String::new(),
            timeout: Duration::from_secs(60),
            network: false,
            cpu_millis: 0,
            mem_bytes: 0,
        }
    }
}

/// Whether this build may reach outside the machine (VER-25).
///
/// A fork pull request and a branch outside `verify.sources.trustedBranches`
/// are `Untrusted`, and an untrusted build never refreshes a `url`, `command`
/// or `screenshot` source — [`refresh`](super::refresh) hands it the latest
/// production snapshot instead.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum BuildTrust {
    #[default]
    Trusted,
    Untrusted,
}

impl BuildTrust {
    /// VER-25's rule, by branch. A fork pull request is `Untrusted` whatever
    /// its branch is called, so the caller passes `fork = true` for one.
    pub fn of(branch: &str, fork: bool, trusted: &[String]) -> Self {
        if fork || !trusted.iter().any(|name| name == branch) {
            Self::Untrusted
        } else {
            Self::Trusted
        }
    }

    pub fn is_trusted(self) -> bool {
        self == Self::Trusted
    }
}

/// One declared source, ready to refresh.
pub struct DeclaredSource {
    spec: SourceSpec,
    trust: TrustLevel,
    vfs: Option<Arc<dyn Vfs>>,
    secrets: Option<Arc<dyn SecretSource>>,
    transport: TransportPolicy,
    http: HttpPolicy,
    /// VER-25: the server's own allow list. Never the repository's.
    allow: Vec<AllowedCommand>,
    limits: SandboxLimits,
    build: BuildTrust,
    /// `None` reads the wall clock at refresh time.
    taken_at: Option<SystemTime>,
}

impl DeclaredSource {
    pub fn new(spec: SourceSpec) -> Self {
        let trust = trust_of(&spec);
        Self {
            spec,
            trust,
            vfs: None,
            secrets: None,
            transport: TransportPolicy::default(),
            http: fact_source_policy(),
            allow: Vec::new(),
            limits: SandboxLimits::default(),
            build: BuildTrust::Trusted,
            taken_at: None,
        }
    }

    #[must_use]
    pub fn with_vfs(mut self, vfs: Arc<dyn Vfs>) -> Self {
        self.vfs = Some(vfs);
        self
    }

    #[must_use]
    pub fn with_secrets(mut self, secrets: Arc<dyn SecretSource>) -> Self {
        self.secrets = Some(secrets);
        self
    }

    #[must_use]
    pub fn with_transport(mut self, transport: TransportPolicy) -> Self {
        self.transport = transport;
        self
    }

    #[must_use]
    pub fn with_http_policy(mut self, http: HttpPolicy) -> Self {
        self.http = http;
        self
    }

    #[must_use]
    pub fn with_allow_list(mut self, allow: Vec<AllowedCommand>) -> Self {
        self.allow = allow;
        self
    }

    #[must_use]
    pub fn with_limits(mut self, limits: SandboxLimits) -> Self {
        self.limits = limits;
        self
    }

    #[must_use]
    pub fn with_build_trust(mut self, build: BuildTrust) -> Self {
        self.build = build;
        self
    }

    /// Fixes the clock a snapshot is stamped with, so a caller with a build
    /// clock does not have to take the wall one.
    #[must_use]
    pub fn taken_at(mut self, at: SystemTime) -> Self {
        self.taken_at = Some(at);
        self
    }

    pub fn spec(&self) -> &SourceSpec {
        &self.spec
    }

    /// Whether refreshing this source reaches outside the machine, which is
    /// what VER-25 forbids an untrusted build.
    pub fn leaves_the_machine(&self) -> bool {
        matches!(
            self.spec.kind,
            SourceKind::Url | SourceKind::Command | SourceKind::Screenshot
        ) || (self.spec.kind == SourceKind::OpenApi && self.spec.url.is_some())
    }

    /// VER-24: how an attestation stands against the clock.
    pub fn attestation(&self, now: SystemTime) -> Attestation {
        attestation_of(&self.spec, now)
    }

    fn secret(&self) -> Option<zeroize::Zeroizing<String>> {
        let auth = self.spec.auth.as_ref()?;
        self.secrets.as_ref()?.get(&auth.secret)
    }

    /// The scrubber a snapshot of this source is built with: whatever
    /// credential it was handed, so a document echoing it back cannot carry it
    /// into the snapshot (§30.2.4).
    fn scrubber(&self) -> Scrubber {
        match self.secret() {
            Some(value) => Scrubber::with_secrets([value.as_str().to_owned()]),
            None => Scrubber::new(),
        }
    }

    fn refuse(&self, why: impl Into<String>) -> SourceError {
        SourceError::Policy(format!("`{}`: {}", self.spec.id, why.into()))
    }

    fn read_vfs(&self, path: &str) -> Result<Vec<u8>, SourceError> {
        let vfs = self
            .vfs
            .as_ref()
            .ok_or_else(|| self.refuse("no file system was provided to read it from"))?;
        vfs.read(&VfsPath::new(path))
            .map(|bytes| bytes.to_vec())
            .map_err(|why| self.refuse(format!("`{path}` could not be read: {why}")))
    }

    /// The document, parsed. JSON and YAML only: TOML would be a dependency
    /// this crate does not have, and a fact table in TOML can be converted.
    fn parse_document(&self, path: &str, bytes: &[u8]) -> Result<Value, SourceError> {
        let text = std::str::from_utf8(bytes)
            .map_err(|_| self.refuse(format!("`{path}` is not UTF-8")))?;
        let lower = path.to_ascii_lowercase();
        if lower.ends_with(".json") {
            serde_json::from_str(text)
                .map_err(|why| self.refuse(format!("`{path}` is not JSON: {why}")))
        } else if lower.ends_with(".yaml") || lower.ends_with(".yml") {
            serde_norway::from_str(text)
                .map_err(|why| self.refuse(format!("`{path}` is not YAML: {why}")))
        } else {
            Err(self.refuse(format!(
                "`{path}` is neither `.json` nor `.yaml`, and those are the formats a fact document is read in"
            )))
        }
    }

    /// The document a `url` or fetched `openapi` source returns.
    async fn fetch(&self, http: &dyn HttpClient) -> Result<Value, SourceError> {
        let url = self
            .transport
            .check(&self.spec)
            .map_err(|refusal| SourceError::Policy(refusal.message.clone()))?;
        let mut headers = Vec::new();
        if let Some(auth) = &self.spec.auth {
            let value = self.secret().ok_or_else(|| {
                self.refuse(format!(
                    "`auth` names `{}` and no such secret was provided",
                    auth.secret
                ))
            })?;
            headers.push((
                auth.header.to_ascii_lowercase(),
                auth.format.replace("{}", value.as_str()),
            ));
        }
        let response = http
            .fetch(
                HttpRequest {
                    method: Method::GET,
                    url,
                    headers,
                    body: None,
                },
                &self.http,
            )
            .await?;
        if !(200..300).contains(&response.status) {
            return Err(self.refuse(format!("the source answered {}", response.status)));
        }
        let text = std::str::from_utf8(&response.body)
            .map_err(|_| self.refuse("the source's answer is not UTF-8"))?;
        serde_json::from_str(text)
            .map_err(|why| self.refuse(format!("the source's answer is not JSON: {why}")))
    }

    /// VER-25: a `command` runs only if the *server's* configuration names its
    /// script path and its content hash, and only in the sandbox.
    async fn run_command(&self, sandbox: Option<&dyn Sandbox>) -> Result<Value, SourceError> {
        let path =
            self.spec.path.as_deref().ok_or_else(|| {
                self.refuse("a `command` source names the script it runs in `path`")
            })?;
        let script = self.read_vfs(path)?;
        self.check_allow_list(path, &script)?;

        let sandbox = sandbox.ok_or_else(|| {
            self.refuse("a `command` source runs in the sandbox, and none was provided")
        })?;
        let mut env = Vec::new();
        if let Some(auth) = &self.spec.auth {
            let value = self.secret().ok_or_else(|| {
                self.refuse(format!(
                    "`auth` names `{}` and no such secret was provided",
                    auth.secret
                ))
            })?;
            env.push((
                auth.secret.clone(),
                auth.format.replace("{}", value.as_str()),
            ));
        }
        let output = sandbox
            .exec(SandboxJob {
                image: self.limits.image.clone(),
                digest: self.limits.digest.clone(),
                cmd: self.spec.command.clone(),
                files: vec![(VfsPath::new(path), script.into())],
                env,
                timeout: self.limits.timeout,
                network: self.limits.network,
                cpu_millis: self.limits.cpu_millis,
                mem_bytes: self.limits.mem_bytes,
            })
            .await?;
        if output.exit != 0 {
            return Err(self.refuse(format!("the command exited {}", output.exit)));
        }
        let text = std::str::from_utf8(&output.stdout)
            .map_err(|_| self.refuse("the command's output is not UTF-8"))?;
        serde_json::from_str(text)
            .map_err(|why| self.refuse(format!("the command's output is not JSON: {why}")))
    }

    /// `E0621`: not on the list, or on it with a different hash.
    fn check_allow_list(&self, path: &str, script: &[u8]) -> Result<(), SourceError> {
        let Some(entry) = self.allow.iter().find(|entry| entry.path == path) else {
            return Err(SourceError::Policy(
                Diagnostic::new(
                    code::E0621,
                    format!(
                        "`{}` runs `{path}`, which `verify.sources.commands.allow` does not name",
                        self.spec.id
                    ),
                )
                .message,
            ));
        };
        let actual = sha256_hex(script);
        if !entry.sha256.eq_ignore_ascii_case(&actual) {
            return Err(SourceError::Policy(
                Diagnostic::new(
                    code::E0621,
                    format!(
                        "`{path}` hashes to {actual} and `verify.sources.commands.allow` names {}",
                        entry.sha256
                    ),
                )
                .message,
            ));
        }
        Ok(())
    }

    /// The document this source produces, whatever it takes to get it.
    async fn document(
        &self,
        http: &dyn HttpClient,
        sandbox: Option<&dyn Sandbox>,
    ) -> Result<Value, SourceError> {
        if self.leaves_the_machine() && !self.build.is_trusted() {
            return Err(self.refuse(format!(
                "a `{}` source is not refreshed by an untrusted build; the latest production snapshot is used instead",
                kind_name(self.spec.kind)
            )));
        }
        match self.spec.kind {
            SourceKind::File | SourceKind::Repo => {
                let path = self
                    .spec
                    .path
                    .clone()
                    .ok_or_else(|| self.refuse("names no `path`"))?;
                let bytes = self.read_vfs(&path)?;
                self.parse_document(&path, &bytes)
            }
            SourceKind::Url => self.fetch(http).await,
            SourceKind::OpenApi => match (&self.spec.url, &self.spec.path) {
                (Some(_), _) => self.fetch(http).await,
                (None, Some(path)) => {
                    let path = path.clone();
                    let bytes = self.read_vfs(&path)?;
                    self.parse_document(&path, &bytes)
                }
                (None, None) => Err(self.refuse("names neither a `url` nor a `path`")),
            },
            SourceKind::Command => self.run_command(sandbox).await,
            SourceKind::Manual => Ok(Value::Object(
                self.spec
                    .values
                    .iter()
                    .map(|(fact, value)| (fact.as_str().to_owned(), value.clone()))
                    .collect(),
            )),
            SourceKind::Screenshot => Err(self.refuse(
                "a `screenshot` source produces an image rather than facts; the image runner takes it",
            )),
            _ => Err(self.refuse("is a kind this release does not refresh")),
        }
    }

    /// The document validated against the source's schema, if it declared one.
    fn check_schema(&self, document: &Value) -> Result<(), SourceError> {
        let Some(schema) = &self.spec.schema else {
            return Ok(());
        };
        let validator = jsonschema::validator_for(schema).map_err(|why| {
            SourceError::Schema(format!(
                "`verify.sources.{}.schema` is not a JSON Schema: {why}",
                self.spec.id
            ))
        })?;
        if let Err(problem) = validator.validate(document) {
            return Err(SourceError::Schema(format!(
                "`{}` at `{}`: {}",
                self.spec.id,
                problem.instance_path(),
                problem
            )));
        }
        Ok(())
    }

    fn values(&self, document: &Value) -> Result<BTreeMap<FactId, FactValue>, SourceError> {
        self.check_schema(document)?;
        read_facts(&self.spec, document)
            .map_err(|why| SourceError::Schema(format!("`{}`: {why}", self.spec.id)))
    }
}

impl TruthSource for DeclaredSource {
    fn id(&self) -> &str {
        &self.spec.id
    }

    fn kind(&self) -> SourceKind {
        self.spec.kind
    }

    fn trust(&self) -> TrustLevel {
        self.trust
    }

    fn snapshot<'a>(
        &'a self,
        http: &'a dyn HttpClient,
        sandbox: Option<&'a dyn Sandbox>,
    ) -> BoxFut<'a, Result<Snapshot, SourceError>> {
        Box::pin(async move {
            let document = self.document(http, sandbox).await?;
            let values = self.values(&document)?;
            let at = self.taken_at.unwrap_or_else(SystemTime::now);
            Ok(snapshot::build(&self.spec.id, at, values, &self.scrubber()))
        })
    }
}

/// Where a manual source's attestation stands (VER-24).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Attestation {
    /// Not a manual source, or one with no expiry to be late for.
    NotApplicable,
    Valid,
    /// Past its expiry and inside the grace period: `E0606` as a warning.
    Expired,
    /// Past its expiry by more than the grace period: `E0606` as an error.
    Lapsed,
}

/// VER-24's grace period. An attestation that expired this morning should not
/// stop tonight's deploy; one that expired last month should.
pub const DEFAULT_GRACE: Duration = Duration::from_secs(14 * 24 * 60 * 60);

pub fn attestation_of(spec: &SourceSpec, now: SystemTime) -> Attestation {
    attestation_with_grace(spec, now, DEFAULT_GRACE)
}

pub fn attestation_with_grace(spec: &SourceSpec, now: SystemTime, grace: Duration) -> Attestation {
    if spec.kind != SourceKind::Manual {
        return Attestation::NotApplicable;
    }
    let Some(expires) = spec.expires.as_deref().and_then(parse_date) else {
        return Attestation::NotApplicable;
    };
    if now <= expires {
        Attestation::Valid
    } else if now <= expires + grace {
        Attestation::Expired
    } else {
        Attestation::Lapsed
    }
}

impl Attestation {
    /// The diagnostic this state raises, if any. `E0606` is an error code;
    /// inside the grace period it is reported at warning severity, which is
    /// what "escalate to errors after a grace period" means.
    pub fn diagnostic(self, source: &str, expires: &str) -> Option<Diagnostic> {
        use liyasa_core::diagnostics::Severity;
        match self {
            Self::NotApplicable | Self::Valid => None,
            Self::Expired => Some(
                Diagnostic::new(
                    code::E0606,
                    format!("`{source}` was attested until {expires}, and that has passed"),
                )
                .with_severity(Severity::Warning)
                .help("re-attest it, or extend `expires`; after the grace period this is an error"),
            ),
            Self::Lapsed => Some(
                Diagnostic::new(
                    code::E0606,
                    format!(
                        "`{source}` was attested until {expires}, and the grace period has passed too"
                    ),
                )
                .help("re-attest it, or extend `expires`"),
            ),
        }
    }
}

/// Midnight UTC on an RFC 3339 date. A date-time keeps its time.
fn parse_date(text: &str) -> Option<SystemTime> {
    let (date, time) = match text.split_once(['T', ' ']) {
        Some((date, rest)) => (date, rest),
        None => (text, "00:00:00"),
    };
    let mut parts = date.split('-');
    let year: i64 = parts.next()?.parse().ok()?;
    let month: i64 = parts.next()?.parse().ok()?;
    let day: i64 = parts.next()?.parse().ok()?;
    if parts.next().is_some() || !(1..=12).contains(&month) || !(1..=31).contains(&day) {
        return None;
    }
    let mut clock = time.trim_end_matches('Z').split(':');
    let hour: i64 = clock.next().unwrap_or("0").parse().unwrap_or(0);
    let minute: i64 = clock.next().unwrap_or("0").parse().unwrap_or(0);
    let second: i64 = clock
        .next()
        .unwrap_or("0")
        .split('.')
        .next()
        .unwrap_or("0")
        .parse()
        .unwrap_or(0);
    let days = days_from_civil(year, month, day);
    let seconds = days * 86_400 + hour * 3_600 + minute * 60 + second;
    if seconds < 0 {
        return None;
    }
    Some(SystemTime::UNIX_EPOCH + Duration::from_secs(seconds as u64))
}

/// Howard Hinnant's `days_from_civil`: days since 1970-01-01, no leap seconds,
/// no table.
fn days_from_civil(year: i64, month: i64, day: i64) -> i64 {
    let year = year - i64::from(month <= 2);
    let era = if year >= 0 { year } else { year - 399 } / 400;
    let year_of_era = year - era * 400;
    let day_of_year = (153 * (month + if month > 2 { -3 } else { 9 }) + 2) / 5 + day - 1;
    let day_of_era = year_of_era * 365 + year_of_era / 4 - year_of_era / 100 + day_of_year;
    era * 146_097 + day_of_era - 719_468
}

fn sha256_hex(bytes: &[u8]) -> String {
    use sha2::Digest as _;
    let digest = sha2::Sha256::digest(bytes);
    digest.iter().fold(String::with_capacity(64), |mut out, b| {
        use std::fmt::Write as _;
        let _ = write!(out, "{b:02x}");
        out
    })
}

/// The outbound policy a fact refresh runs under. An empty allow list is "no
/// host restriction beyond `denyHosts`", which is what `network.allowHosts`
/// means when it is not set.
pub fn fact_source_policy() -> HttpPolicy {
    HttpPolicy {
        allow_hosts: HostSet::default(),
        deny_hosts: HostSet::default(),
        allow_private: false,
        max_redirects: 3,
        max_bytes: 8 * 1024 * 1024,
        timeout: Duration::from_secs(30),
        purpose: Purpose::FactSource,
    }
}

#[cfg(test)]
mod tests;
