//! The Liyasa build engine (PRD §6.6, §34.9).
//!
//! The crate root is shared: several packages own subtrees of it. Append your
//! `pub mod` and `pub use` lines; do not rewrite the file. WP-10 contributes
//! `agents` and the §6.11 re-exports below; see
//! `plan/rfcs/1000-agents-crate-seam.md`.

pub mod agents;

pub mod clock;
pub mod git;

pub use liyasa_core::build::{
    ArtifactCache, BuildClock, CacheError, GcReport, OutputFormat, PoolMetrics, RenderBudget,
    RenderJob, RenderMode, RenderPool, Rendered, Variant,
};
pub use liyasa_core::ids::Fingerprint;
