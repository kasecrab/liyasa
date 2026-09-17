//! The reader-facing assistant (PRD §21) and the provider abstraction it runs
//! on (§6.7).
//!
//! The §34.9 contracts live in [`liyasa_core::ai`] and are re-exported here;
//! this crate is everything built on top of them: the adapters that turn a
//! [`ChatRequest`] into one provider's wire format and back, the chunker and
//! vector index the retrieval reads from, and the loop that plans, retrieves
//! and answers.
//!
//! Nothing here opens a socket of its own. Outbound calls go through
//! [`liyasa_core::net::HttpClient`] under `Purpose::ModelProvider`, so the
//! address policy of §30.2.3 applies to a model provider exactly as it applies
//! to a spec fetch, and a test injects a client instead of a server.

pub mod chunk;
pub mod config;
pub mod error;
pub mod exclude;

pub use chunk::{Chunk, ChunkOptions, chunk};
pub use config::{AiConfig, AssistantConfig, ModelRef, Role};
pub use error::AiFailure;
pub use exclude::{Environment, Excluded, exclusion};
pub use liyasa_core::ai::*;
