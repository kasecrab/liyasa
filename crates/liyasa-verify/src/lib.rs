//! The verification engine (PRD §14).
//!
//! `liyasa-core` owns the contracts (`Runner`, `Sandbox`, `TruthSource`, and
//! the truth-engine traits); this crate implements the halves of §14 that need
//! no container: the in-process runners, build-time link checking, the
//! structural checks that run on every build, prose lint and spelling, the
//! output scrubber, and the report formats.
//!
//! The crate is split by package (`plan/rfcs/1300-verify-owned-paths.md`):
//! `core` and `report` here, `graph`, `sources`, `drift`, `scan`, and the
//! sandboxed `runners` in packages of their own.

pub mod core;
pub mod report;
pub mod graph;
pub mod runners;
