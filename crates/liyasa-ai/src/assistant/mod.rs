//! The reader-facing assistant (§21.2).

pub mod answer;
pub mod context;
pub mod memory;
pub mod run;
pub mod tools;

pub use answer::{Answer, Citation, DeflectionTarget};
pub use context::ReaderContext;
pub use memory::{Thread, Turn};
pub use run::{Outcome, Plan, ask};
pub use tools::{NavEntry, PageExcerpt, ToolError, Tools};
