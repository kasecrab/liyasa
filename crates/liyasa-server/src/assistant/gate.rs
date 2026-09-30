//! The entitlement gate for the reader-facing tools (AST-11, defect 146).
//!
//! `ServerTools` is the neutral bundle reader its own module doc says it is:
//! `ChunkQuery` reaches the vector store, which applies `groups` during
//! retrieval (RFC 1807), and that covers `search`. It does not cover the three
//! tools that read the bundle directly and never reach the store —
//! `get_page`, `get_openapi` and `list_navigation` — whose only gate was
//! `ChunkQuery.routes`, left empty by `ReaderContext::query()` and read as
//! "no restriction".
//!
//! So this wraps it and asks, per route, the same question a page fetch asks:
//! `groups::decide` over the route's declared chain, which is what
//! `mcp::BundleReader` already does for an agent. Without it, writing defect
//! 146's "missing caller" would have served every restricted page's Markdown
//! and the whole navigation to anyone who asked.
//!
//! **An allow-list would not have worked.** Filling `ChunkQuery.routes` looks
//! like the fix, but a reader entitled to nothing produces an empty list and
//! empty means unrestricted — fail-open for exactly the reader who should see
//! least. The gate is a decision per route, never a list.
//!
//! It lives here rather than inside `ServerTools` because `routes/` is not
//! WP-18's to write, and it belongs here anyway: the neutral reader is what
//! the indexer wants (AST-01 reads every page on purpose), and the gate is a
//! property of answering a reader.

use std::sync::Arc;

use liyasa_ai::assistant::tools::{NavEntry, PageExcerpt, ToolError, Tools};
use liyasa_ai::index::{ChunkQuery, Hit};
use liyasa_core::ids::Route;
use liyasa_core::net::BoxFut;

use crate::auth::groups::{self, SiteDefault};
use crate::auth::session::Principal;
use crate::routes::bundle::Bundle;
use crate::routes::tools::ServerTools;

/// `ServerTools` for one reader, with every bundle read gated.
pub struct GatedTools {
    inner: ServerTools,
    bundle: Arc<Bundle>,
    site: SiteDefault,
    /// `None` is an anonymous reader, which `groups::decide` reads as public
    /// content only.
    reader: Option<Principal>,
}

impl GatedTools {
    pub fn new(
        inner: ServerTools,
        bundle: Arc<Bundle>,
        site: SiteDefault,
        reader: Option<Principal>,
    ) -> Self {
        Self {
            inner,
            bundle,
            site,
            reader,
        }
    }

    /// Whether this reader could browse to `route`.
    fn admits(&self, route: &str) -> bool {
        groups::decide(
            self.site,
            &self.bundle.access_chain(route),
            self.reader.as_ref(),
        )
        .is_allowed()
    }
}

impl Tools for GatedTools {
    /// Delegated unchanged: the store applies `filter.groups` during
    /// retrieval, so a chunk this reader may not see is never scored and there
    /// is nothing to drop afterwards.
    fn search<'a>(
        &'a self,
        query: &'a str,
        filter: &'a ChunkQuery,
    ) -> BoxFut<'a, Result<Vec<Hit>, ToolError>> {
        self.inner.search(query, filter)
    }

    fn get_page<'a>(
        &'a self,
        route: &'a Route,
        section: Option<&'a str>,
    ) -> BoxFut<'a, Result<Option<PageExcerpt>, ToolError>> {
        Box::pin(async move {
            // A page this reader could not browse to is not there, which is
            // the same answer the page route gives them.
            if !self.admits(route.as_str()) {
                return Ok(None);
            }
            self.inner.get_page(route, section).await
        })
    }

    /// Filtered on the way out rather than in: the operation name is resolved
    /// to a route by the bundle, so the route is not known until it answers.
    fn get_openapi<'a>(
        &'a self,
        operation: &'a str,
    ) -> BoxFut<'a, Result<Option<PageExcerpt>, ToolError>> {
        Box::pin(async move {
            Ok(self
                .inner
                .get_openapi(operation)
                .await?
                .filter(|page| self.admits(&page.route)))
        })
    }

    fn list_navigation<'a>(&'a self) -> BoxFut<'a, Result<Vec<NavEntry>, ToolError>> {
        Box::pin(async move {
            // `llms.txt` is the WHOLE site's navigation: the build writes it
            // with no reader in mind, so passing it through lists every
            // restricted page's route and title to anyone. A heading carries
            // no route and stays.
            let mut entries = self.inner.list_navigation().await?;
            entries.retain(|entry| entry.route.is_empty() || self.admits(&entry.route));
            Ok(entries)
        })
    }

    fn get_current_page<'a>(&'a self) -> BoxFut<'a, Result<Option<PageExcerpt>, ToolError>> {
        Box::pin(async move {
            Ok(self
                .inner
                .get_current_page()
                .await?
                .filter(|page| self.admits(&page.route)))
        })
    }
}
