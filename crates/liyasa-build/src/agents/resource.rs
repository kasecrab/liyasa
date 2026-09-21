//! One generated file and the set of them.
//!
//! Agent surfaces are files with a path, a media type, and sometimes an
//! audience: a skill restricted to a group on a private site is served to that
//! group and to nobody else (RX-73).

use liyasa_core::diagnostics::Diagnostics;

pub const MARKDOWN: &str = "text/markdown; charset=utf-8";
pub const PLAIN_TEXT: &str = "text/plain; charset=utf-8";
pub const JSON: &str = "application/json; charset=utf-8";
pub const RSS: &str = "application/rss+xml; charset=utf-8";
pub const ATOM: &str = "application/atom+xml; charset=utf-8";
pub const FEED_JSON: &str = "application/feed+json; charset=utf-8";

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Resource {
    /// Site-absolute path, such as `/llms.txt`.
    pub path: String,
    pub media_type: &'static str,
    /// Groups this resource is served to; empty is public.
    pub groups: Vec<String>,
    pub body: String,
    /// For a listing, which route occupies which bytes of `body`, so the
    /// server can drop what a reader may not see. Empty for a resource that
    /// lists nothing.
    pub entries: Vec<crate::manifest::ListingEntry>,
}

impl Resource {
    pub fn new(path: impl Into<String>, media_type: &'static str, body: impl Into<String>) -> Self {
        Self {
            path: path.into(),
            media_type,
            groups: Vec::new(),
            body: body.into(),
            entries: Vec::new(),
        }
    }

    #[must_use]
    pub fn listing(mut self, entries: Vec<crate::manifest::ListingEntry>) -> Self {
        self.entries = entries;
        self
    }

    #[must_use]
    pub fn restricted_to(mut self, groups: Vec<String>) -> Self {
        self.groups = groups;
        self
    }

    pub fn is_public(&self) -> bool {
        self.groups.is_empty()
    }
}

#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct Surfaces {
    pub resources: Vec<Resource>,
    pub diagnostics: Diagnostics,
}

impl Surfaces {
    pub fn get(&self, path: &str) -> Option<&Resource> {
        self.resources.iter().find(|r| r.path == path)
    }

    pub fn paths(&self) -> impl Iterator<Item = &str> {
        self.resources.iter().map(|r| r.path.as_str())
    }

    /// Folds another set in, keeping both diagnostics lists.
    pub fn absorb(&mut self, other: Surfaces) {
        self.resources.extend(other.resources);
        self.diagnostics.extend(other.diagnostics);
    }
}
