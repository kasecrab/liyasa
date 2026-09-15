//! Verification and truth-engine contracts (PRD §14, §14.12, §34.9).
//!
//! `liyasa-verify` re-exports these; each runner implementer and the server's
//! schedulers code against them.

use std::collections::BTreeMap;
use std::time::{Duration, SystemTime};

use serde::{Deserialize, Serialize};

use crate::ai::TrustLevel;
use crate::diagnostics::{Diagnostic, Severity};
use crate::document::{DepTarget, Document, Edge, EdgeOrigin};
use crate::ids::{BlockId, BuildId, CheckId, FactId, Fingerprint, Route};
use crate::markdown::ExpansionRecord;
use crate::net::{BoxFut, HttpClient, NetError};
use crate::vfs::{Bytes, VfsPath};

// ---- checks and runners ----

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, schemars::JsonSchema)]
#[serde(tag = "input", rename_all = "camelCase")]
#[non_exhaustive]
pub enum CheckInput {
    Code {
        lang: String,
        source: String,
        hidden_lines: Vec<u32>,
    },
    Http {
        request: String,
    },
    Fact {
        id: FactId,
    },
    Link {
        url: String,
    },
    Screenshot {
        source: String,
        target: String,
    },
    Schema {
        lang: String,
        source: String,
        schema: String,
    },
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, schemars::JsonSchema)]
#[serde(tag = "expect", content = "value", rename_all = "camelCase")]
#[non_exhaustive]
pub enum Expectation {
    Stdout(String),
    StdoutFile(VfsPath),
    Exit(i32),
    Status(u16),
    JsonPath {
        path: String,
        value: serde_json::Value,
    },
    Header {
        name: String,
        value: String,
    },
    ResponseSchema {
        spec: String,
        op: String,
    },
    Equals(FactValue),
    Tolerance(f32),
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, schemars::JsonSchema)]
pub struct CheckSpec {
    pub id: CheckId,
    pub page: Route,
    pub block: BlockId,
    pub runner: String,
    pub input: CheckInput,
    pub expect: Vec<Expectation>,
    #[serde(with = "crate::serde_time::duration_ms")]
    #[schemars(with = "u64")]
    pub timeout: Duration,
    pub needs_network: bool,
    pub needs_secrets: Vec<String>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, schemars::JsonSchema)]
#[serde(tag = "outcome", rename_all = "camelCase")]
pub enum CheckOutcome {
    Pass,
    /// The excerpt is scrubbed and capped at 512 bytes before it is stored.
    Fail {
        excerpt: String,
    },
    Skip {
        reason: String,
    },
    Error(Diagnostic),
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, schemars::JsonSchema)]
pub struct CheckResult {
    pub id: CheckId,
    pub outcome: CheckOutcome,
    #[serde(with = "crate::serde_time::duration_ms")]
    #[schemars(with = "u64")]
    pub duration: Duration,
    pub digest: Fingerprint,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, schemars::JsonSchema)]
#[serde(rename_all = "camelCase")]
pub enum Isolation {
    InProcess,
    Sandbox,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SandboxJob {
    pub image: String,
    pub digest: String,
    pub cmd: Vec<String>,
    pub files: Vec<(VfsPath, Bytes)>,
    pub env: Vec<(String, String)>,
    pub timeout: Duration,
    pub network: bool,
    pub cpu_millis: u32,
    pub mem_bytes: u64,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SandboxOutput {
    pub exit: i32,
    /// Capped here, scrubbed by the caller.
    pub stdout: Bytes,
    pub stderr: Bytes,
    pub duration: Duration,
}

#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
#[non_exhaustive]
pub enum SandboxError {
    #[error("no sandbox is configured")]
    Unavailable,
    #[error("image `{0}` is not allowed or its digest does not match")]
    Image(String),
    #[error("sandbox job timed out")]
    Timeout,
    #[error("sandbox: {0}")]
    Io(String),
}

pub trait SecretSource: Send + Sync {
    fn get(&self, name: &str) -> Option<zeroize::Zeroizing<String>>;
}

pub trait Sandbox: Send + Sync {
    fn exec<'a>(&'a self, job: SandboxJob) -> BoxFut<'a, Result<SandboxOutput, SandboxError>>;
}

pub trait Runner: Send + Sync {
    fn id(&self) -> &'static str;
    fn languages(&self) -> &'static [&'static str];
    fn isolation(&self) -> Isolation;
    fn run<'a>(
        &'a self,
        spec: &'a CheckSpec,
        sandbox: &'a dyn Sandbox,
        secrets: &'a dyn SecretSource,
    ) -> BoxFut<'a, CheckResult>;
}

// ---- facts and truth sources ----

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, schemars::JsonSchema)]
#[serde(rename_all = "lowercase")]
#[non_exhaustive]
pub enum SourceKind {
    File,
    Repo,
    Url,
    OpenApi,
    Command,
    Screenshot,
    Manual,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, schemars::JsonSchema)]
#[serde(tag = "type", content = "value", rename_all = "camelCase")]
#[non_exhaustive]
pub enum FactValue {
    Str(String),
    Num(f64),
    Currency {
        amount: i64,
        minor: u8,
        code: String,
    },
    Percent(f64),
    Date(String),
    Bool(bool),
    Enum(String),
    List(Vec<FactValue>),
    Object(BTreeMap<String, FactValue>),
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, schemars::JsonSchema)]
pub struct Snapshot {
    pub source: String,
    #[serde(with = "crate::serde_time::system_time_ms")]
    #[schemars(with = "u64")]
    pub taken_at: SystemTime,
    pub digest: Fingerprint,
    pub values: BTreeMap<FactId, FactValue>,
}

#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
#[non_exhaustive]
pub enum SourceError {
    #[error("source unreachable: {0}")]
    Net(#[from] NetError),
    #[error("source rejected by policy: {0}")]
    Policy(String),
    #[error("source returned a value that does not match its schema: {0}")]
    Schema(String),
    #[error("sandbox: {0}")]
    Sandbox(#[from] SandboxError),
}

pub trait TruthSource: Send + Sync {
    fn id(&self) -> &str;
    fn kind(&self) -> SourceKind;
    fn trust(&self) -> TrustLevel;
    fn snapshot<'a>(
        &'a self,
        http: &'a dyn HttpClient,
        sandbox: Option<&'a dyn Sandbox>,
    ) -> BoxFut<'a, Result<Snapshot, SourceError>>;
}

// ---- the truth engine, decomposed (§14.12) ----

pub trait DependencyExtractor: Send + Sync {
    fn extract(&self, doc: &Document, expansion: &ExpansionRecord) -> Vec<Edge>;
}

#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
#[non_exhaustive]
pub enum StoreError {
    #[error("not found")]
    NotFound,
    #[error("version conflict")]
    Conflict,
    #[error("i/o: {0}")]
    Io(String),
    #[error("sql: {0}")]
    Sql(String),
}

#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize, schemars::JsonSchema)]
pub struct GraphDiff {
    pub added: Vec<Edge>,
    pub removed: Vec<Edge>,
}

pub trait GraphStore: Send + Sync {
    fn replace_page_edges(
        &self,
        build: BuildId,
        page: &Route,
        edges: &[Edge],
    ) -> Result<(), StoreError>;
    fn dependents(&self, target: &DepTarget) -> Result<Vec<EdgeOrigin>, StoreError>;
    fn dependencies(&self, origin: &EdgeOrigin) -> Result<Vec<Edge>, StoreError>;
    fn diff(&self, from: BuildId, to: BuildId) -> Result<GraphDiff, StoreError>;
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, schemars::JsonSchema)]
#[serde(rename_all = "lowercase")]
pub enum ChangeKind {
    Added,
    Removed,
    Changed,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, schemars::JsonSchema)]
pub struct FactChange {
    pub fact: FactId,
    pub old: Option<FactValue>,
    pub new: Option<FactValue>,
    pub kind: ChangeKind,
}

pub trait SnapshotDiffer: Send + Sync {
    fn diff(&self, old: &Snapshot, new: &Snapshot) -> Vec<FactChange>;
}

/// A change and the blocks it reaches, each with the edge path that proves it.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, schemars::JsonSchema)]
pub struct Impact {
    pub change: FactChange,
    pub blocks: Vec<(EdgeOrigin, Vec<Edge>)>,
}

pub trait ImpactQuery: Send + Sync {
    fn impact(
        &self,
        changes: &[FactChange],
        graph: &dyn GraphStore,
    ) -> Result<Vec<Impact>, StoreError>;
}

/// Mirrors the `verify.policy` schema object.
#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize, schemars::JsonSchema)]
#[non_exhaustive]
pub struct VerifyPolicy {
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub fail_on: Option<Severity>,
    #[serde(flatten)]
    pub rest: BTreeMap<String, serde_json::Value>,
}

#[derive(
    Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize, schemars::JsonSchema,
)]
pub struct DriftReport {
    pub created: u32,
    pub updated: u32,
    pub resolved: u32,
}

pub trait DriftEngine: Send + Sync {
    fn apply(
        &self,
        impacts: &[Impact],
        checks: &[CheckResult],
        policy: &VerifyPolicy,
        store: &dyn crate::store::DriftRepo,
    ) -> Result<DriftReport, StoreError>;
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, schemars::JsonSchema)]
pub struct ClaimCandidate {
    pub block: BlockId,
    pub page: Route,
    pub fact: FactId,
    pub confidence: f32,
    pub excerpt: String,
}

#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
#[non_exhaustive]
pub enum ScanError {
    #[error("model: {0}")]
    Model(String),
    #[error("store: {0}")]
    Store(#[from] StoreError),
}

pub trait ClaimScanner: Send + Sync {
    fn scan<'a>(
        &'a self,
        change: &'a FactChange,
        candidates: &'a [Route],
    ) -> BoxFut<'a, Result<Vec<ClaimCandidate>, ScanError>>;
}
