//! Build-engine contracts (PRD §6.6, §34.9).
//!
//! `liyasa-build` re-exports these. `RenderPool` is the only CPU pool used for
//! request-time rendering: the server constructs one and borrows it everywhere
//! (§6.6.3 item 5).

use std::collections::BTreeSet;
use std::sync::Arc;
use std::time::{Duration, SystemTime};

use serde::{Deserialize, Serialize};

use crate::components::RenderError;
use crate::document::SourceDocument;
use crate::ids::{Fingerprint, Locale, PageId, Version};
use crate::markdown::TemplateContext;
use crate::net::BoxFut;
use crate::vfs::Bytes;

/// The single timestamp chosen at build start (§6.6.2 rule 1). Everything in
/// output that carries a time reads it, and nothing reads the wall clock.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub struct BuildClock(pub SystemTime);

#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
#[error("artifact cache: {0}")]
pub struct CacheError(pub String);

#[derive(
    Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize, schemars::JsonSchema,
)]
pub struct GcReport {
    pub removed: u64,
    pub bytes_freed: u64,
}

/// The fingerprint-keyed artifact cache (`.liyasa/cache`).
pub trait ArtifactCache: Send + Sync {
    fn get(&self, key: &Fingerprint) -> Option<Bytes>;
    /// `inputs` are what the value was derived from, so a garbage collection
    /// pass can reason about reachability.
    fn put(
        &self,
        key: &Fingerprint,
        value: Bytes,
        inputs: &[Fingerprint],
    ) -> Result<(), CacheError>;
    fn gc(&self, max_bytes: u64, max_age: Duration) -> Result<GcReport, CacheError>;
}

/// One coordinate in a page's variant set (§6.6.3).
#[derive(
    Debug,
    Clone,
    Default,
    PartialEq,
    Eq,
    PartialOrd,
    Ord,
    Hash,
    Serialize,
    Deserialize,
    schemars::JsonSchema,
)]
pub struct Variant {
    pub version: Option<Version>,
    pub locale: Option<Locale>,
    pub product: Option<String>,
    pub groups: BTreeSet<String>,
    pub region: Option<String>,
    pub variation: Option<String>,
}

#[derive(
    Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize, schemars::JsonSchema,
)]
#[serde(rename_all = "lowercase")]
pub enum OutputFormat {
    #[default]
    Html,
    Markdown,
}

#[derive(Debug, Clone)]
pub enum RenderMode {
    /// A reader is present: expansion runs with their values.
    OnDemand {
        source: Arc<SourceDocument>,
        ctx: TemplateContext,
    },
    /// Every `reader.*` field undefined under lenient-undefined semantics; what
    /// every shared index and agent surface reads (§6.6.4).
    Anonymous { source: Arc<SourceDocument> },
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct RenderBudget {
    pub cpu: Duration,
    pub max_bytes: u64,
}

#[derive(Debug, Clone)]
pub struct RenderJob {
    pub page: PageId,
    pub variant: Variant,
    pub format: OutputFormat,
    pub mode: RenderMode,
    pub budget: RenderBudget,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Rendered {
    pub body: String,
    pub cpu: Duration,
}

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct PoolMetrics {
    pub depth: usize,
    pub wait_p95: Duration,
    pub rejected: u64,
}

/// A rayon pool sized to `server.dynamic.concurrency`, a semaphore of the same
/// size, and a bounded queue. Static routes never touch it.
#[derive(Debug)]
#[non_exhaustive]
pub struct RenderPool;

impl RenderPool {
    pub fn new(_threads: usize, _queue: usize, _queue_timeout: Duration) -> Self {
        todo!("WP-06 owns the implementation; the signature is frozen here")
    }

    /// `Err(RenderError::Budget)` on timeout, a full queue, or a CPU or output
    /// overrun.
    pub fn submit<'a>(&'a self, _job: RenderJob) -> BoxFut<'a, Result<Rendered, RenderError>> {
        todo!("WP-06 owns the implementation; the signature is frozen here")
    }

    pub fn metrics(&self) -> PoolMetrics {
        todo!("WP-06 owns the implementation; the signature is frozen here")
    }
}
