//! The Source Document: scanner, segments, span maps, and the formatter.

pub mod escape;
pub(crate) mod lines;
pub mod mask;
mod normalize;
pub mod yaml;

mod scan;
mod wellformed;

pub use escape::escape_untrusted_markdown;
pub use normalize::normalize;
pub use scan::{body_start, scan};
