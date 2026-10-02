//! What a build hands back about each page, for a caller that needs the render
//! rather than the output (RFC 0914, defect 184).
//!
//! `liyasa verify` and the deploy worker both need every page's rendered AST.
//! Without this they re-render each page through `liyasa_lsp::analysis`, which
//! uses the **lenient** context, so `site.*`, `nav.*` and `env.*` are unexpanded
//! inside a fence — a verified code fence then runs code that differs from what
//! the page shows. The build already holds the right render; handing it over
//! costs a clone.
//!
//! The hook is the primitive and the map in `Report` is a collector over it.
//! `verify --changed` wants three pages out of five thousand, so a map would
//! hand over 4,997 documents to be dropped.

use std::fmt;
use std::sync::Arc;

use liyasa_core::document::Document;
use liyasa_core::ids::Route;
use liyasa_core::markdown::ExpansionRecord;

/// One page's render, as the build produced it.
#[derive(Debug, Clone, PartialEq)]
pub struct PageRecord {
    pub document: Document,
    /// What the page read while expanding. `PageExtractor::extract` builds fact
    /// edges from this rather than from the AST, because a fact read inside a
    /// template expression has no node to hang an edge on — so without it the
    /// drift engine's impact walk reaches no page and the facts class reports
    /// nothing on a site where every fact has moved.
    pub expansion: ExpansionRecord,
}

/// What a caller is handed per page. `Send + Sync` because pages render under
/// rayon, so the hook is called from whichever thread finished the page.
type Hook = dyn Fn(&Route, &PageRecord) + Send + Sync;

/// A per-page hook, called once for every page the build placed — rendered or
/// read back from the cache.
///
/// Unordered: pages render under rayon. A caller that needs an order imposes it.
#[derive(Clone)]
pub struct PageHook(Arc<Hook>);

impl PageHook {
    pub fn new(hook: impl Fn(&Route, &PageRecord) + Send + Sync + 'static) -> Self {
        Self(Arc::new(hook))
    }

    pub fn call(&self, route: &Route, record: &PageRecord) {
        (self.0)(route, record);
    }
}

/// `Options` derives `Debug` and a closure has none.
impl fmt::Debug for PageHook {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str("PageHook(..)")
    }
}
