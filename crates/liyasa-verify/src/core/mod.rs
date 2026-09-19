//! Checks that run in the verifier's own process, and the scrubber every
//! output path goes through.

pub mod config;
#[cfg(test)]
pub mod corpus;
pub mod duration;
pub mod links;
pub mod orchestrate;
pub mod policy;
pub mod prose;
pub mod runners;
pub mod scrub;
pub mod spell;
pub mod structural;

pub use config::{VerifyConfig, VerifyDefault};
pub use duration::{DurationError, DurationSetting, Unit};
pub use links::{LinkChecker, LinkOutcome, LinkStatus, RateLimits};
pub use orchestrate::{Budget, Orchestrator, Page, Run};
pub use policy::{CheckClass, PageVerify, Policy, PolicyLevel, PolicySet, Skip, block_skip};
pub use runners::{Registry, in_process};
pub use scrub::{EXCERPT_LIMIT, REDACTED, Scrubber};
pub use spell::{Dictionary, SpellChecker};
pub use structural::{PageView, SiteView, SizeLimits, check_site};
