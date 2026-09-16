//! Reading a built site back off disk, in the shape the §25 agent-readiness
//! checks take.
//!
//! The engine assembles this view internally and drops it; `liyasa test` and
//! `liyasa score` need it after the fact, from whatever is in the output
//! directory. Front matter comes from the source tree because the manifest
//! does not carry titles and descriptions, and the rendered artifacts come from
//! the output because that is what a reader and an agent actually receive.

use std::path::Path;

use liyasa_build::agents::resource::{MARKDOWN, PLAIN_TEXT, Resource, Surfaces};
use liyasa_build::agents::site::{
    AgentsSettings, CanonicalOrigin, FeedsSettings, NavSection, PageRecord, SiteInput,
};
use liyasa_build::agents::spec::BuiltPage;
use liyasa_build::engine::Settings;
use liyasa_config::vfs::OsVfs;
use liyasa_core::ids::{Locale, Route};
use liyasa_core::source_map::SourceMap;
use liyasa_core::vfs::VfsPath;

pub struct Snapshot {
    pub site: SiteInput,
    pub surfaces: Surfaces,
    pub pages: Vec<BuiltPage>,
}

#[derive(Debug)]
pub enum Missing {
    /// No output directory at all.
    Output(String),
    /// The site has no `seo.canonicalOrigin`, without which no agent surface
    /// can be addressed and the checks have nothing to grade.
    Origin,
}

impl Missing {
    pub fn diagnostic(&self) -> liyasa_core::Diagnostic {
        use liyasa_core::diagnostics::code;
        match self {
            Self::Output(path) => {
                liyasa_core::Diagnostic::new(code::E0011, format!("`{path}` does not exist"))
                    .help("Run `liyasa build` first, or pass `--output <dir>`.")
            }
            Self::Origin => liyasa_core::Diagnostic::new(
                code::W0131,
                "`seo.canonicalOrigin` is not set, so the agent surfaces were never written",
            )
            .help("Set `seo.canonicalOrigin` in `liyasa.json` and build again."),
        }
    }
}

/// Reads the built site at `output` for the project rooted at `root`.
pub fn read(root: &Path, output: &Path) -> Result<Snapshot, Missing> {
    if !output.is_dir() {
        return Err(Missing::Output(output.display().to_string()));
    }

    let vfs = OsVfs::new(root);
    let mut sources = SourceMap::new();
    let load = liyasa_config::load(
        &vfs,
        &mut sources,
        &liyasa_config::Options {
            root: VfsPath::new(""),
            env: None,
        },
    );
    let settings = Settings::from_value(&load.value);
    let Some(origin) = CanonicalOrigin::parse(&settings.canonical_origin) else {
        return Err(Missing::Origin);
    };

    let tree = liyasa_build::tree::discover(
        &vfs,
        &mut sources,
        &liyasa_build::tree::Options {
            output: settings.output.clone(),
            drafts: settings.drafts,
        },
    );

    let mut records = Vec::new();
    let mut pages = Vec::new();
    for page in &tree.pages {
        let route = page.route.clone();
        let html = std::fs::read_to_string(html_path(output, &route)).unwrap_or_default();
        let markdown = std::fs::read_to_string(markdown_path(output, &route)).unwrap_or_default();
        if html.is_empty() && markdown.is_empty() {
            continue;
        }

        records.push(PageRecord {
            id: page.front.id,
            route: route.clone(),
            title: page
                .front
                .title
                .clone()
                .unwrap_or_else(|| route.as_str().to_owned()),
            description: page.front.description.clone(),
            locale: Locale::new(settings.locale.clone()),
            version: page.version.clone(),
            tab: None,
            group: None,
            indexable: page.indexing.ai && !page.draft,
            personalized: false,
            markdown: markdown.clone(),
            updated: page.front.updated.clone(),
            changelog: liyasa_build::changelog::is_entry_file(page.path.as_str()),
        });

        let mut built = BuiltPage::new(route, markdown, html);
        // What a static host would send over the wire, which is what the
        // transfer-size checks measure.
        built.transfer_bytes = built.html.len() as u64;
        pages.push(built);
    }

    let site = SiteInput {
        name: settings.name.clone(),
        summary: (!settings.description.is_empty()).then(|| settings.description.clone()),
        origin,
        locale: Locale::new(settings.locale.clone()),
        version: None,
        nav: navigation(&load.value),
        pages: records,
        agents: AgentsSettings::default(),
        feeds: FeedsSettings::default(),
    };

    Ok(Snapshot {
        site,
        surfaces: surfaces(output),
        pages,
    })
}

/// The generated agent surfaces, read back from the output rather than
/// regenerated, so what is graded is what was actually written.
fn surfaces(output: &Path) -> Surfaces {
    let mut resources = Vec::new();
    // The paths carry a leading slash, because that is how a resource is
    // addressed and how the §25 checks look one up. Reading them back under
    // the bare file name made every `llms.txt` check fail on a site that had
    // one.
    for (path, media) in [
        (liyasa_build::agents::llms::ROOT_PATH, PLAIN_TEXT),
        (liyasa_build::agents::llms::FULL_PATH, PLAIN_TEXT),
        (liyasa_build::agents::skill::SKILL_PATH, MARKDOWN),
    ] {
        if let Ok(body) = std::fs::read_to_string(output.join(path.trim_start_matches('/'))) {
            resources.push(Resource::new(path, media, body));
        }
    }

    // The Markdown twin of every page is a resource too: the coverage and
    // parity checks ask whether the routes `llms.txt` names can be fetched.
    let mut stack = vec![output.to_path_buf()];
    while let Some(at) = stack.pop() {
        let Ok(entries) = std::fs::read_dir(&at) else {
            continue;
        };
        for entry in entries.flatten() {
            let path = entry.path();
            if path.is_dir() {
                stack.push(path);
            } else if path.extension().is_some_and(|e| e == "md") {
                if let (Ok(body), Ok(relative)) =
                    (std::fs::read_to_string(&path), path.strip_prefix(output))
                {
                    let route = format!("/{}", relative.to_string_lossy().replace('\\', "/"));
                    if !resources.iter().any(|existing| existing.path == route) {
                        resources.push(Resource::new(route, MARKDOWN, body));
                    }
                }
            }
        }
    }
    Surfaces {
        resources,
        diagnostics: liyasa_core::Diagnostics::new(),
    }
}

/// The navigation as the configuration declares it, which is enough for the
/// coverage checks: they ask which routes are reachable, not how the sidebar
/// renders.
fn navigation(config: &serde_json::Value) -> Vec<NavSection> {
    let Some(groups) = config
        .get("navigation")
        .and_then(serde_json::Value::as_array)
    else {
        return Vec::new();
    };
    let mut out = Vec::new();
    collect(groups, &mut out);
    out
}

fn collect(nodes: &[serde_json::Value], out: &mut Vec<NavSection>) {
    for node in nodes {
        let Some(object) = node.as_object() else {
            continue;
        };
        if let Some(pages) = object.get("pages").and_then(serde_json::Value::as_array) {
            let routes: Vec<Route> = pages
                .iter()
                .filter_map(serde_json::Value::as_str)
                .map(route_of)
                .collect();
            if !routes.is_empty() {
                out.push(NavSection {
                    title: object
                        .get("group")
                        .or_else(|| object.get("tab"))
                        .and_then(serde_json::Value::as_str)
                        .unwrap_or("Pages")
                        .to_owned(),
                    tab: object
                        .get("tab")
                        .and_then(serde_json::Value::as_str)
                        .map(str::to_owned),
                    routes,
                });
            }
            let nested: Vec<serde_json::Value> = pages
                .iter()
                .filter(|page| page.is_object())
                .cloned()
                .collect();
            collect(&nested, out);
        }
    }
}

/// `index` and `guides/install` are both written without a leading slash in
/// the configuration and with one in a route.
fn route_of(page: &str) -> Route {
    let trimmed = page.trim_start_matches('/');
    if trimmed == "index" || trimmed.is_empty() {
        Route::new("/")
    } else {
        Route::new(format!("/{}", trimmed.trim_end_matches("/index")))
    }
}

fn html_path(output: &Path, route: &Route) -> std::path::PathBuf {
    let trimmed = route.as_str().trim_matches('/');
    if trimmed.is_empty() {
        output.join("index.html")
    } else {
        output.join(trimmed).join("index.html")
    }
}

fn markdown_path(output: &Path, route: &Route) -> std::path::PathBuf {
    let trimmed = route.as_str().trim_matches('/');
    if trimmed.is_empty() {
        output.join("index.md")
    } else {
        output.join(format!("{trimmed}.md"))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_configured_page_path_becomes_a_route() {
        assert_eq!(route_of("index").as_str(), "/");
        assert_eq!(route_of("guides/install").as_str(), "/guides/install");
        assert_eq!(route_of("/guides/install").as_str(), "/guides/install");
    }

    #[test]
    fn a_route_names_the_files_the_build_writes() {
        let output = Path::new("/dist");
        assert_eq!(
            html_path(output, &Route::new("/")),
            output.join("index.html")
        );
        assert_eq!(
            html_path(output, &Route::new("/guides/install")),
            output.join("guides/install/index.html")
        );
        assert_eq!(
            markdown_path(output, &Route::new("/guides/install")),
            output.join("guides/install.md")
        );
    }

    #[test]
    fn navigation_is_read_out_of_the_configuration() {
        let config = serde_json::json!({
            "navigation": [
                { "group": "Start", "pages": ["index", "guides/install"] },
                { "group": "Other", "pages": ["checklist"] }
            ]
        });
        let sections = navigation(&config);
        assert_eq!(sections.len(), 2);
        assert_eq!(sections[0].title, "Start");
        assert_eq!(sections[0].routes.len(), 2);
        assert_eq!(sections[0].routes[0].as_str(), "/");
    }
}
