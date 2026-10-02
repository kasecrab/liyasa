//! The drift engine (§14.12, VER-73).
//!
//! Everything upstream of here produces an observation: a snapshot diff and the
//! blocks it reaches ([`crate::sources`]), an operation that moved between two
//! versions of a spec, a link that has been failing for a while
//! ([`crate::core::links`]), a check that stopped passing
//! ([`crate::core::orchestrate`]). None of them decides what becomes a record,
//! how bad it is, or when it closes. This package is that decision.
//!
//! It reads a completed run rather than taking part in one, so `core` never
//! depends on `drift` and the two can be reasoned about apart.

pub mod checks;
pub mod engine;
pub mod entries;
pub mod facts;
pub mod links;
pub mod owners;
pub mod policy;
pub mod record;
pub mod review;
pub mod spec;
pub mod store;
pub mod wire;

// `DriftSeverity` is drift's own scale (RFC 1302) and lives in `core::config`
// because that is where `verify.drift` is parsed. Re-exported here because that
// is where a caller of this module looks for it: WP-14's first real call site
// spent two compile errors finding it under `core::config`.
pub use crate::core::config::DriftSeverity;
pub use engine::{Coverage, Engine, GraphRoutes, Routes};
pub use entries::entries;
pub use links::FailingLink;
pub use owners::Docowners;
pub use policy::DriftPolicy;
pub use record::{Candidate, DriftKey, DriftKind, DriftRecord, DriftState, Resolution, escalated};
pub use review::{Cadence, Digest, Overdue, OwnerDigest, PageReview};
pub use store::{MemoryDrift, RecordStore};
