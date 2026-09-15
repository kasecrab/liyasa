//! The Liyasa build engine (PRD §6.6, §34.9).
//!
//! The crate root is shared: several packages own subtrees of it. Append your
//! `pub mod` and `pub use` lines; do not rewrite the file. WP-10 contributes
//! `agents` and the §6.11 re-exports below; see
//! `plan/rfcs/1000-agents-crate-seam.md`.

pub mod agents;

pub mod assets;
pub mod cache;
pub mod clock;
pub mod dev;
pub mod engine;
pub mod git;
pub mod images;
pub mod manifest;
pub mod pool;
pub mod redirects;
pub mod render;
pub mod tree;
pub mod variants;
pub mod versions;
pub mod watch;

pub use liyasa_core::build::{
    ArtifactCache, BuildClock, CacheError, GcReport, OutputFormat, PoolMetrics, RenderBudget,
    RenderJob, RenderMode, Rendered, Variant,
};
// TODO(rfc-0602): `RenderPool` is WP-06's implementation, not the `todo!()`
// stub `liyasa-core` holds the signature in.
pub use liyasa_core::ids::Fingerprint;
pub use pool::RenderPool;
