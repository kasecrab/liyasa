//! What a partial is handed (THM-22).
//!
//! Every field here is part of the theme's public surface: an override reads
//! the same context the default does, and a change to these types is a
//! breaking change under semver. [`reference`] documents each partial's keys
//! and `tests/thm_22_context.rs` holds the two to each other.
// TODO(rfc-0010): `PageMeta` and `NavCtx` in liyasa-core are empty frozen
// stubs; these are the types the partials actually receive.

use serde::{Deserialize, Serialize};

use crate::actions::{Action, Placement};
use crate::nav::{Breadcrumbs, Crumb, Link, Navigation, TocEntry};
use crate::strings::Strings;

/// A page layout (§7.7, CM-60). An operator adds a mode by adding a template
/// to `theme/layouts/`, so the set is open (THM-21).
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum Mode {
    #[default]
    Default,
    Wide,
    Custom,
    Frame,
    Center,
    Assistant,
    #[serde(untagged)]
    Named(String),
}

impl Mode {
    /// The six modes §7.7 names.
    pub const BUILT_IN: [&'static str; 6] =
        ["default", "wide", "custom", "frame", "center", "assistant"];

    pub fn name(&self) -> &str {
        match self {
            Self::Default => "default",
            Self::Wide => "wide",
            Self::Custom => "custom",
            Self::Frame => "frame",
            Self::Center => "center",
            Self::Assistant => "assistant",
            Self::Named(name) => name,
        }
    }

    pub fn parse(name: &str) -> Self {
        match name {
            "default" => Self::Default,
            "wide" => Self::Wide,
            "custom" => Self::Custom,
            "frame" => Self::Frame,
            "center" => Self::Center,
            "assistant" => Self::Assistant,
            other => Self::Named(other.to_owned()),
        }
    }

    /// `custom`, `center`, and `frame` drop the sidebar (§7.7).
    pub fn has_sidebar(&self) -> bool {
        matches!(self, Self::Default | Self::Wide | Self::Assistant)
    }

    /// Only `default` and `assistant` keep the right rail (§7.7, RX-23).
    pub fn has_rail(&self) -> bool {
        matches!(self, Self::Default | Self::Assistant)
    }

    /// `frame` keeps a minimal top bar and nothing else.
    pub fn has_chrome(&self) -> bool {
        !matches!(self, Self::Frame)
    }
}

#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(default, rename_all = "camelCase")]
pub struct RenderContext {
    pub page: Page,
    pub site: Site,
    pub nav: Nav,
    pub assets: Assets,
    pub strings: Strings,
    /// The CSP nonce for this response (RX-110).
    pub nonce: String,
}

#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(default, rename_all = "camelCase")]
pub struct Page {
    pub route: String,
    pub id: String,
    pub title: String,
    pub description: String,
    /// The section label shown above the title when breadcrumbs are `eyebrow`.
    pub eyebrow: Option<String>,
    pub mode: Mode,
    /// The rendered page body. Already sanitized (CMP-102); the template marks
    /// it safe rather than escaping it again.
    pub content: String,
    pub toc: Vec<TocEntry>,
    pub breadcrumbs: Vec<Crumb>,
    pub previous: Option<Link>,
    pub next: Option<Link>,
    /// CFG-74, formatted by the build in the site's locale.
    pub last_modified: Option<String>,
    pub actions: Vec<Action>,
    pub actions_placement: Placement,
    /// A `:::panel` replaces the right rail's contents (RX-23).
    pub panel: Option<String>,
    pub markdown_url: String,
    pub mcp_url: Option<String>,
    pub og: Og,
    pub meta: Vec<Meta>,
    /// Rendered on demand for this reader (§6.6.4); `no-store`, never cached.
    pub personalized: bool,
    pub feedback: bool,
}

#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(default, rename_all = "camelCase")]
pub struct Og {
    pub title: String,
    pub description: String,
    /// CFG-73: the generated thumbnail, or the page's `og.image` override.
    pub image: Option<String>,
}

#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct Meta {
    pub name: String,
    pub content: String,
}

#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(default, rename_all = "camelCase")]
pub struct Site {
    pub name: String,
    pub description: String,
    pub logo: Logo,
    pub favicon: Option<String>,
    pub origin: String,
    pub base_path: String,
    pub llms_txt: Option<String>,
    pub version: Option<String>,
    pub locale: String,
    pub direction: String,
    pub navbar: Vec<NavbarLink>,
    pub footer: Footer,
    pub banner: Option<Banner>,
    pub search: bool,
    pub assistant: bool,
    /// THM-40: the OSS distribution may drop the line.
    pub built_with: bool,
    pub appearance: Appearance,
}

#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(default)]
pub struct Logo {
    pub light: Option<String>,
    pub dark: Option<String>,
    pub href: Option<String>,
    pub alt: Option<String>,
}

#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(default)]
pub struct NavbarLink {
    pub label: String,
    pub href: String,
    pub primary: bool,
    pub current: bool,
}

#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(default)]
pub struct Footer {
    pub columns: Vec<FooterColumn>,
    pub social: Vec<NavbarLink>,
    pub note: Option<String>,
}

#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(default)]
pub struct FooterColumn {
    pub title: String,
    pub links: Vec<NavbarLink>,
}

/// CFG-70. `id` is what a dismissal is remembered against, so changing it
/// re-shows a dismissed banner.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(default)]
pub struct Banner {
    pub id: String,
    /// The banner's Markdown, already rendered.
    pub html: String,
    pub dismissible: bool,
}

#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(default)]
pub struct Appearance {
    /// `system`, `light`, or `dark` (CFG-08).
    pub default: String,
    pub strict: bool,
}

#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(default, rename_all = "camelCase")]
pub struct Nav {
    pub navigation: Navigation,
    /// Index into `navigation.tabs` of the tab the page is in.
    pub active_tab: usize,
    pub active_route: String,
    pub breadcrumbs: Breadcrumbs,
}

#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(default, rename_all = "camelCase")]
pub struct Assets {
    /// Hashed URL of the one cached stylesheet (THM-30).
    pub stylesheet: String,
    /// Hashed URL of the base bundle (THM-31).
    pub script: String,
    /// Inlined in `<head>`; matched by a CSP style hash (RX-110).
    pub critical: String,
    /// Inlined with the nonce, before first paint (RX-40).
    pub bootstrap: String,
    /// `theme.css`, appended after the theme's own (CMP-100).
    pub custom_css: Vec<String>,
    /// `theme.js`, deferred after the runtime (CMP-101).
    pub custom_js: Vec<String>,
    /// The JSON `window.liyasa` reads, already escaped for a script element.
    pub page_data: String,
}

/// One partial and the context it may read (THM-22).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct PartialDoc {
    pub partial: &'static str,
    pub doc: &'static str,
    pub keys: &'static [&'static str],
}

/// The documented context of every partial THM-20 names.
pub fn reference() -> &'static [PartialDoc] {
    &[
        PartialDoc {
            partial: "head",
            doc: "Everything inside `<head>`: metadata, the critical block, and the scheme bootstrap.",
            keys: &[
                "page.title",
                "page.description",
                "page.route",
                "page.og",
                "page.meta",
                "page.markdownUrl",
                "page.personalized",
                "site.name",
                "site.origin",
                "site.favicon",
                "site.locale",
                "site.appearance",
                "assets.stylesheet",
                "assets.critical",
                "assets.bootstrap",
                "assets.customCss",
                "nonce",
            ],
        },
        PartialDoc {
            partial: "navbar",
            doc: "The sticky header: logo, top-level links, search trigger, and the appearance toggle.",
            keys: &[
                "site.logo",
                "site.name",
                "site.navbar",
                "site.search",
                "site.appearance",
                "site.assistant",
                "strings",
                "page.actions",
                "page.actionsPlacement",
            ],
        },
        PartialDoc {
            partial: "sidebar",
            doc: "Tabs, version and locale switchers, and the groups of the active tab.",
            keys: &[
                "nav.navigation",
                "nav.activeTab",
                "nav.activeRoute",
                "page.actions",
                "page.actionsPlacement",
                "strings",
            ],
        },
        PartialDoc {
            partial: "sidebar-item",
            doc: "One navigation entry and its children; included recursively.",
            keys: &["item", "nav.activeRoute", "depth"],
        },
        PartialDoc {
            partial: "breadcrumbs",
            doc: "The trail above the page title, or the section eyebrow.",
            keys: &["page.breadcrumbs", "page.eyebrow", "nav.breadcrumbs"],
        },
        PartialDoc {
            partial: "page-header",
            doc: "Title, description, and the last-modified line.",
            keys: &[
                "page.title",
                "page.description",
                "page.eyebrow",
                "page.lastModified",
                "strings",
            ],
        },
        PartialDoc {
            partial: "page-actions",
            doc: "The copy, view, and open-in menu (RX-100).",
            keys: &["page.actions", "page.markdownUrl", "page.mcpUrl", "strings"],
        },
        PartialDoc {
            partial: "content",
            doc: "The rendered page body.",
            keys: &["page.content", "page.mode"],
        },
        PartialDoc {
            partial: "toc",
            doc: "The on-page table of contents (RX-21).",
            keys: &["page.toc", "strings"],
        },
        PartialDoc {
            partial: "pagination",
            doc: "Previous and next in navigation order (RX-22).",
            keys: &["page.previous", "page.next", "strings"],
        },
        PartialDoc {
            partial: "feedback",
            doc: "The was-this-helpful form.",
            keys: &["page.route", "page.feedback", "strings"],
        },
        PartialDoc {
            partial: "footer",
            doc: "Footer columns, social links, and the built-with line.",
            keys: &["site.footer", "site.builtWith", "site.name", "strings"],
        },
        PartialDoc {
            partial: "search",
            doc: "The search overlay's markup; the index reader loads lazily.",
            keys: &["site.search", "site.assistant", "strings"],
        },
        PartialDoc {
            partial: "assistant",
            doc: "The assistant panel's shell.",
            keys: &["site.assistant", "strings"],
        },
        PartialDoc {
            partial: "banner",
            doc: "The site-wide banner (CFG-70).",
            keys: &["site.banner", "strings"],
        },
        PartialDoc {
            partial: "code-block",
            doc: "One code block: chrome, copy button, and the highlighted body.",
            keys: &["code", "strings"],
        },
        PartialDoc {
            partial: "callout",
            doc: "One callout: tone, title, and body.",
            keys: &["callout"],
        },
        PartialDoc {
            partial: "component",
            doc: "The fallback for a component with no partial of its own.",
            keys: &["component"],
        },
    ]
}

/// Every partial name THM-20 lists.
pub fn partial_names() -> Vec<&'static str> {
    reference().iter().map(|doc| doc.partial).collect()
}

impl RenderContext {
    /// Sets the page body, removing anything CMP-102 forbids and reporting
    /// what it removed. Templates mark the body safe, so this is the point
    /// where the theme takes responsibility for what it emits.
    pub fn set_content(&mut self, html: &str) -> liyasa_core::Diagnostics {
        let (clean, diagnostics) = crate::safety::strip_scripts(html);
        self.page.content = clean;
        diagnostics
    }

    /// The same for a `:::panel`, which replaces the right rail (RX-23).
    pub fn set_panel(&mut self, html: &str) -> liyasa_core::Diagnostics {
        let (clean, diagnostics) = crate::safety::strip_scripts(html);
        self.page.panel = Some(clean);
        diagnostics
    }

    /// A context with every field populated, for documentation tests and for
    /// `liyasa theme diff`.
    pub fn sample() -> Self {
        use crate::actions::{Config, Targets, resolve};
        use crate::nav::{Group, Item, Tab};

        let strings = Strings::default();
        let targets = Targets {
            markdown_url: "https://docs.example/guide/install.md".to_owned(),
            markdown: Some("# Install\n".to_owned()),
            mcp_url: Some("https://docs.example/mcp".to_owned()),
            pdf_url: None,
            edit_url: Some("https://github.com/acme/docs/edit/main/install.md".to_owned()),
            suggest_url: None,
        };
        let navigation = Navigation {
            tabs: vec![Tab {
                title: "Guides".to_owned(),
                groups: vec![Group {
                    title: "Get started".to_owned(),
                    expanded: true,
                    items: vec![Item {
                        title: "Install".to_owned(),
                        route: "/guide/install".to_owned(),
                        ..Item::default()
                    }],
                    ..Group::default()
                }],
                ..Tab::default()
            }],
            ..Navigation::default()
        };
        Self {
            page: Page {
                route: "/guide/install".to_owned(),
                id: "01J0000000000000000000000".to_owned(),
                title: "Install".to_owned(),
                description: "Install Liyasa and build the first site.".to_owned(),
                eyebrow: Some("Guides".to_owned()),
                mode: Mode::Default,
                content: "<p>Body</p>".to_owned(),
                toc: vec![TocEntry {
                    level: 2,
                    text: "Requirements".to_owned(),
                    anchor: "requirements".to_owned(),
                    children: Vec::new(),
                }],
                breadcrumbs: navigation.trail("/guide/install"),
                previous: None,
                next: None,
                last_modified: Some("15 September 2026".to_owned()),
                actions: resolve(&Config::default(), &targets, &strings),
                actions_placement: Placement::Header,
                panel: None,
                markdown_url: targets.markdown_url.clone(),
                mcp_url: targets.mcp_url.clone(),
                og: Og {
                    title: "Install".to_owned(),
                    description: "Install Liyasa and build the first site.".to_owned(),
                    image: Some("/og/guide/install.png".to_owned()),
                },
                meta: vec![Meta {
                    name: "robots".to_owned(),
                    content: "index,follow".to_owned(),
                }],
                personalized: false,
                feedback: true,
            },
            site: Site {
                name: "Acme docs".to_owned(),
                description: "Everything about Acme.".to_owned(),
                logo: Logo {
                    light: Some("/logo.svg".to_owned()),
                    dark: Some("/logo-dark.svg".to_owned()),
                    href: Some("/".to_owned()),
                    alt: Some("Acme".to_owned()),
                },
                favicon: Some("/favicon.svg".to_owned()),
                origin: "https://docs.example".to_owned(),
                base_path: String::new(),
                llms_txt: Some("https://docs.example/llms.txt".to_owned()),
                version: Some("v2".to_owned()),
                locale: "en".to_owned(),
                direction: "ltr".to_owned(),
                navbar: vec![NavbarLink {
                    label: "Support".to_owned(),
                    href: "https://acme.example/support".to_owned(),
                    primary: true,
                    current: false,
                }],
                footer: Footer {
                    columns: vec![FooterColumn {
                        title: "Product".to_owned(),
                        links: vec![NavbarLink {
                            label: "Changelog".to_owned(),
                            href: "/changelog".to_owned(),
                            ..NavbarLink::default()
                        }],
                    }],
                    social: Vec::new(),
                    note: None,
                },
                banner: Some(Banner {
                    id: "2026-launch".to_owned(),
                    html: "<p>Acme 2.0 is out</p>".to_owned(),
                    dismissible: true,
                }),
                search: true,
                assistant: true,
                built_with: true,
                appearance: Appearance {
                    default: "system".to_owned(),
                    strict: false,
                },
            },
            nav: Nav {
                active_tab: 0,
                active_route: "/guide/install".to_owned(),
                breadcrumbs: Breadcrumbs::Path,
                navigation,
            },
            assets: Assets {
                stylesheet: "/_liyasa/theme.6f1a2b.css".to_owned(),
                script: "/_liyasa/theme.9c4d1e.js".to_owned(),
                critical: ":root{--ly-color-bg:#fcfdfe}".to_owned(),
                bootstrap: crate::runtime::BOOTSTRAP.to_owned(),
                custom_css: vec!["/brand.css".to_owned()],
                custom_js: vec!["/brand.js".to_owned()],
                page_data: "{}".to_owned(),
            },
            strings,
            nonce: "r4nd0mn0nc3".to_owned(),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_mode_round_trips_through_its_name() {
        for name in Mode::BUILT_IN {
            assert_eq!(Mode::parse(name).name(), name);
        }
        assert_eq!(Mode::parse("gallery"), Mode::Named("gallery".to_owned()));
        assert_eq!(Mode::parse("gallery").name(), "gallery");
    }

    #[test]
    fn the_modes_agree_with_section_7_7() {
        assert!(Mode::Default.has_sidebar() && Mode::Default.has_rail());
        assert!(Mode::Wide.has_sidebar() && !Mode::Wide.has_rail());
        assert!(!Mode::Custom.has_sidebar() && Mode::Custom.has_chrome());
        assert!(!Mode::Frame.has_chrome());
        assert!(!Mode::Center.has_sidebar());
        assert!(Mode::Assistant.has_rail());
    }

    #[test]
    fn the_sample_context_serializes_to_the_documented_shape() {
        let value = serde_json::to_value(RenderContext::sample()).expect("context serializes");
        let object = value.as_object().expect("an object");
        let mut keys: Vec<&str> = object.keys().map(String::as_str).collect();
        keys.sort_unstable();
        assert_eq!(
            keys,
            vec!["assets", "nav", "nonce", "page", "site", "strings"]
        );
        assert!(value["page"]["markdownUrl"].as_str().is_some());
        assert_eq!(value["site"]["builtWith"], serde_json::json!(true));
    }
}
