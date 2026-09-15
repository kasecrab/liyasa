//! Checks that run in the verifier's own process, and the scrubber every
//! output path goes through.

pub mod scrub;

pub use scrub::{EXCERPT_LIMIT, REDACTED, Scrubber};
