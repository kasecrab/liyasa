//! Checks that run in the verifier's own process, and the scrubber every
//! output path goes through.

pub mod config;
pub mod duration;
pub mod policy;
pub mod runners;
pub mod scrub;

pub use config::{VerifyConfig, VerifyDefault};
pub use duration::{DurationError, DurationSetting, Unit};
pub use policy::{CheckClass, PageVerify, Policy, PolicyLevel, PolicySet, Skip, block_skip};
pub use runners::{Registry, in_process};
pub use scrub::{EXCERPT_LIMIT, REDACTED, Scrubber};
