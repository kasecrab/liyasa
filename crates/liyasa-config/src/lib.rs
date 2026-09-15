//! `liyasa.json`: loading through a [`Vfs`](liyasa_core::vfs::Vfs), validation
//! against `schemas/liyasa.schema.json`, defaults, and migration between schema
//! versions (PRD §8).

pub mod vfs;

/// The config types, generated from `schemas/liyasa.schema.json` by typify
/// (CFG-94). Never edited by hand; change the schema instead.
#[allow(
    clippy::all,
    clippy::pedantic,
    clippy::unwrap_used,
    dead_code,
    missing_docs
)]
pub mod model {
    include!(concat!(env!("OUT_DIR"), "/model.rs"));
}

pub use model::SiteConfig;
