//! The reference site: the pages every budget is measured against and the e2e
//! suite drives.
//!
//! It is a real site rather than a fixture string. The pages are Markdown in
//! `tests/content/`, rendered to HTML and assembled by `liyasa-theme` with the
//! runtime this repository builds, so a budget measures what a reader would be
//! served. What is missing is the build engine (WP-06): routes, assets, and
//! the page context are assembled here instead, and this module is the place
//! that changes when `liyasa-build` can produce the same site.

use liyasa_theme::actions::{Config as ActionConfig, Targets, resolve};
use liyasa_theme::config::ThemeConfig;
use liyasa_theme::context::{
    Assets, Footer, FooterColumn, Logo, Mode, NavbarLink, RenderContext, page_data,
};
use liyasa_theme::nav::{Group, Item, Link, Navigation, Tab, TocEntry};
use liyasa_theme::runtime::{BOOTSTRAP, Runtime};
use liyasa_theme::stylesheet::Styles;
use liyasa_theme::theme::Theme;
use liyasa_theme::tokens::Tokens;

/// The bundle `web/reader` builds. Committed, so a Rust-only checkout still
/// renders the site; `budget/thm_31.rs` holds it to the build that made it.
pub const READER: &str = include_str!("../../web/reader/dist/reader.js");
/// The field-metric collector the vitals run injects (RX-11).
pub const MEASURE: &str = include_str!("../../web/reader/dist/measure.js");

/// Where the reference site is published (§33.1 item 8: there is no domain).
pub const ORIGIN: &str = "https://kasecrab.github.io";

/// Without a `<link rel="icon">` the browser asks for `/favicon.ico`, and the
/// 404 it gets is a console error Lighthouse counts against best practices
/// (RX-10). The reference site therefore ships one, like any real site.
pub const FAVICON_URL: &str = "/favicon.svg";

pub const FAVICON: &str = concat!(
    r#"<svg xmlns="http://www.w3.org/2000/svg" viewBox="0 0 16 16">"#,
    r##"<rect width="16" height="16" rx="3" fill="#1f2933"/>"##,
    r##"<path d="M5 4v8h6" fill="none" stroke="#fff" stroke-width="2" stroke-linecap="square"/>"##,
    "</svg>\n"
);

pub const STYLESHEET_URL: &str = "/_liyasa/theme.css";
pub const SCRIPT_URL: &str = "/_liyasa/base.js";
pub const READER_URL: &str = "/_liyasa/reader.js";

struct Source {
    route: &'static str,
    title: &'static str,
    description: &'static str,
    markdown: &'static str,
}

const SOURCES: &[Source] = &[
    Source {
        route: "/",
        title: "Liyasa",
        description: "Documentation that builds from Markdown in your repository.",
        markdown: include_str!("../content/index.md"),
    },
    Source {
        route: "/guide/install",
        title: "Install Liyasa",
        description: "Everything the build needs is a supported platform and a terminal.",
        markdown: include_str!("../content/guide/install.md"),
    },
    Source {
        route: "/guide/configuration",
        title: "Configuration",
        description: "One file configures a site, and a published schema documents it.",
        markdown: include_str!("../content/guide/configuration.md"),
    },
    Source {
        route: "/reference/cli",
        title: "CLI reference",
        description: "Every command, every flag, and what each exit code means.",
        markdown: include_str!("../content/reference/cli.md"),
    },
];

pub struct Page {
    pub route: String,
    pub title: String,
    /// What `<route>.md` serves, and what the conversion ratio counts.
    pub markdown: String,
    /// The whole response body.
    pub html: String,
}

pub struct Site {
    pub pages: Vec<Page>,
    pub stylesheet: String,
    /// The theme's own base bundle (THM-31).
    pub script: String,
    /// `web/reader`'s bundle, loaded after it.
    pub reader: String,
}

impl Site {
    pub fn page(&self, route: &str) -> Option<&Page> {
        self.pages.iter().find(|page| page.route == route)
    }
}

/// Renders every page of the reference site.
pub fn build() -> Result<Site, Box<dyn std::error::Error>> {
    let config = ThemeConfig::default();
    let tokens = Tokens::aurora();
    let styles = Styles::build(&config, &tokens, &[])?;
    let runtime = Runtime::build(&config);
    let theme = Theme::new()?;
    let navigation = navigation();

    let mut pages = Vec::with_capacity(SOURCES.len());
    for (at, source) in SOURCES.iter().enumerate() {
        let content = to_html(source.markdown);
        let mut context = RenderContext::sample();
        context.page.route = source.route.to_owned();
        context.page.title = source.title.to_owned();
        context.page.description = source.description.to_owned();
        context.page.og.title = source.title.to_owned();
        context.page.og.description = source.description.to_owned();
        context.page.og.image = None;
        context.page.eyebrow = None;
        context.page.mode = Mode::Default;
        context.page.toc = toc(&content);
        context.page.content = content;
        context.page.markdown_url = markdown_url(source.route);
        context.page.mcp_url = None;
        context.page.actions = resolve(
            &ActionConfig::default(),
            &Targets {
                markdown_url: format!("{ORIGIN}{}", markdown_url(source.route)),
                markdown: Some(source.markdown.to_owned()),
                mcp_url: None,
                pdf_url: None,
                edit_url: None,
                suggest_url: None,
            },
            &context.strings,
        );
        context.page.breadcrumbs = navigation.trail(source.route);
        context.page.previous = at.checked_sub(1).and_then(|at| SOURCES.get(at)).map(link);
        context.page.next = SOURCES.get(at + 1).map(link);
        context.site.name = "Liyasa".to_owned();
        context.site.description = "Documentation that builds from Markdown.".to_owned();
        context.site.origin = ORIGIN.to_owned();
        context.site.base_path = String::new();
        context.site.banner = None;
        // Everything the reference site renders has to exist in it: a logo
        // file or an assistant module that 404s would be measured as part of
        // the page (RX-10, RX-11).
        context.site.logo = Logo {
            href: Some("/".to_owned()),
            ..Logo::default()
        };
        context.site.favicon = Some(FAVICON_URL.to_owned());
        context.site.llms_txt = None;
        context.site.version = None;
        context.site.assistant = false;
        context.site.navbar = Vec::new();
        context.site.footer = Footer {
            columns: vec![FooterColumn {
                title: "Documentation".to_owned(),
                links: SOURCES
                    .iter()
                    .skip(1)
                    .map(|source| NavbarLink {
                        label: source.title.to_owned(),
                        href: source.route.to_owned(),
                        ..NavbarLink::default()
                    })
                    .collect(),
            }],
            ..Footer::default()
        };
        context.nav.navigation = navigation.clone();
        context.nav.active_route = source.route.to_owned();
        context.nav.active_tab = usize::from(source.route.starts_with("/reference"));
        context.assets = Assets {
            stylesheet: STYLESHEET_URL.to_owned(),
            script: SCRIPT_URL.to_owned(),
            critical: styles.critical.clone(),
            bootstrap: BOOTSTRAP.to_owned(),
            custom_css: Vec::new(),
            custom_js: vec![READER_URL.to_owned()],
            page_data: String::new(),
            search_module: None,
            assistant_module: None,
        };
        context.assets.page_data = page_data(&context);

        pages.push(Page {
            route: source.route.to_owned(),
            title: source.title.to_owned(),
            markdown: source.markdown.to_owned(),
            html: theme.render_page(&context)?,
        });
    }

    Ok(Site {
        pages,
        stylesheet: styles.css,
        script: runtime.base,
        reader: READER.to_owned(),
    })
}

/// RX-60: `<route>.md`, and `/index.md` for the root.
fn markdown_url(route: &str) -> String {
    if route == "/" {
        "/index.md".to_owned()
    } else {
        format!("{route}.md")
    }
}

fn link(source: &Source) -> Link {
    Link {
        title: source.title.to_owned(),
        route: source.route.to_owned(),
    }
}

fn item(source: &Source) -> Item {
    Item {
        title: source.title.to_owned(),
        route: source.route.to_owned(),
        ..Item::default()
    }
}

fn navigation() -> Navigation {
    let guides = Tab {
        title: "Guides".to_owned(),
        groups: vec![Group {
            title: "Get started".to_owned(),
            expanded: true,
            items: SOURCES
                .iter()
                .filter(|source| !source.route.starts_with("/reference"))
                .map(item)
                .collect(),
            ..Group::default()
        }],
        ..Tab::default()
    };
    let reference = Tab {
        title: "Reference".to_owned(),
        groups: vec![Group {
            title: "Command line".to_owned(),
            expanded: true,
            items: SOURCES
                .iter()
                .filter(|source| source.route.starts_with("/reference"))
                .map(item)
                .collect(),
            ..Group::default()
        }],
        ..Tab::default()
    };
    Navigation {
        tabs: vec![guides, reference],
        ..Navigation::default()
    }
}

fn options() -> comrak::Options<'static> {
    let mut options = comrak::Options::default();
    options.extension.table = true;
    options.extension.strikethrough = true;
    options.extension.autolink = true;
    options.extension.tasklist = true;
    options.extension.footnotes = true;
    options.extension.description_lists = true;
    // Anchors are what the table of contents links to (RX-21).
    options.extension.header_id_prefix = Some(String::new());
    options.render.github_pre_lang = true;
    options
}

fn to_html(markdown: &str) -> String {
    strip_heading_anchors(&comrak::markdown_to_html(markdown, &options()))
}

/// comrak decorates every heading with a labelled anchor link, 140 bytes a
/// heading that Liyasa's own renderer does not emit — the theme's stylesheet
/// draws the anchor. Leaving it in would put a parser's decoration inside the
/// page budget.
fn strip_heading_anchors(html: &str) -> String {
    let mut out = String::with_capacity(html.len());
    let mut rest = html;
    while let Some(at) = rest.find("<a href=\"#") {
        let Some(end) = rest[at..].find("</a>").map(|end| at + end + "</a>".len()) else {
            break;
        };
        if rest[at..end].contains("class=\"anchor\"") {
            out.push_str(&rest[..at]);
        } else {
            out.push_str(&rest[..end]);
        }
        rest = &rest[end..];
    }
    out.push_str(rest);
    out
}

/// H2 and H3, the depth RX-21 takes by default, read back from the rendered
/// HTML so the anchors are the ones the page actually carries.
fn toc(html: &str) -> Vec<TocEntry> {
    let mut flat: Vec<(usize, TocEntry)> = Vec::new();
    for (level, tag) in [(2u8, "<h2 id=\""), (3, "<h3 id=\"")] {
        for (at, _) in html.match_indices(tag) {
            let rest = &html[at + tag.len()..];
            let Some((anchor, rest)) = rest.split_once('"') else {
                continue;
            };
            let Some((inner, _)) = rest.split_once("</h") else {
                continue;
            };
            flat.push((
                at,
                TocEntry {
                    level,
                    text: text_of(inner),
                    anchor: anchor.to_owned(),
                    children: Vec::new(),
                },
            ));
        }
    }
    flat.sort_by_key(|(at, _)| *at);
    nest(flat.into_iter().map(|(_, entry)| entry).collect())
}

/// An H3 belongs to the H2 above it (RX-21).
fn nest(flat: Vec<TocEntry>) -> Vec<TocEntry> {
    let mut out: Vec<TocEntry> = Vec::new();
    for entry in flat {
        match out.last_mut() {
            Some(parent) if entry.level == 3 => parent.children.push(entry),
            _ => out.push(entry),
        }
    }
    out
}

fn text_of(html: &str) -> String {
    let mut out = String::with_capacity(html.len());
    let mut depth = 0usize;
    for character in html.chars() {
        match character {
            '<' => depth += 1,
            '>' => depth = depth.saturating_sub(1),
            _ if depth == 0 => out.push(character),
            _ => {}
        }
    }
    out.trim().to_owned()
}
