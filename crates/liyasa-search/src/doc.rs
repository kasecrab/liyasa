//! The unit both indexes store: one section of one page (SRC-01).

use liyasa_core::ids::{Locale, Route, Version};
use serde::{Deserialize, Serialize};

/// RX-32's content-type facet.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum DocKind {
    #[default]
    Page,
    Endpoint,
    Changelog,
}

impl DocKind {
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Page => "page",
            Self::Endpoint => "endpoint",
            Self::Changelog => "changelog",
        }
    }

    pub fn parse(text: &str) -> Option<Self> {
        match text {
            "page" => Some(Self::Page),
            "endpoint" => Some(Self::Endpoint),
            "changelog" => Some(Self::Changelog),
            _ => None,
        }
    }
}

/// What an endpoint page adds to its lead document: the parts of an operation
/// a reader searches for but that are not prose (SRC-01).
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct EndpointFacts {
    pub method: String,
    pub path: String,
    pub summary: String,
    pub parameters: Vec<String>,
    /// Status codes as written, `"200"`, `"404"`.
    pub responses: Vec<String>,
}

/// Everything about a page that its sections inherit. The build fills it from
/// front matter, navigation, and the route table; extraction adds only what it
/// reads out of the Rendered AST.
#[derive(Debug, Clone)]
pub struct PageMeta {
    pub route: Route,
    pub title: String,
    pub breadcrumb: Vec<String>,
    pub tab: Option<String>,
    pub version: Option<Version>,
    pub locale: Locale,
    pub kind: DocKind,
    pub keywords: Vec<String>,
    pub groups: Vec<String>,
    pub regions: Vec<String>,
    /// From `search.boost` (§8.6) and front matter `search.boost`, already
    /// resolved for this route.
    pub boost: f32,
    /// Milliseconds since the epoch, from the build clock (§6.6.2). Drives the
    /// recency tie-break of SRC-03; `None` sorts last.
    pub updated: Option<u64>,
    pub endpoint: Option<EndpointFacts>,
}

impl PageMeta {
    pub fn new(route: Route, title: impl Into<String>, locale: Locale) -> Self {
        Self {
            route,
            title: title.into(),
            breadcrumb: Vec::new(),
            tab: None,
            version: None,
            locale,
            kind: DocKind::Page,
            keywords: Vec::new(),
            groups: Vec::new(),
            regions: Vec::new(),
            boost: 1.0,
            updated: None,
            endpoint: None,
        }
    }
}

/// One indexed document. The field list is SRC-01's, and the order the ranker
/// weights them in is SRC-03's.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct SectionDocument {
    pub route: Route,
    /// The heading's anchor, empty for the page's own lead document.
    pub anchor: String,
    /// The page title, repeated on every section so a section is findable by
    /// the page it belongs to.
    pub title: String,
    /// This section's heading, or the page title for the lead document.
    pub section: String,
    pub breadcrumb: Vec<String>,
    pub body: String,
    pub code: String,
    pub keywords: Vec<String>,
    pub tab: Option<String>,
    pub version: Option<Version>,
    pub locale: Locale,
    #[serde(rename = "type")]
    pub kind: DocKind,
    pub boost: f32,
    pub groups: Vec<String>,
    pub regions: Vec<String>,
    pub updated: Option<u64>,
}

impl SectionDocument {
    /// `route#anchor`: the URL a hit navigates to, and the key incremental
    /// indexing (SRC-07) matches documents by.
    pub fn key(&self) -> String {
        if self.anchor.is_empty() {
            self.route.as_str().to_owned()
        } else {
            format!("{}#{}", self.route, self.anchor)
        }
    }
}
