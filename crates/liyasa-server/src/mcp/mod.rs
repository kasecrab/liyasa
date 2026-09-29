//! The MCP server (MCP-01..05).
//!
//! A documentation site is read by agents at least as often as by people, and
//! an agent that has to scrape HTML gets a worse answer than one that can ask.
//! This subtree is the ask: the same corpus the reader sees, the same
//! entitlement filter, over JSON-RPC.
//!
//! ## Where it answers
//!
//! `/mcp`, and `/_liyasa/mcp` as an alias (RFC 1900). `/mcp` is canonical
//! because that is what MCP-01 says and what every generated `llms.txt`
//! already publishes to agents; the alias exists because `routes::pool_for`
//! charges the `Mcp` rate-limit bucket on the `/_liyasa/` spelling. Serving
//! only one of the two would leave either the advertised URL a 404 or agent
//! traffic charged to the human page pool.
//!
//! ## Layers
//!
//! - [`jsonrpc`] — the JSON-RPC 2.0 envelope. Knows nothing about MCP.
//! - [`protocol`] — what a method name means. Knows nothing about HTTP.
//! - [`tools`], [`resources`], [`prompts`] — the surface, as declarations.
//! - [`reader`] — the one trait a host supplies to answer them.
//!
//! The split is what lets the same surface serve both transports: `liyasa
//! serve` mounts it over Streamable HTTP, and `liyasa mcp --dist` (MCP-04)
//! runs the same dispatcher over stdio against a static build.

pub mod bundle_reader;
pub mod jsonrpc;
pub mod markdown;
pub mod openapi;
pub mod prompts;
pub mod protocol;
pub mod reader;
pub mod resources;
pub mod tools;
pub mod discovery;
pub mod feedback;
pub mod http;
pub mod stdio;

use std::sync::Arc;

use liyasa_core::diagnostics::{Diagnostic, Diagnostics, code};

use crate::routes::AppState;
use crate::routes::bundle::{Bundle, Target};
use crate::routes::mount::Mount;

use bundle_reader::BundleReader;
use reader::SiteInfo;

/// The subtree, as `routes::mount::subtrees()` mounts it (RFC 1403).
///
/// Mounting nothing is a first-class answer here. An instance with no bundle
/// has no corpus for a tool to read, and a site that turned `agents.mcp` off
/// asked for no server — in both cases the reason is recorded and reported at
/// startup rather than answered per request.
pub fn mount(app: &Arc<AppState>) -> Mount {
    let Some(bundle) = app.bundle.clone() else {
        return Mount::skipped("this instance serves no site, and every MCP tool reads one");
    };
    let settings = discovery::Settings::read(&app.config.site_config);
    if !settings.enabled {
        return Mount::skipped("`agents.mcp.enabled` is false in this site's configuration");
    }

    let info = site_info(&app.config.site_config, bundle.base_path());
    let diagnostics = shadowed(&bundle, &info.base_path);
    let card = discovery::card(
        &info,
        &settings,
        &absolute(&info, http::PATH),
        &absolute(&info, resources::LLMS_TXT),
    );
    let base_path = info.base_path.clone();
    let state = Arc::new(http::McpState {
        app: app.clone(),
        reader: Arc::new(BundleReader::new(bundle, info)),
        card,
        base_path,
    });
    Mount::routes(http::router(state)).with_diagnostics(diagnostics)
}

/// W0818: a page of the site that this subtree's routes take precedence over.
///
/// `/mcp` is in the SITE's namespace, which is what `/_liyasa/` exists to
/// avoid, and `routes::application` ends in `.fallback(page)` — an explicit
/// axum route beats a fallback, so a site with a real page at `/mcp` has that
/// page silently replaced by a JSON-RPC endpoint. A documentation site about
/// the Model Context Protocol is not a strange thing to exist.
///
/// Declining to serve `/mcp` on such a site was the other option and is
/// worse: the endpoint's existence would depend on the site's content, so the
/// URL every generated `llms.txt` publishes would be right on most sites and
/// wrong on some, silently. Loud precedence beats conditional existence
/// (RFC 1900).
///
/// Raised once, here, at startup — not per request, and not at the
/// composition point, which is another package's file and a long way from the
/// cause.
/// `base_path` is the prefix the endpoint also answers under, because that is
/// where `llms.txt` advertises it. The well-known paths are checked WITHOUT
/// it: a well-known URI is defined relative to the origin's root (RFC 8615)
/// and this server serves them there whatever the site's prefix is, so a page
/// at `<base>/.well-known/mcp` shadows nothing and must not be warned about.
fn shadowed(bundle: &Bundle, base_path: &str) -> Diagnostics {
    let mut diagnostics = Diagnostics::new();
    let mut paths = vec![
        format!("{base_path}{}", http::PATH),
        format!("{base_path}{}", http::ALIAS),
    ];
    paths.extend(http::WELL_KNOWN.iter().map(|path| (*path).to_owned()));
    for path in paths {
        if !matches!(bundle.resolve(&path, false), Target::NotFound) {
            diagnostics.push(Diagnostic::new(
                code::W0818,
                format!(
                    "this site has something at `{path}`, and the MCP server answers there \
                     instead: an explicit route takes precedence over the page fallback"
                ),
            ));
        }
    }
    diagnostics
}

/// What the site says it is, out of the config the server was given and the
/// prefix the bundle was built under.
pub(crate) fn site_info(config: &serde_json::Value, base_path: &str) -> SiteInfo {
    let text = |value: &serde_json::Value| {
        value
            .as_str()
            .map(str::trim)
            .filter(|value| !value.is_empty())
            .map(str::to_owned)
    };
    SiteInfo {
        name: text(&config["name"]).unwrap_or_else(|| "Documentation".to_owned()),
        description: text(&config["description"]),
        origin: text(&config["seo"]["canonicalOrigin"])
            .map(|origin| origin.trim_end_matches('/').to_owned()),
        base_path: match base_path.trim_matches('/') {
            "" => String::new(),
            prefix => format!("/{prefix}"),
        },
    }
}

/// A site path as an absolute URL when the site declares an origin, and as a
/// site-relative path when it does not — carrying `build.basePath` either way,
/// so it is the same address `CanonicalOrigin::resource_url` publishes.
///
/// A relative URL still resolves against the document a client just fetched,
/// which is better than an absolute one built on a host this process only
/// guessed at — a server behind a proxy sees its own bind address, not the
/// name the agent used.
fn absolute(info: &SiteInfo, path: &str) -> String {
    let path = format!("{}{path}", info.base_path);
    match &info.origin {
        Some(origin) => format!("{origin}{path}"),
        None => path,
    }
}
