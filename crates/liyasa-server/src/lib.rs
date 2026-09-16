//! The Liyasa server (PRD §18, §30.2.5).
//!
//! The crate root is shared: several packages own subtrees of it. Append your
//! `pub mod` and `pub use` lines; do not rewrite the file. WP-14 contributes
//! `routes`.

pub mod routes;
