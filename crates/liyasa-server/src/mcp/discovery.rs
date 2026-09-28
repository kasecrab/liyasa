//! The server card (MCP-02).
//!
//! Served at `/.well-known/mcp` **and** `/.well-known/mcp.json`, the same
//! document at both, because the ecosystem has used both spellings and an
//! agent that guessed the other one would conclude this site has no server.
//! One document rather than a redirect: a redirect between well-known paths
//! is one more thing for a minimal client to get wrong.
//!
//! ## Versioning
//!
//! MCP's discovery format is not settled, so the card says which shape it is
//! in rather than leaving a reader to infer it. `agents.mcp.discoveryVersion`
//! names the shape, defaults to [`DISCOVERY_VERSION`], and appears in the
//! document as `discoveryVersion`. MCP-02's policy for when the format
//! changes — serve the new shape at the new path, keep the previous one for
//! two minor releases — is a policy about future paths; what belongs in the
//! code today is that the current document is labelled, so the transition has
//! something to be conditional on. RFC 1901 records the rest.

use serde_json::{Value, json};

use super::protocol;
use super::reader::SiteInfo;

/// `application/json`, not a bespoke type: every client that fetches a
/// well-known document already parses JSON, and a type nothing recognises is
/// a type something refuses.
pub const CONTENT_TYPE: &str = "application/json";

/// The discovery shape this build writes, when the site names none.
pub const DISCOVERY_VERSION: &str = "2025-06-18";

/// What the site configured under `agents.mcp`.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Settings {
    pub enabled: bool,
    pub name: Option<String>,
    pub description: Option<String>,
    pub discovery_version: Option<String>,
}

impl Default for Settings {
    fn default() -> Self {
        // `enabled` defaults to true, matching `liyasa-build`'s `McpSettings`.
        // A site that says nothing publishes the server, which is what the
        // generated `llms.txt` already promises on its behalf.
        Self {
            enabled: true,
            name: None,
            description: None,
            discovery_version: None,
        }
    }
}

impl Settings {
    /// Reads `agents.mcp` out of the site config the server was given.
    ///
    /// Read from the raw config rather than through `liyasa-build`'s parsed
    /// `McpSettings`, because that type is built from a `SiteInput` the
    /// server never assembles — it has the config's JSON and nothing else.
    pub fn read(config: &Value) -> Self {
        let mcp = &config["agents"]["mcp"];
        let text = |key: &str| {
            mcp.get(key)
                .and_then(Value::as_str)
                .map(str::trim)
                .filter(|value| !value.is_empty())
                .map(str::to_owned)
        };
        Self {
            enabled: mcp
                .get("enabled")
                .and_then(Value::as_bool)
                .unwrap_or(Self::default().enabled),
            name: text("name"),
            description: text("description"),
            discovery_version: text("discoveryVersion"),
        }
    }

    pub fn version(&self) -> &str {
        self.discovery_version
            .as_deref()
            .unwrap_or(DISCOVERY_VERSION)
    }
}

/// Builds the card. `endpoint` and `index` are absolute when the site
/// declares a canonical origin and site-relative when it does not — a
/// relative URL still resolves against the document a client just fetched,
/// which is better than an absolute one built on a guessed host.
pub fn card(site: &SiteInfo, settings: &Settings, endpoint: &str, index: &str) -> Value {
    let name = settings
        .name
        .clone()
        .unwrap_or_else(|| format!("{} documentation", site.name));
    let description = settings
        .description
        .clone()
        .or_else(|| site.description.clone())
        .unwrap_or_else(|| {
            format!(
                "Search and read the documentation for {} through MCP tools rather than by \
                 crawling it.",
                site.name
            )
        });
    json!({
        "discoveryVersion": settings.version(),
        "name": name,
        "description": description,
        "protocolVersion": protocol::PROTOCOL_VERSION,
        "protocolVersions": protocol::SUPPORTED,
        "servers": [{
            "name": name,
            "transport": "streamable-http",
            "url": endpoint,
            "authentication": "none"
        }],
        "capabilities": {
            "tools": true,
            "resources": true,
            "prompts": true
        },
        "documentation": index
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    fn site() -> SiteInfo {
        SiteInfo {
            name: "Acme docs".to_owned(),
            description: Some("How Acme works".to_owned()),
            origin: Some("https://docs.acme.com".to_owned()),
        }
    }

    #[test]
    fn a_site_that_configures_nothing_publishes_the_server() {
        // The generated `llms.txt` advertises the endpoint whenever
        // `agents.mcp.enabled` is true, and its default is true. A different
        // default here would have the index promising a 404.
        assert!(Settings::read(&json!({})).enabled);
        assert!(Settings::read(&json!({ "agents": {} })).enabled);
        assert!(!Settings::read(&json!({ "agents": { "mcp": { "enabled": false } } })).enabled);
    }

    #[test]
    fn a_blank_name_is_not_a_name() {
        let settings = Settings::read(&json!({ "agents": { "mcp": { "name": "   " } } }));
        assert_eq!(settings.name, None);
        let document = card(&site(), &settings, "https://docs.acme.com/mcp", "/llms.txt");
        assert_eq!(document["name"], json!("Acme docs documentation"));
    }

    #[test]
    fn the_card_says_which_shape_it_is_in() {
        // MCP's discovery format is unsettled; a document that did not label
        // itself would leave the next format change with nothing to be
        // conditional on (RFC 1901).
        let document = card(
            &site(),
            &Settings::default(),
            "https://docs.acme.com/mcp",
            "/llms.txt",
        );
        assert_eq!(document["discoveryVersion"], json!(DISCOVERY_VERSION));

        let pinned = Settings::read(&json!({
            "agents": { "mcp": { "discoveryVersion": "2099-01-01" } }
        }));
        assert_eq!(
            card(&site(), &pinned, "https://docs.acme.com/mcp", "/llms.txt")["discoveryVersion"],
            json!("2099-01-01")
        );
    }

    #[test]
    fn the_card_names_the_endpoint_it_was_built_with() {
        let document = card(
            &site(),
            &Settings::default(),
            "https://docs.acme.com/mcp",
            "https://docs.acme.com/llms.txt",
        );
        assert_eq!(
            document["servers"][0]["url"],
            json!("https://docs.acme.com/mcp")
        );
        assert_eq!(
            document["servers"][0]["transport"],
            json!("streamable-http")
        );
        assert_eq!(
            document["documentation"],
            json!("https://docs.acme.com/llms.txt")
        );
    }

    #[test]
    fn the_cards_protocol_version_is_the_one_the_dispatcher_answers_with() {
        // Two constants that drifted would advertise a revision the server
        // then refuses at `initialize`.
        let document = card(&site(), &Settings::default(), "/mcp", "/llms.txt");
        assert_eq!(
            document["protocolVersion"],
            json!(protocol::PROTOCOL_VERSION)
        );
        assert!(
            protocol::SUPPORTED.contains(&protocol::PROTOCOL_VERSION),
            "the preferred version must be one of the supported ones"
        );
    }
}
