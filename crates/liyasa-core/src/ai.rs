//! Model-provider contracts (PRD §6.7, §30.2.2, §34.9).
//!
//! `liyasa-ai` re-exports these. Nothing here is bound to a vendor: every
//! feature that calls a model declares which role it uses, and an operator
//! routes roles to providers.

use std::time::Duration;

use serde::{Deserialize, Serialize};

use crate::net::{BoxFut, BoxStream, NetError};
use crate::vfs::Bytes;

/// How far a value may travel before it must be escaped (§30.2.2).
///
/// Ordered from most to least trusted, so `trust <= TrustLevel::Member` reads
/// as "at least a member".
#[derive(
    Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize, schemars::JsonSchema,
)]
#[serde(rename_all = "lowercase")]
pub enum TrustLevel {
    Operator,
    Member,
    Anonymous,
    External,
}

/// Untrusted content handed to a model, rendered with the fixed preamble that
/// says it is data and not instructions.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, schemars::JsonSchema)]
pub struct DataBlock {
    pub label: String,
    pub trust: TrustLevel,
    pub content: String,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, schemars::JsonSchema)]
#[serde(rename_all = "lowercase")]
pub enum Role {
    User,
    Assistant,
    Tool,
}

#[derive(Debug, Clone, PartialEq)]
#[non_exhaustive]
pub enum Part {
    Text(String),
    Image {
        mime: String,
        data: Bytes,
    },
    ToolCall {
        id: String,
        name: String,
        input: serde_json::Value,
    },
    ToolResult {
        id: String,
        output: serde_json::Value,
        trust: TrustLevel,
    },
}

#[derive(Debug, Clone, PartialEq)]
pub struct Message {
    pub role: Role,
    pub content: Vec<Part>,
    pub trust: TrustLevel,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, schemars::JsonSchema)]
pub struct ToolSpec {
    pub name: String,
    pub description: String,
    pub input_schema: serde_json::Value,
    /// The tool is unavailable below this level.
    pub min_trust: TrustLevel,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, schemars::JsonSchema)]
pub struct Budget {
    pub max_tokens: u32,
    pub max_tool_calls: u16,
    #[serde(with = "crate::serde_time::duration_ms")]
    #[schemars(with = "u64")]
    pub wall: Duration,
    pub cost_cents: Option<u32>,
}

#[derive(Debug, Clone, PartialEq)]
pub struct ChatRequest {
    /// Operator text only. Reader and agent content belongs in `data`.
    pub system: String,
    pub messages: Vec<Message>,
    pub data: Vec<DataBlock>,
    pub tools: Vec<ToolSpec>,
    pub output_schema: Option<serde_json::Value>,
    pub budget: Budget,
}

#[derive(Debug, Clone, PartialEq)]
#[non_exhaustive]
pub enum ChatEvent {
    Token(String),
    ToolCall {
        id: String,
        name: String,
        input: serde_json::Value,
    },
    Usage {
        input: u32,
        output: u32,
    },
    Done,
}

#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
#[non_exhaustive]
pub enum AiError {
    #[error("provider returned {status}: {message}")]
    Provider { status: u16, message: String },
    #[error("budget exhausted")]
    Budget,
    #[error("rate limited by the provider")]
    RateLimited { retry_after: Option<Duration> },
    #[error("blocked by policy: {0}")]
    Policy(String),
    #[error(transparent)]
    Net(#[from] NetError),
}

pub trait ChatModel: Send + Sync {
    fn id(&self) -> &str;
    fn complete<'a>(
        &'a self,
        req: ChatRequest,
    ) -> BoxFut<'a, Result<BoxStream<'a, ChatEvent>, AiError>>;
}

pub trait EmbeddingModel: Send + Sync {
    fn id(&self) -> &str;
    fn dims(&self) -> usize;
    fn embed<'a>(&'a self, inputs: &'a [String]) -> BoxFut<'a, Result<Vec<Vec<f32>>, AiError>>;
}

pub trait Reranker: Send + Sync {
    fn rerank<'a>(
        &'a self,
        query: &'a str,
        docs: &'a [String],
    ) -> BoxFut<'a, Result<Vec<(usize, f32)>, AiError>>;
}
