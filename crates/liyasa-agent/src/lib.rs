//! The writing agent (PRD §31): the runtime that turns a task into a reviewable
//! proposal against the documentation itself.
//!
//! Two properties hold before anything else in this crate runs, and everything
//! else is built to satisfy them.
//!
//! The first is that every input to a run carries a trust level, and the run's
//! policy is the minimum over its inputs (§30.2.2). A run with any `anonymous`
//! or `external` input is an *untrusted-trigger run*: its write scope is the
//! pages the trigger named and nothing else, and no configuration can let it
//! merge itself. [`trust`] decides that and [`policy`] enforces it.
//!
//! The second is that the check on what a run produced does not run inside the
//! model. [`gates`] is a deterministic function over a typed diff, and it runs
//! before a proposal exists. A model that has been talked into writing
//! something it should not still has to get that diff past a checker that never
//! read the prompt.
//!
//! Nothing here opens a socket or touches a filesystem of its own: a run reads
//! through [`liyasa_core::vfs::Vfs`] and reaches a model through
//! [`liyasa_core::ai::ChatModel`], both injected, so the whole crate is
//! testable against a mock and the address policy of §30.2.3 applies.

pub mod agents_md;
pub mod config;
pub mod diff;
pub mod dispatch;
pub mod frontmatter;
pub mod gates;
pub mod hosts;
pub mod injection;
pub mod policy;
pub mod proposal;
pub mod record;
pub mod repos;
pub mod scope;
pub mod secrets;
pub mod tools;
pub mod trust;
