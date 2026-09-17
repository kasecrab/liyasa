//! What the agent surfaces are handed (PRD §11.7, §11.8, §25).
//!
//! Every field names a key of `schemas/liyasa.schema.json` or a column of the
//! build's page table; nothing here is invented. `liyasa-build` maps a
//! `SiteConfig` onto these at integration, so a schema change is a mapping
//! change and not a compile break across the package boundary
//! (`plan/rfcs/1000-agents-crate-seam.md`).

use liyasa_core::ids::{Locale, PageId, Route, Version};
use liyasa_core::markdown::SiteMeta;
use liyasa_core::net::Url;

/// `seo.canonicalOrigin` and `build.basePath`, kept as the base every absolute
/// URL is built from.
///
/// The origin may carry a path (`https://kasecrab.github.io/liyasa`), so URLs
/// are concatenated onto it rather than joined: [`Url::join`] would discard the
/// path for any route with a leading slash.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CanonicalOrigin {
    url: Url,
    base: String,
}

impl CanonicalOrigin {
    pub fn parse(text: &str) -> Option<Self> {
        Self::parse_with_base_path(text, "")
    }

    /// The origin an agent surface publishes, given `seo.canonicalOrigin` and
    /// `build.basePath`.
    ///
    /// The prefix belongs to `build.basePath` and to nothing else
    /// (`plan/rfcs/1006-who-owns-the-base-path.md`): the surfaces used to build
    /// every URL from the origin alone, so a site served under a prefix
    /// published links that resolved nowhere.
    pub fn parse_with_base_path(text: &str, base_path: &str) -> Option<Self> {
        let url = Url::parse(text).ok()?;
        if !matches!(url.scheme(), "http" | "https") || !url.has_host() {
            return None;
        }
        let origin = url.as_str().trim_end_matches('/');
        let prefix = base_path.trim_matches('/');
        let base = match prefix.is_empty() {
            true => origin.to_owned(),
            false => format!("{origin}/{prefix}"),
        };
        Some(Self { url, base })
    }

    pub fn url(&self) -> &Url {
        &self.url
    }

    /// The origin with no trailing slash, such as `https://example.com/docs`.
    pub fn base(&self) -> &str {
        &self.base
    }

    /// The absolute HTML URL of a route.
    pub fn page_url(&self, route: &Route) -> String {
        let path = route.as_str().trim_end_matches('/');
        if path.is_empty() {
            return format!("{}/", self.base);
        }
        format!("{}{path}", self.base)
    }

    /// The absolute Markdown URL of a route. The site root has no `.md` name of
    /// its own, so it takes the `index.md` form (RX-60).
    pub fn markdown_url(&self, route: &Route) -> String {
        let path = route.as_str().trim_end_matches('/');
        if path.is_empty() {
            return format!("{}/index.md", self.base);
        }
        format!("{}{path}.md", self.base)
    }

    /// The absolute URL of a generated resource, given its site-absolute path.
    pub fn resource_url(&self, path: &str) -> String {
        format!("{}/{}", self.base, path.trim_start_matches('/'))
    }

    /// Whether a URL points inside this origin, which is what the spec's
    /// cross-origin `llms.txt` finding asks (§25.1).
    pub fn contains(&self, url: &str) -> bool {
        url.strip_prefix(&self.base)
            .is_some_and(|rest| rest.is_empty() || rest.starts_with('/'))
    }
}

/// One page as the agent surfaces see it: already rendered to Markdown by
/// [`markdown::render_page`](super::markdown::render_page) from the anonymous
/// render (§6.6.4), because reader values never reach a shared surface
/// (SRC-12).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PageRecord {
    pub id: Option<PageId>,
    pub route: Route,
    pub title: String,
    pub description: Option<String>,
    pub locale: Locale,
    pub version: Option<Version>,
    /// The navigation tab this page sits under, used as a split key (RX-70).
    pub tab: Option<String>,
    /// The navigation group, used as a split key when tabs do not divide the
    /// site finely enough (RX-71).
    pub group: Option<String>,
    /// Listed in `llms.txt` and concatenated into `llms-full.txt`: neither
    /// `noindex`, `hidden`, `draft`, nor `ai: false`.
    pub indexable: bool,
    /// Rendered on demand from `reader.*` fields; excluded from every agent
    /// surface (SRC-12).
    pub personalized: bool,
    /// The page's agent Markdown, discovery blockquote and all.
    pub markdown: String,
    /// `updated` front matter, as the author wrote it (§7.6).
    pub updated: Option<String>,
    /// Belongs to a changelog collection, which `feeds.changelog` publishes.
    pub changelog: bool,
}

impl PageRecord {
    /// Whether this page reaches a shared agent surface at all.
    pub fn is_published(&self) -> bool {
        self.indexable && !self.personalized
    }
}

/// One `llms.txt` section, mirroring a navigation group (RX-70).
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct NavSection {
    pub title: String,
    /// The tab this section belongs to, which is how the root index splits.
    pub tab: Option<String>,
    pub routes: Vec<Route>,
}

/// `agents.llms` (§8).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct LlmsSettings {
    /// `agents.llms.custom`: a repository path whose file overrides the
    /// generated root index (RX-72).
    pub custom: Option<String>,
    /// `agents.llms.full`: publish `llms-full.txt`.
    pub full: bool,
    /// `agents.llms.split`: allow splitting into `/_llms/` indexes and parts.
    pub split: bool,
    /// `agents.llms.fullMaxBytes`, default 8 MB.
    pub full_max_bytes: u64,
}

impl Default for LlmsSettings {
    fn default() -> Self {
        Self {
            custom: None,
            full: true,
            split: true,
            full_max_bytes: 8 * 1024 * 1024,
        }
    }
}

/// `agents.markdown` (§8).
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct MarkdownSettings {
    /// `agents.markdown.includeOpenApiSchema` (RX-63).
    pub include_openapi_schema: bool,
    /// `agents.markdown.instructions`: the site-wide "Notes for agents" text
    /// (CM-142).
    pub instructions: Option<String>,
}

/// `agents.skill` (§8).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SkillSettings {
    pub enabled: bool,
    /// `agents.skill.files`: custom skills from `skills/*.md`, as
    /// `(name, body)` pairs already read from the repository.
    pub files: Vec<CustomSkill>,
}

impl Default for SkillSettings {
    fn default() -> Self {
        Self {
            enabled: true,
            files: Vec::new(),
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CustomSkill {
    pub name: String,
    pub description: String,
    pub body: String,
    /// Groups this skill is restricted to on a private site; empty is public
    /// (RX-73).
    pub groups: Vec<String>,
}

/// `agents.mcp` (§8).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct McpSettings {
    pub enabled: bool,
    pub name: Option<String>,
    pub description: Option<String>,
    pub discovery_version: Option<String>,
}

impl Default for McpSettings {
    fn default() -> Self {
        Self {
            enabled: true,
            name: None,
            description: None,
            discovery_version: None,
        }
    }
}

/// `feeds` (§8).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct FeedsSettings {
    pub changelog: bool,
    pub updates: bool,
}

impl Default for FeedsSettings {
    fn default() -> Self {
        Self {
            changelog: true,
            updates: false,
        }
    }
}

/// `agents` (§8).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct AgentsSettings {
    pub llms: LlmsSettings,
    pub markdown: MarkdownSettings,
    pub skill: SkillSettings,
    pub mcp: McpSettings,
    /// `agents.specVersion`: the spec release the check set is held to
    /// (SPEC-03).
    pub spec_version: String,
}

impl Default for AgentsSettings {
    fn default() -> Self {
        Self {
            llms: LlmsSettings::default(),
            markdown: MarkdownSettings::default(),
            skill: SkillSettings::default(),
            mcp: McpSettings::default(),
            spec_version: super::spec::SPEC_VERSION.to_owned(),
        }
    }
}

/// Everything the agent surfaces read, for one locale and version.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SiteInput {
    pub name: String,
    /// The blockquote summary of `llms.txt`; falls back to the site name.
    pub summary: Option<String>,
    pub origin: CanonicalOrigin,
    pub locale: Locale,
    pub version: Option<Version>,
    pub pages: Vec<PageRecord>,
    pub nav: Vec<NavSection>,
    pub agents: AgentsSettings,
    pub feeds: FeedsSettings,
}

impl SiteInput {
    /// The frozen §34.9 view of the same data, for callers that take a
    /// [`SiteMeta`].
    pub fn meta(&self) -> Option<SiteMeta> {
        Some(SiteMeta {
            name: self.name.clone(),
            canonical_origin: self.origin.url().clone(),
            llms_txt: Url::parse(&self.origin.resource_url("llms.txt")).ok()?,
            version: self.version.clone(),
            locale: self.locale.clone(),
        })
    }

    pub fn page(&self, route: &Route) -> Option<&PageRecord> {
        self.pages.iter().find(|p| &p.route == route)
    }

    /// The pages that reach a shared agent surface, in route order.
    pub fn published(&self) -> impl Iterator<Item = &PageRecord> {
        self.pages.iter().filter(|p| p.is_published())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn origin() -> CanonicalOrigin {
        CanonicalOrigin::parse("https://kasecrab.github.io/liyasa").expect("a valid origin")
    }

    /// The URL the in-page layer publishes for a file, which is the one an
    /// agent surface has to agree with.
    fn in_page_url(path: &str, base_path: &str) -> String {
        let plan = crate::assets::plan(
            &[(
                liyasa_core::vfs::VfsPath::new(path),
                liyasa_core::ids::Fingerprint::of("bytes"),
            )],
            &[],
            &crate::assets::Options {
                base_path: base_path.to_owned(),
                ..Default::default()
            },
        );
        plan.entries()
            .first()
            .expect("the planned asset")
            .url
            .clone()
    }

    #[test]
    fn a_base_path_reaches_an_agent_surface_and_an_in_page_link_alike() {
        let origin = CanonicalOrigin::parse_with_base_path("https://example.com", "/docs")
            .expect("a valid origin");

        // Exact strings, not `contains`: the failure mode is a doubled prefix,
        // and `/docs/docs/guide/install.md` contains `/docs` too.
        assert_eq!(
            in_page_url("guide/install.md", "/docs"),
            "/docs/guide/install.md"
        );
        assert_eq!(
            origin.markdown_url(&Route::new("/guide/install")),
            "https://example.com/docs/guide/install.md"
        );
        assert_eq!(
            origin.resource_url("llms.txt"),
            "https://example.com/docs/llms.txt"
        );
        assert_eq!(
            origin.page_url(&Route::new("/guide/install")),
            "https://example.com/docs/guide/install"
        );
    }

    #[test]
    fn the_base_path_appears_exactly_once_in_each_layer() {
        let origin = CanonicalOrigin::parse_with_base_path("https://example.com", "/docs")
            .expect("a valid origin");
        for url in [
            in_page_url("guide/install.md", "/docs"),
            origin.markdown_url(&Route::new("/guide/install")),
            origin.resource_url("llms.txt"),
        ] {
            assert_eq!(url.matches("/docs").count(), 1, "{url}");
        }
    }

    #[test]
    fn an_empty_base_path_leaves_every_url_as_it_was() {
        assert_eq!(
            CanonicalOrigin::parse_with_base_path("https://example.com", ""),
            CanonicalOrigin::parse("https://example.com")
        );
    }

    #[test]
    fn a_base_path_is_taken_with_or_without_its_slashes() {
        let bare = CanonicalOrigin::parse_with_base_path("https://example.com", "docs");
        let slashed = CanonicalOrigin::parse_with_base_path("https://example.com/", "/docs/");
        assert_eq!(
            bare.as_ref().map(CanonicalOrigin::base),
            Some("https://example.com/docs")
        );
        assert_eq!(
            slashed.as_ref().map(CanonicalOrigin::base),
            Some("https://example.com/docs")
        );
    }

    #[test]
    fn a_prefixed_url_is_still_inside_the_origin() {
        let origin = CanonicalOrigin::parse_with_base_path("https://example.com", "/docs")
            .expect("a valid origin");
        assert!(origin.contains("https://example.com/docs/guide/install.md"));
        assert!(!origin.contains("https://example.com/guide/install.md"));
    }

    #[test]
    fn markdown_urls_keep_the_origin_path() {
        let origin = origin();
        assert_eq!(
            origin.markdown_url(&Route::new("/guide/install")),
            "https://kasecrab.github.io/liyasa/guide/install.md"
        );
    }

    #[test]
    fn the_site_root_serializes_as_index_md() {
        assert_eq!(
            origin().markdown_url(&Route::new("/")),
            "https://kasecrab.github.io/liyasa/index.md"
        );
    }

    #[test]
    fn a_sibling_path_is_not_inside_the_origin() {
        let origin = origin();
        assert!(origin.contains("https://kasecrab.github.io/liyasa/guide.md"));
        assert!(!origin.contains("https://kasecrab.github.io/liyasa-other/guide.md"));
        assert!(!origin.contains("https://example.com/liyasa/guide.md"));
    }

    #[test]
    fn a_non_http_origin_is_rejected() {
        assert!(CanonicalOrigin::parse("ftp://example.com").is_none());
        assert!(CanonicalOrigin::parse("not a url").is_none());
    }
}
