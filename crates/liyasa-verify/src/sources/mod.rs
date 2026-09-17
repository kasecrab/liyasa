//! Truth sources: what a fact is, where its value comes from, and what a
//! changed value reaches (§14.11, §14.12).
//!
//! A source is declared in `verify.sources.<id>` ([`spec`], RFC 2030),
//! refreshed into a [`Snapshot`](liyasa_core::verify::Snapshot), and diffed
//! against the last one. [`impact`] turns that diff into the blocks it reaches,
//! by walking the graph [`crate::graph`] built.
//!
//! The graph is WP-20a's and the drift records are WP-20c's; this package is
//! the half between them.

pub mod impact;
pub mod kinds;
pub mod openapi;
pub mod refresh;
pub mod snapshot;
pub mod spec;
pub mod trust;

pub use impact::{OperationChange, OperationImpact, PathImpact, routes_of};
pub use kinds::{Attestation, BuildTrust, DeclaredSource, SandboxLimits};
pub use openapi::operation_changes;
pub use refresh::{Fact, Facts, RefreshReport, Refresher, plain};
pub use snapshot::{SnapshotLog, StoredSnapshot, ValueDiffer};
pub use spec::{FactType, SourceSet, SourceSpec};
pub use trust::{TransportError, TransportPolicy, trust_of};
