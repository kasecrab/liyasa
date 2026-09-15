//! Rate-limiter contracts (PRD §30.2.5, §34.9).
//!
//! Frozen so that per-customer tuning is typed rather than a string map.

use std::net::IpAddr;
use std::time::Duration;

use serde::{Deserialize, Serialize};

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
#[non_exhaustive]
pub enum RateLimitPool {
    Pages,
    /// `.md` responses and MCP fetches.
    AgentPages,
    Search,
    Assistant,
    Feedback,
    PlaygroundProxy,
    Rest,
    Mcp,
    Auth,
}

/// An IPv4 `/32` or IPv6 `/64`: a limiter never keys on a single IPv6 address,
/// which a client can rotate freely within its prefix.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct IpNet(pub IpAddr, pub u8);

#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub enum LimiterSubject {
    Ip(IpNet),
    Token(crate::ids::TokenId),
}

#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct RateLimitKey {
    pub pool: RateLimitPool,
    pub subject: LimiterSubject,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, schemars::JsonSchema)]
pub struct RateLimit {
    pub per_minute: u32,
    pub burst: u32,
    pub daily: Option<u64>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, thiserror::Error)]
#[error("rate limited; retry after {:?}", .0)]
pub struct RetryAfter(pub Duration);

pub trait RateLimiter: Send + Sync {
    fn check(&self, key: &RateLimitKey) -> Result<(), RetryAfter>;
    fn configure(&self, pool: RateLimitPool, limit: RateLimit);
}
