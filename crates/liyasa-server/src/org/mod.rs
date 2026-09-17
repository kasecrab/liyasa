//! The organization and cloud control plane (ORG-01..03, ORG-10, ORG-20..21,
//! ORG-30..33, HOST-10..13, MIG-11).
//!
//! Everything here is one subtree of the server in the sense of RFC 1403, and
//! every piece of its state is held in [`state::OrgState`] rather than in the
//! store, for the reason RFC 1501 gives for `AuthState`: `liyasa-store` and
//! `migrations/` are WP-14's paths, and a package that cannot add a table can
//! still be complete, testable and honest about where its rows live. What that
//! costs is written down in RFC 2800 rather than left to be discovered.
//!
//! ORG-32 is the shape of the whole module: an OSS instance runs
//! [`plan::Plan::unlimited`] and takes the same code path a Free project takes.
//! No handler asks which edition it is in.

pub mod credits;
pub mod meter;
pub mod model;
pub mod plan;
pub mod region;
