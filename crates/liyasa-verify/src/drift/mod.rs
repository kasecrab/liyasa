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

pub mod engine;
pub mod facts;
pub mod policy;
pub mod record;
pub mod store;

pub use engine::{Coverage, Engine, GraphRoutes, Routes};
pub use policy::DriftPolicy;
pub use record::{Candidate, DriftKey, DriftKind, DriftRecord, DriftState, Resolution, escalated};
pub use store::{MemoryDrift, RecordStore};
