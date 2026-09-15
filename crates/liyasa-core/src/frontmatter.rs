//! Front matter (PRD §7.6).
//!
//! `value` keeps the parsed YAML verbatim so unknown keys stay available to
//! templates as `page.<key>`; `typed` is the subset Liyasa itself reads.

use std::collections::BTreeMap;

use serde::{Deserialize, Serialize};

use crate::ids::{Locale, PageId, Route, Version};
use crate::span::Span;

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, schemars::JsonSchema)]
pub struct Frontmatter {
    pub span: Span,
    pub value: serde_json::Value,
    pub typed: FrontmatterFields,
}

/// Page layout (§7.7).
#[derive(
    Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize, schemars::JsonSchema,
)]
#[serde(rename_all = "lowercase")]
pub enum PageMode {
    #[default]
    Default,
    Wide,
    Custom,
    Frame,
    Center,
    Assistant,
}

/// Whether a page on a private site is readable without authentication
/// (AUTH-07). Distinct from the site-level `public` flag.
#[derive(
    Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize, schemars::JsonSchema,
)]
#[serde(rename_all = "lowercase")]
pub enum Access {
    #[default]
    Inherit,
    Public,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, schemars::JsonSchema)]
#[serde(untagged)]
pub enum SearchSetting {
    Enabled(bool),
    Options {
        boost: Option<f32>,
        exclude: Option<bool>,
    },
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, schemars::JsonSchema)]
#[serde(untagged)]
pub enum AiSetting {
    Enabled(bool),
    /// `instructions` are appended to the page's Markdown output.
    Options {
        instructions: Option<String>,
    },
}

#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize, schemars::JsonSchema)]
pub struct RegionGate {
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub only: Option<Vec<String>>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub except: Option<Vec<String>>,
}

#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize, schemars::JsonSchema)]
pub struct SocialMeta {
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub title: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub description: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub image: Option<String>,
}

/// The keys of §7.6 that Liyasa itself reads. Every one is optional; `title`
/// falls back to the first H1 or the file name with a warning.
///
/// Dates are kept as the author wrote them rather than parsed into a calendar
/// type: the build clock (§6.6.2) is the only time source that reaches output,
/// and no date crate is in the dependency table.
#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize, schemars::JsonSchema)]
#[serde(rename_all = "camelCase", default)]
pub struct FrontmatterFields {
    pub title: Option<String>,
    pub id: Option<PageId>,
    pub description: Option<String>,
    pub sidebar_title: Option<String>,
    pub icon: Option<String>,
    pub icon_type: Option<String>,
    pub tag: Option<String>,
    pub mode: Option<PageMode>,
    pub keywords: Vec<String>,
    pub slug: Option<String>,
    /// External URL; the page is a navigation link only and has no body.
    pub url: Option<String>,
    pub noindex: Option<bool>,
    pub search: Option<SearchSetting>,
    pub ai: Option<AiSetting>,
    pub hidden: Option<bool>,
    /// `"spec-id METHOD /path"`.
    pub openapi: Option<String>,
    pub asyncapi: Option<String>,
    pub graphql: Option<String>,
    pub groups: Vec<String>,
    pub access: Option<Access>,
    pub regions: Option<RegionGate>,
    pub locales: Vec<Locale>,
    pub versions: Vec<Version>,
    pub product: Option<String>,
    pub variation: Vec<String>,
    pub og: Option<SocialMeta>,
    pub twitter: Option<SocialMeta>,
    pub canonical: Option<String>,
    /// Page IDs or routes for the related topics block.
    pub related: Vec<String>,
    pub date: Option<String>,
    pub updated: Option<String>,
    pub reviewed: Option<String>,
    /// Declares that the page reads free-form `reader.*` fields and is rendered
    /// on demand (§6.6.4); without it, `reader.*` is `E0208`.
    pub personalized: Option<bool>,
    pub authors: Vec<String>,
    pub draft: Option<bool>,
    pub template: Option<String>,
    /// Mirrors the `verify` schema object (§14).
    pub verify: Option<serde_json::Value>,
    pub facts: BTreeMap<String, serde_json::Value>,
}

impl FrontmatterFields {
    /// The route override implied by `slug`, if any.
    pub fn slug_route(&self, parent: &Route) -> Option<Route> {
        let slug = self.slug.as_deref()?;
        Some(Route::new(format!(
            "{}/{slug}",
            parent.as_str().trim_end_matches('/')
        )))
    }
}
