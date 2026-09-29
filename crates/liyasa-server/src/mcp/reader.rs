//! What the read tools read, and the one seam a host implements.
//!
//! Two hosts implement it: `liyasa serve`, over the bundle it is already
//! serving, and `liyasa mcp --dist` (MCP-04), over a static build's files with
//! no server at all. Keeping the surface behind a trait is what makes those
//! the same MCP server rather than two that drift.
//!
//! **Entitlement is not the reader's choice.** Every method takes a [`Scope`]
//! and every listing is filtered through `auth::groups::decide` — the one
//! decision function AUTH-10 names, not a second opinion. A tool that answered
//! from the bundle directly would hand an agent the pages a signed-out reader
//! is refused, which is the whole risk of putting a query interface in front
//! of a corpus that has restricted parts.

use liyasa_core::ai::TrustLevel;

use crate::auth::groups::SiteDefault;
use crate::auth::session::Principal;

/// Who is asking.
pub struct Scope {
    pub site: SiteDefault,
    /// `None` is an agent with no session, which is the normal case on a
    /// public site (MCP-03).
    pub reader: Option<Principal>,
    /// An agent is `Anonymous` even when it is signed in as somebody: being a
    /// program does not raise trust, and an agent relaying a third party's
    /// text is the injection path §30.2.2 exists for.
    pub trust: TrustLevel,
}

impl Scope {
    /// An unauthenticated agent against a public site.
    pub fn anonymous() -> Self {
        Self {
            site: SiteDefault::Public,
            reader: None,
            trust: TrustLevel::Anonymous,
        }
    }

    pub fn reader(&self) -> Option<&Principal> {
        self.reader.as_ref()
    }
}

/// One page, as a listing names it.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PageRef {
    pub route: String,
    pub title: String,
    /// Depth in the derived tree: `/` is 0, `/guides` is 1, `/guides/install`
    /// is 2. The server carries no navigation — the hierarchy exists only in
    /// the config, at build time, which is why `RouteEntry` ships an access
    /// chain rather than a parent — so `list_pages` derives the tree from the
    /// routes themselves and says so in its description.
    pub depth: u8,
}

/// A page, or one section of it, as an agent is given it.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PageText {
    pub route: String,
    pub title: String,
    /// The section's anchor, or `""` for the whole page.
    pub anchor: String,
    pub markdown: String,
}

/// One search result.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Hit {
    pub route: String,
    pub title: String,
    /// The heading the match sits under, or `""` for the page's own text.
    pub section: String,
    pub anchor: String,
    /// The matching text, trimmed to a line an agent can read.
    pub snippet: String,
}

/// What the site says it is, for `initialize` and the discovery card.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SiteInfo {
    pub name: String,
    pub description: Option<String>,
    /// The canonical origin, with no trailing slash. `None` when the site
    /// declares none, in which case discovery serves relative paths.
    pub origin: Option<String>,
    /// `build.basePath`, as a leading-slash prefix with no trailing slash, or
    /// `""`.
    ///
    /// It is here because the URLs this server publishes have to be the URLs
    /// the rest of the build publishes. `CanonicalOrigin::resource_url` puts
    /// the prefix into every address `llms.txt` advertises, the MCP endpoint
    /// among them, so a server that built its own URLs from the origin alone
    /// would advertise and answer at two different places on any site served
    /// under a prefix — which is defect 162 again, one deployment shape
    /// narrower. `plan/rfcs/1006-who-owns-the-base-path.md` records that the
    /// prefix belongs to `build.basePath` and to nothing else.
    pub base_path: String,
}

/// Why a tool could not answer.
///
/// Every variant becomes a tool result with `isError: true` rather than a
/// JSON-RPC error: the protocol asks for that, because a model can read a
/// tool result and retry, and cannot read a transport failure.
#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
pub enum ToolFailure {
    #[error("{0}")]
    BadInput(String),
    #[error("{0}")]
    NotFound(String),
    /// The mechanism is not present on this host — `report_issue` under
    /// `--dist`, `ask` with no model configured. Names what would supply it.
    #[error("{0}")]
    Unavailable(String),
    #[error("{0}")]
    Internal(String),
}

/// The corpus, filtered for one caller.
pub trait SiteReader: Send + Sync {
    fn site(&self) -> &SiteInfo;

    /// Every page this caller may see, in route order.
    fn pages(&self, scope: &Scope) -> Vec<PageRef>;

    /// One page's Markdown, or one section of it.
    ///
    /// `locator` is a route (`/guides/install`), that route's Markdown twin
    /// (`/guides/install.md`), or an absolute URL on this site — MCP-01 says
    /// "by route or URL" and an agent that found a link in a search result
    /// has the URL, not the route.
    fn page(
        &self,
        locator: &str,
        section: Option<&str>,
        scope: &Scope,
    ) -> Result<PageText, ToolFailure>;

    fn search(&self, query: &str, limit: usize, scope: &Scope) -> Result<Vec<Hit>, ToolFailure>;

    /// One API operation, named `METHOD /path` or by its `operationId`.
    fn operation(&self, operation: &str, scope: &Scope) -> Result<PageText, ToolFailure>;

    /// `llms.txt` as this caller may see it, entries they may not already
    /// dropped. `None` when the build wrote none.
    fn llms_txt(&self, scope: &Scope) -> Option<String>;
}
