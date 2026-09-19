//! What the server supplies to the assistant (AST-10).
//!
//! `liyasa_ai::assistant::tools::Tools` had exactly one implementor in the
//! workspace and it was a test fake, so a reader could not ask anything. Every
//! method is a read, and all but one are answered from the deployed bundle:
//! the pages, their Markdown twins and the `llms.txt` the build writes are
//! already the agent-facing view of the site.
//!
//! **Entitlement is applied here, not by the model.** The reader's groups and
//! region reach retrieval and never the prompt (RFC 1807): an access decision
//! a model makes is a suggestion an injected page can argue with, and a chunk
//! that is never retrieved is nothing to leak. The filter is the same
//! `ChunkQuery` the index uses, so the assistant and search cannot disagree
//! about who may see what.

use std::sync::Arc;

use liyasa_ai::assistant::tools::{NavEntry, PageExcerpt, ToolError, Tools};
use liyasa_ai::index::{ChunkQuery, Hit, VectorStore};
use liyasa_core::ids::Route;
use liyasa_core::net::BoxFut;

use super::bundle::Bundle;

/// The tools for one reader's question.
///
/// Built per request rather than shared, because `get_current_page` is about
/// where this reader is and the filter is about what this reader may see.
pub struct ServerTools {
    bundle: Arc<Bundle>,
    /// What this reader may be shown. Applied to retrieval and to page reads.
    filter: ChunkQuery,
    /// The route the reader is on, if the request said.
    current: Option<Route>,
    /// The semantic index, when one is configured. `None` is a site with
    /// nothing indexed, which is a true answer of "no results" rather than a
    /// failure (AST-10).
    index: Option<Arc<dyn VectorStore>>,
    /// The embedding for the query, supplied by the caller because embedding
    /// is the AI package's to do and needs a provider this crate does not hold.
    embed: Option<Arc<dyn Embed>>,
}

/// How many chunks one search returns. The assistant over-fetches and
/// re-ranks above this, so this is the store's page rather than the answer's.
pub const RESULTS: usize = 8;

/// Turns a question into a vector. The provider lives in `liyasa-ai`; this is
/// the seam so that `ServerTools` can be built and tested without one.
pub trait Embed: Send + Sync {
    fn embed<'a>(&'a self, query: &'a str) -> BoxFut<'a, Result<Vec<f32>, String>>;
}

impl std::fmt::Debug for ServerTools {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("ServerTools")
            .field("current", &self.current)
            .field("indexed", &self.index.is_some())
            .finish_non_exhaustive()
    }
}

impl ServerTools {
    pub fn new(bundle: Arc<Bundle>, filter: ChunkQuery) -> Self {
        Self {
            bundle,
            filter,
            current: None,
            index: None,
            embed: None,
        }
    }

    pub fn on_page(mut self, route: Route) -> Self {
        self.current = Some(route);
        self
    }

    pub fn with_index(mut self, index: Arc<dyn VectorStore>, embed: Arc<dyn Embed>) -> Self {
        self.index = Some(index);
        self.embed = Some(embed);
        self
    }

    /// The Markdown twin of a route, as the build wrote it.
    fn markdown(&self, route: &str) -> Option<(String, String)> {
        let entry = self.bundle.route(route)?;
        // A route the filter excludes is not there as far as this reader is
        // concerned. `routes` is empty for "no restriction".
        if !self.filter.routes.is_empty()
            && !self
                .filter
                .routes
                .iter()
                .any(|r| r.as_str() == entry.route.as_str())
        {
            return None;
        }
        let bytes = self.bundle.read(&entry.markdown).ok()?;
        let markdown = String::from_utf8_lossy(&bytes).into_owned();
        let title = first_heading(&markdown).unwrap_or_else(|| entry.route.as_str().to_owned());
        Some((title, markdown))
    }
}

/// The first ATX heading, which is the page's title in every twin the build
/// writes.
fn first_heading(markdown: &str) -> Option<String> {
    markdown
        .lines()
        .find_map(|line| line.strip_prefix("# ").map(|title| title.trim().to_owned()))
}

/// One section of a page, from its heading to the next of the same or higher
/// level. `anchor` is matched against the slug the theme would give a heading.
fn section(markdown: &str, anchor: &str) -> Option<String> {
    let mut out = Vec::new();
    let mut depth = 0usize;
    for line in markdown.lines() {
        let hashes = line.chars().take_while(|c| *c == '#').count();
        let is_heading = hashes > 0 && line.chars().nth(hashes) == Some(' ');
        if out.is_empty() {
            if is_heading && slug(line[hashes + 1..].trim()) == anchor {
                depth = hashes;
                out.push(line);
            }
            continue;
        }
        if is_heading && hashes <= depth {
            break;
        }
        out.push(line);
    }
    (!out.is_empty()).then(|| out.join("\n"))
}

/// The theme's heading slug: lowercase, non-alphanumerics to hyphens, no
/// leading or trailing hyphen.
fn slug(text: &str) -> String {
    let mut out = String::new();
    for c in text.chars() {
        if c.is_alphanumeric() {
            out.extend(c.to_lowercase());
        } else if !out.ends_with('-') {
            out.push('-');
        }
    }
    out.trim_matches('-').to_owned()
}

/// Parses the navigation out of the `llms.txt` the build writes: `## Section`
/// headings and `- [Title](url)` entries.
///
/// That file is the site's own agent-facing index, built from the resolved
/// navigation, so reading it keeps this tool and the published surface in
/// step. The alternative was rebuilding the navigation from the manifest,
/// which carries no titles and no order.
fn navigation_from_llms(text: &str, base: &str) -> Vec<NavEntry> {
    let mut out = Vec::new();
    for line in text.lines() {
        if let Some(heading) = line.strip_prefix("## ") {
            out.push(NavEntry {
                route: String::new(),
                title: heading.trim().to_owned(),
                depth: 0,
            });
            continue;
        }
        let Some(rest) = line.strip_prefix("- [") else {
            continue;
        };
        let Some(close) = rest.find("](") else {
            continue;
        };
        let title = rest[..close].to_owned();
        let after = &rest[close + 2..];
        let Some(end) = after.find(')') else { continue };
        let url = &after[..end];
        // The file carries absolute URLs; the tool wants site-relative routes.
        let route = url
            .split_once("://")
            .and_then(|(_, rest)| rest.find('/').map(|at| rest[at..].to_owned()))
            .unwrap_or_else(|| url.to_owned());
        let route = route.strip_suffix(".md").unwrap_or(&route).to_owned();
        // The twin of `/` is `index.md`, and of `/guides` is
        // `guides/index.md`, so the suffix has to come off or the tool names
        // routes that `get_page` cannot open.
        let route = route.strip_suffix("/index").unwrap_or(&route).to_owned();
        let route = match base.is_empty() {
            true => route,
            false => route.strip_prefix(base).unwrap_or(&route).to_owned(),
        };
        let route = if route.is_empty() {
            "/".to_owned()
        } else {
            route
        };
        out.push(NavEntry {
            route,
            title,
            depth: 1,
        });
    }
    out
}

impl Tools for ServerTools {
    fn search<'a>(
        &'a self,
        query: &'a str,
        filter: &'a ChunkQuery,
    ) -> BoxFut<'a, Result<Vec<Hit>, ToolError>> {
        Box::pin(async move {
            // A site with nothing indexed retrieves nothing. That is a true
            // answer, not a failure: the assistant scores an empty retrieval
            // at 0.0 and deflects, which is the correct outcome and the one
            // an operator should see (RFC 1804). Reporting `Unavailable`
            // instead would make an unindexed site look broken.
            let (Some(index), Some(embed)) = (&self.index, &self.embed) else {
                return Ok(Vec::new());
            };
            // No active index means nothing has been built yet, which is a
            // site with no results rather than a broken one.
            if index
                .active()
                .await
                .map_err(|e| ToolError::Unavailable(e.to_string()))?
                .is_none()
            {
                return Ok(Vec::new());
            }
            let vector = embed.embed(query).await.map_err(ToolError::Unavailable)?;
            // The filter is applied by the store rather than after it, so an
            // entitled chunk is never fetched and then discarded (RFC 1807).
            index
                .query(&vector, RESULTS, filter)
                .await
                .map_err(|e| ToolError::Unavailable(e.to_string()))
        })
    }

    fn get_page<'a>(
        &'a self,
        route: &'a Route,
        section_anchor: Option<&'a str>,
    ) -> BoxFut<'a, Result<Option<PageExcerpt>, ToolError>> {
        Box::pin(async move {
            let Some((title, markdown)) = self.markdown(route.as_str()) else {
                return Ok(None);
            };
            match section_anchor {
                None | Some("") => Ok(Some(PageExcerpt {
                    route: route.as_str().to_owned(),
                    title,
                    anchor: String::new(),
                    markdown,
                })),
                Some(anchor) => Ok(section(&markdown, anchor).map(|markdown| PageExcerpt {
                    route: route.as_str().to_owned(),
                    title,
                    anchor: anchor.to_owned(),
                    markdown,
                })),
            }
        })
    }

    fn get_openapi<'a>(
        &'a self,
        operation: &'a str,
    ) -> BoxFut<'a, Result<Option<PageExcerpt>, ToolError>> {
        Box::pin(async move {
            // The build renders each operation onto a page, so the operation
            // is looked up the way a reader would reach it rather than by
            // re-parsing the specification here.
            let wanted = slug(operation);
            let hit = self.bundle.manifest().routes.iter().find(|entry| {
                let route = entry.route.as_str();
                route.ends_with(&format!("/{wanted}")) || slug(route).ends_with(&wanted)
            });
            let Some(entry) = hit else {
                return Ok(None);
            };
            let Some((title, markdown)) = self.markdown(entry.route.as_str()) else {
                return Ok(None);
            };
            Ok(Some(PageExcerpt {
                route: entry.route.as_str().to_owned(),
                title,
                anchor: String::new(),
                markdown,
            }))
        })
    }

    fn list_navigation<'a>(&'a self) -> BoxFut<'a, Result<Vec<NavEntry>, ToolError>> {
        Box::pin(async move {
            let Ok(bytes) = self.bundle.read("llms.txt") else {
                // A build with no `seo.canonicalOrigin` writes no agent
                // surfaces at all, so this is an empty navigation rather than
                // an error.
                return Ok(Vec::new());
            };
            Ok(navigation_from_llms(
                &String::from_utf8_lossy(&bytes),
                self.bundle.base_path(),
            ))
        })
    }

    fn get_current_page<'a>(&'a self) -> BoxFut<'a, Result<Option<PageExcerpt>, ToolError>> {
        Box::pin(async move {
            let Some(route) = &self.current else {
                return Ok(None);
            };
            self.get_page(route, None).await
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_heading_becomes_the_anchor_the_theme_would_give_it() {
        assert_eq!(slug("Install the CLI"), "install-the-cli");
        assert_eq!(slug("What's new?"), "what-s-new");
        assert_eq!(slug("  Spaces  "), "spaces");
        assert_eq!(slug("--"), "");
    }

    #[test]
    fn a_section_runs_to_the_next_heading_of_its_level_or_higher() {
        let markdown =
            "# Page\n\nLead.\n\n## Install\n\nOne.\n\n### Detail\n\nTwo.\n\n## Next\n\nThree.\n";
        let install = section(markdown, "install").expect("the section");
        assert!(install.contains("One."));
        assert!(
            install.contains("### Detail"),
            "a deeper heading stays inside"
        );
        assert!(install.contains("Two."));
        assert!(!install.contains("Three."), "the next sibling ends it");
        assert!(!install.contains("Lead."));

        assert_eq!(section(markdown, "absent"), None);
    }

    #[test]
    fn the_title_is_the_first_heading() {
        assert_eq!(
            first_heading("---\ntitle: x\n---\n\n# Install\n\nBody\n").as_deref(),
            Some("Install")
        );
        assert_eq!(first_heading("no heading here"), None);
    }

    #[test]
    fn the_navigation_comes_out_of_the_published_index() {
        let llms = "# Acme\n\n> Docs.\n\n## Guides\n\n- [Install](https://docs.acme.com/guides/install.md): How to.\n- [Upgrade](https://docs.acme.com/guides/upgrade.md)\n\n## Reference\n\n- [API](https://docs.acme.com/api.md)\n";
        let nav = navigation_from_llms(llms, "");
        assert_eq!(nav.len(), 5);
        assert_eq!(nav[0].title, "Guides");
        assert_eq!(nav[0].depth, 0);
        assert_eq!(nav[0].route, "");
        assert_eq!(nav[1].title, "Install");
        assert_eq!(
            nav[1].route, "/guides/install",
            "the .md twin maps to its route"
        );
        assert_eq!(nav[1].depth, 1);
        assert_eq!(nav[4].route, "/api");
    }

    #[test]
    fn the_home_pages_twin_maps_back_to_the_root_route() {
        // `/index.md` is the twin of `/`, not of `/index`. Naming a route
        // `get_page` cannot open is worse than omitting it.
        let llms = "## Start\n\n- [Home](https://docs.acme.com/index.md)\n- [Guides](https://docs.acme.com/guides/index.md)\n";
        let nav = navigation_from_llms(llms, "");
        assert_eq!(nav[1].route, "/");
        assert_eq!(nav[2].route, "/guides");
    }

    #[test]
    fn a_base_path_is_stripped_from_a_navigation_route() {
        let llms = "## Guides\n\n- [Install](https://example.com/docs/guides/install.md)\n";
        let nav = navigation_from_llms(llms, "/docs");
        assert_eq!(nav[1].route, "/guides/install");
    }
}
