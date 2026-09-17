//! Analytics: the event schema, who is asking, what the numbers say, and what
//! is thrown away (PRD §26, ANA-01..71).
//!
//! `liyasa-server` owns the write path — the beacon endpoint, the daily salt,
//! the bounded queue — and `liyasa-store` owns the database. This crate is the
//! read half: the published schema those two write against, the maintained
//! agent list they classify with, the queries the dashboard draws, the insight
//! cards and digest computed from them, and the retention that deletes the
//! rest.
//!
//! It never opens a database. Every entry point that reads one takes a pool
//! the caller already opened, which in practice is
//! `liyasa_store::ingest::Writer::pool()` (RFC 1700).

pub mod actions;
pub mod api;
pub mod agents;
pub mod digest;
pub mod feedback;
pub mod insights;
pub mod integrations;
pub mod privacy;
pub mod props;
pub mod query;
pub mod retention;
pub mod schema;
pub mod search;
pub(crate) mod sql;
pub mod traffic;

pub use agents::{Caller, CallerKind, Headless, Signals, Structural};
pub use query::{Comparison, Filters, Grain, Range, RangeSpec, SavedView};
