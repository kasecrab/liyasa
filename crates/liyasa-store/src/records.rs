//! The rows the server reads and writes (RFC 1400).
//!
//! `liyasa-core`'s entity structs carry no fields yet; these are the field
//! sets proposed for them, stored by `migrations/0001_init.sql`.

use liyasa_core::ids::{BuildId, JobId, OrgId, ProjectId};
use liyasa_core::store::{BuildStatus, JobState};
use serde::{Deserialize, Serialize};

// TODO(rfc-1400): becomes `liyasa_core::store::Project` once it has fields.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ProjectRecord {
    pub id: ProjectId,
    pub org: Option<OrgId>,
    pub slug: String,
    pub name: String,
    pub created_at: i64,
    pub updated_at: i64,
    pub version: i64,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct BuildRecord {
    pub id: BuildId,
    pub project: ProjectId,
    pub env: String,
    pub status: BuildStatus,
    /// Where the bundle is: a directory for local storage, an object key
    /// otherwise.
    pub dist: String,
    pub created_at: i64,
    pub updated_at: i64,
    pub version: i64,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct DeploymentRecord {
    pub project: ProjectId,
    pub env: String,
    pub build: BuildId,
    pub created_at: i64,
    pub updated_at: i64,
    pub version: i64,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct JobRecord {
    pub id: JobId,
    pub name: String,
    /// De-duplication key: one live job per `(name, key)` (HOST-07).
    pub key: String,
    pub priority: i32,
    pub state: JobState,
    pub project: Option<ProjectId>,
    pub payload: serde_json::Value,
    pub attempts: u32,
    pub max_attempts: u32,
    /// Not runnable before this instant (backoff, schedules).
    pub run_at: i64,
    pub lease_ms: i64,
    pub lease_until: Option<i64>,
    pub worker: Option<String>,
    pub result: Option<serde_json::Value>,
    pub error: Option<String>,
    pub created_at: i64,
    pub updated_at: i64,
    pub version: i64,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum FeedbackKind {
    /// RX-50: thumbs on a page.
    Page,
    /// RX-51: thumbs on a code block.
    Code,
    /// RX-52: an agent reporting a page that failed it.
    Agent,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum FeedbackStatus {
    Open,
    Triaged,
    Resolved,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct FeedbackRecord {
    pub id: String,
    pub project: Option<ProjectId>,
    pub route: String,
    pub kind: FeedbackKind,
    /// `1` up, `-1` down, absent for an agent report.
    pub rating: Option<i32>,
    pub category: Option<String>,
    pub text: Option<String>,
    pub block_id: Option<String>,
    /// RX-52: the failing task, summarized by the agent.
    pub task: Option<String>,
    pub status: FeedbackStatus,
    pub notes: String,
    pub created_at: i64,
    pub updated_at: i64,
}

/// One analytics event (ANA-02, §34.6). `props` holds what the type needs
/// and nothing the scrubber has not passed.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, Default)]
pub struct EventRecord {
    pub ts: i64,
    pub site: String,
    pub env: String,
    pub route: String,
    #[serde(rename = "type")]
    pub kind: String,
    #[serde(default)]
    pub variant: serde_json::Value,
    #[serde(default)]
    pub caller: serde_json::Value,
    #[serde(default)]
    pub format: String,
    #[serde(default)]
    pub session_key: String,
    #[serde(default)]
    pub referrer_host: Option<String>,
    #[serde(default)]
    pub device: serde_json::Value,
    #[serde(default)]
    pub country: Option<String>,
    #[serde(default)]
    pub duration_ms: Option<u32>,
    #[serde(default)]
    pub props: serde_json::Value,
}

/// ANA-08's drop order: interaction first, then search and view, never the
/// rest.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum EventClass {
    Interaction,
    SearchOrView,
    Critical,
}

impl EventRecord {
    pub fn class(&self) -> EventClass {
        match self.kind.as_str() {
            "page_view" | "markdown_fetch" | "search" | "search_click" => EventClass::SearchOrView,
            "deployment" | "feedback" | "assistant_message" | "mcp_call" | "playground_request" => {
                EventClass::Critical
            }
            _ => EventClass::Interaction,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct WebhookSubscription {
    pub id: String,
    pub project: Option<ProjectId>,
    pub url: String,
    pub secret: String,
    /// Event types, or empty for all.
    pub events: Vec<String>,
    pub active: bool,
    pub failures: u32,
    pub created_at: i64,
    pub updated_at: i64,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum DeliveryStatus {
    Pending,
    Delivered,
    Failed,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct WebhookDelivery {
    pub id: String,
    pub subscription: String,
    pub event_id: String,
    pub event_type: String,
    pub payload: String,
    pub attempt: u32,
    pub next_at: i64,
    pub status: DeliveryStatus,
    pub last_status: Option<u16>,
    pub created_at: i64,
    pub updated_at: i64,
}
