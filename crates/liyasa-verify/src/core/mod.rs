//! Checks that run in the verifier's own process, and the scrubber every
//! output path goes through.

pub mod duration;
pub mod scrub;

pub use duration::{DurationError, DurationSetting, Unit};
pub use scrub::{EXCERPT_LIMIT, REDACTED, Scrubber};
