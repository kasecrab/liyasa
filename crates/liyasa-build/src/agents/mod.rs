//! The agent surfaces: Markdown routes, `llms.txt`, skills, feeds, and the
//! agent-readiness report (PRD §11.7, §11.8, §11.9, §25).
//!
//! Everything here is produced from the anonymous render of a page (§6.6.4), so
//! no reader value can reach a shared surface (SRC-12).
//!
//! The order of a build is: render each page with [`markdown::render_page`],
//! measure it with [`size::check`], then hand the set to [`surfaces`], which
//! writes every generated file. [`spec::run`] then grades what was written.

pub mod continuation;
pub mod feeds;
pub mod geo;
pub mod llms;
pub mod markdown;
pub mod notfound;
pub mod resource;
pub mod routes;
pub mod site;
pub mod size;
pub mod skill;
pub mod spec;

use liyasa_core::build::BuildClock;

pub use markdown::{Page, render_page};
pub use resource::{Resource, Surfaces};
pub use site::{
    AgentsSettings, CanonicalOrigin, CustomSkill, FeedsSettings, LlmsSettings, MarkdownSettings,
    McpSettings, NavSection, PageRecord, SiteInput, SkillSettings,
};

/// What a surface build needs beyond the site itself.
pub struct Options<'a> {
    /// The one timestamp of §6.6.2 rule 1; feeds read it and nothing reads the
    /// wall clock.
    pub clock: BuildClock,
    /// `errors/404.md`, already rendered to agent Markdown (RX-82).
    pub not_found: Option<&'a str>,
}

/// Every file an agent can fetch, for one locale and version.
pub fn surfaces(site: &SiteInput, options: &Options<'_>) -> Surfaces {
    let mut out = routes::generate(site);
    out.absorb(llms::generate(site));
    out.absorb(skill::generate(site));
    out.absorb(notfound::generate(site, options.not_found));
    out.absorb(feeds::generate(site, options.clock));
    out
}

#[cfg(test)]
mod tests {
    use std::time::{Duration, UNIX_EPOCH};

    use liyasa_core::ids::{Locale, Route};

    use super::*;

    fn page(route: &str, title: &str, changelog: bool) -> PageRecord {
        PageRecord {
            id: None,
            route: Route::new(route),
            title: title.to_owned(),
            description: Some(format!("What {title} is for.")),
            locale: Locale::new("en"),
            version: None,
            tab: None,
            group: None,
            indexable: true,
            personalized: false,
            markdown: format!(
                "> For AI agents: a documentation index is available at \
                 https://example.com/llms.txt\n\n# {title}\n\nBody of {title}.\n"
            ),
            updated: Some("2026-09-15".to_owned()),
            changelog,
        }
    }

    fn site() -> SiteInput {
        SiteInput {
            name: "Liyasa".to_owned(),
            summary: Some("Documentation.".to_owned()),
            origin: CanonicalOrigin::parse("https://example.com").expect("a valid origin"),
            locale: Locale::new("en"),
            version: None,
            pages: vec![
                page("/guide/install", "Install", false),
                page("/changelog/2026-09", "September 2026", true),
            ],
            nav: vec![NavSection {
                title: "Guide".to_owned(),
                tab: None,
                routes: vec![Route::new("/guide/install")],
            }],
            agents: AgentsSettings::default(),
            feeds: FeedsSettings::default(),
        }
    }

    fn options() -> Options<'static> {
        Options {
            clock: BuildClock(UNIX_EPOCH + Duration::from_secs(1_789_473_600)),
            not_found: None,
        }
    }

    #[test]
    fn a_build_writes_every_surface_the_packet_names() {
        let surfaces = surfaces(&site(), &options());
        for path in [
            "/guide/install.md",
            "/guide/install/index.md",
            "/llms.txt",
            "/llms-full.txt",
            "/skill.md",
            "/.well-known/skills/liyasa.md",
            "/.well-known/agent-card.json",
            "/404.md",
            "/404.html",
            "/changelog.xml",
            "/changelog.atom",
            "/changelog.json",
        ] {
            assert!(surfaces.get(path).is_some(), "{path}");
        }
        assert!(
            !surfaces.diagnostics.has_errors(),
            "{:?}",
            surfaces.diagnostics
        );
    }

    #[test]
    fn no_two_surfaces_claim_the_same_path() {
        let surfaces = surfaces(&site(), &options());
        let mut seen = std::collections::BTreeSet::new();
        for path in surfaces.paths() {
            assert!(seen.insert(path), "{path} is written twice");
        }
    }

    #[test]
    fn a_custom_404_reaches_the_build() {
        let options = Options {
            not_found: Some("# Gone\n"),
            ..options()
        };
        let surfaces = surfaces(&site(), &options);
        assert_eq!(surfaces.get("/404.md").expect("the 404").body, "# Gone");
    }
}
