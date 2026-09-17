//! Localization and regions (PRD §7.11, §7.12, §19.6).
//!
//! Two independent axes (AUTH-55): a locale decides which words a reader sees
//! and shows in the route, a region decides which content exists for them and
//! shows in the variant. Nothing here couples them — a German reader in the US
//! is `locale = de` and `region = us`, and each is resolved on its own.

pub mod config;
pub mod fallback;
pub mod locales;
pub mod negotiate;
pub mod openapi;
pub mod regions;
pub mod variations;
pub mod strings;
pub mod sync;

pub use config::{Detection, Fallback, LocaleDecl, Localization, Regions, VariationDecl};
pub use fallback::{Serve, Translations};
pub use locales::{Alternate, Locales, SwitcherEntry};
pub use negotiate::{Decision, Routing};
pub use regions::{Detector, Rendering, Scope};
pub use variations::Variations;
