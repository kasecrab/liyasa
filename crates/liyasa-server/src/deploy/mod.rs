//! Deployments: what gets built, in what order, and what an environment
//! points at (PRD §18, §6.13, GIT-20..GIT-51).
//!
//! `routes/deployments.rs` already points an environment at a build and points
//! it back. This module is the half that decides *what* to point at: the queue
//! a push turns into, the environment a branch belongs to, the preview a pull
//! request gets, and the rules an untrusted build is held to.

pub mod environment;
pub mod hooks;
pub mod hosts;
pub mod preview;
pub mod queue;
pub mod retention;
pub mod routes;
pub mod rollback;
pub mod service;
pub mod untrusted;
pub mod worker;

pub use environment::{Environment, EnvironmentKind, Protection};
pub use hosts::{Paved, PublishPlan};
pub use preview::Preview;
pub use queue::{Accepted, BuildRequest, Class, DeployQueue, Limits, QueueError, Trigger};
pub use retention::Retention;
pub use rollback::{Actor, Rollback, RollbackError};
pub use routes::router;
pub use service::{Binding, DeployState, Hooks};
pub use untrusted::{Reason, Sandbox};
pub use worker::run_build;
