//! The agent surfaces: Markdown routes, `llms.txt`, skills, feeds, and the
//! agent-readiness report (PRD §11.7, §11.8, §11.9, §25).
//!
//! Everything here is produced from the anonymous render of a page (§6.6.4), so
//! no reader value can reach a shared surface (SRC-12).

pub mod continuation;
pub mod feeds;
pub mod llms;
pub mod markdown;
pub mod notfound;
pub mod resource;
pub mod site;
pub mod size;
pub mod skill;
pub mod spec;

pub use markdown::{Page, render_page};
pub use resource::{Resource, Surfaces};
pub use site::{
    AgentsSettings, CanonicalOrigin, CustomSkill, FeedsSettings, LlmsSettings, MarkdownSettings,
    McpSettings, NavSection, PageRecord, SiteInput, SkillSettings,
};
