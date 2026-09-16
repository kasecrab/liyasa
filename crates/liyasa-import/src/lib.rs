//! Importers: Mintlify, Docusaurus, and generic MDX (PRD §28.1).
//!
//! Every importer reads a source project through a [`Vfs`](liyasa_core::vfs::Vfs)
//! and returns a [`Plan`]: the files it would write, and a [`Report`] naming
//! every construct it could not convert. Nothing reaches the disk until the
//! caller applies the plan, so a dry run is the default rather than a mode
//! (MIG-06).
//!
//! The component registry lives in `liyasa-components`, which this crate does
//! not depend on (PRD §34.7); like
//! [`from_mdx`](liyasa_markdown::from_mdx), an importer is told which component
//! names Liyasa knows and translates syntax only.

pub mod plan;
pub mod report;

pub use plan::{Apply, Content, FileWrite, Plan};
pub use report::{Attention, Kind, PageReport, Redirect, Report, Source};
