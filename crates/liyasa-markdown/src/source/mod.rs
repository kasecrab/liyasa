//! The Source Document: scanner, segments, span maps, and the formatter.

pub mod escape;
pub mod expand;
pub mod filters;
mod format;
pub(crate) mod lines;
pub mod mask;
mod normalize;
pub mod yaml;

pub mod route;
mod scan;
mod serialize;
mod wellformed;

pub use escape::escape_untrusted_markdown;
pub use expand::{Budget, ExpandOptions, Undefined, environment, expand, expand_with};
pub use format::{FormatOptions, format, format_with, is_formatted};
pub use normalize::normalize;
pub use route::{Ignore, external_of, href, is_routable, route_of};
pub use scan::{body_start, scan};
pub use serialize::serialize_source;
