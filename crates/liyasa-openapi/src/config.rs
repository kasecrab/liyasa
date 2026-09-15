//! The `openapi`, `api`, and `playground` config blocks (API-01, API-07,
//! API-20, API-40).
//!
//! `liyasa-config` owns the site config as a whole; this is the shape of the
//! three subtrees that belong to API docs, read from the JSON it produces so
//! that neither crate has to depend on the other.

use serde::{Deserialize, Serialize};

use crate::model::ParameterIn;

/// How a spec's operations are grouped into navigation (API-03).
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum GroupBy {
    #[default]
    Tag,
    /// The first segment of the path: `/users/{id}` groups under `users`.
    PathPrefix,
    None,
}

/// Which readers a whole spec is for (API-51).
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum Visibility {
    #[default]
    Public,
    /// Only readers in one of the listed groups; an empty list is nobody.
    Groups(Vec<String>),
    /// Loaded and validated, but not published.
    Hidden,
}

impl Visibility {
    pub fn allows(&self, reader_groups: &[String]) -> bool {
        match self {
            Self::Public => true,
            Self::Hidden => false,
            Self::Groups(groups) => groups.iter().any(|want| reader_groups.contains(want)),
        }
    }
}

/// One entry of the `openapi` array.
#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", default)]
pub struct SpecConfig {
    /// How navigation and front matter name this spec. Defaults to the
    /// source's file stem.
    pub id: String,
    pub source: String,
    /// The site version (`versions`) this spec documents (API-07).
    pub version: Option<String>,
    /// The site locale this spec documents (API-07).
    pub locale: Option<String>,
    /// Overrides the spec's own `servers` for the playground (API-42).
    pub base_url: Option<String>,
    pub auth: Option<Auth>,
    pub overlays: Vec<String>,
    pub visibility: Visibility,
    /// `"all"`, or the selectors to generate: `"GET /users/{id}"`.
    pub operations: Operations,
    pub group_by: GroupBy,
    /// `x-liyasa.groups` filtering needs reader groups; off unless the site
    /// has them (API-52).
    pub schema_pages: bool,
}

impl SpecConfig {
    /// Reads one entry, which is either the object above or the string
    /// shorthand for `{ source }`.
    pub fn parse(value: &serde_json::Value) -> Result<Self, String> {
        let mut config: Self = match value {
            serde_json::Value::String(source) => Self {
                source: source.clone(),
                ..Self::default()
            },
            other => serde_json::from_value(other.clone()).map_err(|e| e.to_string())?,
        };
        if config.source.is_empty() {
            return Err("`openapi[].source` is required".to_owned());
        }
        if config.id.is_empty() {
            config.id = default_id(&config.source);
        }
        Ok(config)
    }

    /// `<spec>.overlay.yaml` beside the spec, which is applied without being
    /// configured (API-06).
    pub fn discovered_overlay(&self) -> Option<String> {
        let (stem, _) = self.source.rsplit_once('.')?;
        Some(format!("{stem}.overlay.yaml"))
    }
}

/// The identifier a spec gets when the config does not name one: the file
/// stem, so `openapi/payments.yaml` is `payments`.
fn default_id(source: &str) -> String {
    let file = source.rsplit(['/', '\\']).next().unwrap_or(source);
    let stem = file.split_once('.').map_or(file, |(stem, _)| stem);
    if stem.is_empty() {
        "api".to_owned()
    } else {
        stem.to_owned()
    }
}

/// Which of a spec's operations become pages (API-03).
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", untagged)]
pub enum Operations {
    #[default]
    All,
    /// Selectors in `METHOD /path` form.
    Listed(Vec<String>),
}

impl Operations {
    pub fn includes(&self, selector: &str) -> bool {
        match self {
            Self::All => true,
            Self::Listed(listed) => listed.iter().any(|want| want == selector),
        }
    }
}

/// `api.auth`, which powers the playground on pages with no spec (API-20).
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", default)]
pub struct Auth {
    pub method: AuthMethod,
    /// The header, query, or cookie name for `apiKey`.
    pub name: Option<String>,
    #[serde(rename = "in")]
    pub location: Option<ParameterIn>,
}

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum AuthMethod {
    Bearer,
    Basic,
    ApiKey,
    #[default]
    None,
}

/// The `api` block: what a manual endpoint page has instead of a spec.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", default)]
pub struct ApiConfig {
    pub base_url: Option<String>,
    pub auth: Option<Auth>,
}

/// How much of the playground an operation offers (API-40).
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum Display {
    #[default]
    Interactive,
    /// Samples and a response viewer, but no form that sends a request.
    Simple,
    /// The auth controls only, for a spec whose calls have side effects.
    AuthOnly,
    None,
}

impl Display {
    pub fn sends_requests(self) -> bool {
        matches!(self, Self::Interactive)
    }

    pub fn shows_auth(self) -> bool {
        matches!(self, Self::Interactive | Self::AuthOnly)
    }
}

/// The `playground` block.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", default)]
pub struct PlaygroundConfig {
    pub display: Display,
    pub proxy: ProxyConfig,
    /// The sample languages this site offers, in the order the selector shows
    /// them (API-30).
    pub languages: Vec<String>,
    /// Prefill only the required parameters (API-31).
    pub required_only: bool,
}

impl Default for PlaygroundConfig {
    fn default() -> Self {
        Self {
            display: Display::default(),
            proxy: ProxyConfig::default(),
            languages: crate::codegen::DEFAULT_LANGUAGES
                .iter()
                .map(|name| (*name).to_owned())
                .collect(),
            required_only: false,
        }
    }
}

#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", default)]
pub struct ProxyConfig {
    pub enabled: bool,
    /// Hosts allowed in addition to the ones derived from the spec (API-41).
    pub allow: Vec<String>,
}

/// Reads the `openapi` array, reporting each entry that does not read rather
/// than losing the whole block to one bad row.
pub fn specs(value: Option<&serde_json::Value>) -> (Vec<SpecConfig>, Vec<String>) {
    let Some(serde_json::Value::Array(items)) = value else {
        return (Vec::new(), Vec::new());
    };
    let mut specs = Vec::new();
    let mut problems = Vec::new();
    for (index, item) in items.iter().enumerate() {
        match SpecConfig::parse(item) {
            Ok(config) => {
                if specs.iter().any(|other: &SpecConfig| other.id == config.id) {
                    problems.push(format!(
                        "`openapi[{index}]` repeats the id `{}`; ids name specs in navigation",
                        config.id
                    ));
                    continue;
                }
                specs.push(config);
            }
            Err(message) => problems.push(format!("`openapi[{index}]`: {message}")),
        }
    }
    (specs, problems)
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn a_string_entry_is_shorthand_for_its_source() {
        let config = SpecConfig::parse(&json!("openapi/payments.yaml")).expect("it reads");
        assert_eq!(config.source, "openapi/payments.yaml");
        assert_eq!(config.id, "payments", "the id defaults to the file stem");
        assert_eq!(config.operations, Operations::All);
    }

    #[test]
    fn a_url_source_still_gets_an_id_from_its_last_segment() {
        let config =
            SpecConfig::parse(&json!("https://example.com/specs/billing.json")).expect("it reads");
        assert_eq!(config.id, "billing");
    }

    #[test]
    fn the_full_entry_reads_every_key_api_01_names() {
        let config = SpecConfig::parse(&json!({
            "id": "api",
            "source": "openapi/api.yaml",
            "version": "2.0",
            "locale": "de",
            "baseUrl": "https://api.example.com",
            "auth": { "method": "bearer" },
            "overlays": ["openapi/hide-internal.yaml"],
            "visibility": { "groups": ["staff"] },
            "operations": ["GET /users/{id}"],
            "groupBy": "pathPrefix"
        }))
        .expect("it reads");

        assert_eq!(config.version.as_deref(), Some("2.0"));
        assert_eq!(config.locale.as_deref(), Some("de"));
        assert_eq!(config.base_url.as_deref(), Some("https://api.example.com"));
        assert_eq!(config.auth.map(|a| a.method), Some(AuthMethod::Bearer));
        assert_eq!(
            config.overlays,
            vec!["openapi/hide-internal.yaml".to_owned()]
        );
        assert_eq!(config.group_by, GroupBy::PathPrefix);
        assert!(config.operations.includes("GET /users/{id}"));
        assert!(!config.operations.includes("GET /widgets"));
        assert!(config.visibility.allows(&["staff".to_owned()]));
        assert!(!config.visibility.allows(&["customer".to_owned()]));
    }

    #[test]
    fn an_entry_with_no_source_says_so_and_the_others_still_read() {
        let (specs, problems) = specs(Some(&json!([
            "openapi/a.yaml",
            { "id": "b" },
            "openapi/c.yaml"
        ])));
        assert_eq!(specs.len(), 2);
        assert_eq!(problems.len(), 1);
        assert!(problems[0].contains("openapi[1]"), "{}", problems[0]);
    }

    #[test]
    fn two_specs_may_not_share_an_id() {
        let (specs, problems) = specs(Some(&json!([
            { "id": "api", "source": "openapi/a.yaml" },
            { "id": "api", "source": "openapi/b.yaml" }
        ])));
        assert_eq!(specs.len(), 1);
        assert_eq!(problems.len(), 1);
        assert!(problems[0].contains("repeats the id"), "{}", problems[0]);
    }

    #[test]
    fn an_overlay_is_discovered_beside_the_spec() {
        let config = SpecConfig::parse(&json!("openapi/api.yaml")).expect("it reads");
        assert_eq!(
            config.discovered_overlay().as_deref(),
            Some("openapi/api.overlay.yaml")
        );
    }

    #[test]
    fn the_four_display_modes_differ_in_what_they_offer() {
        assert!(Display::Interactive.sends_requests());
        assert!(!Display::Simple.sends_requests());
        assert!(Display::AuthOnly.shows_auth());
        assert!(!Display::AuthOnly.sends_requests());
        assert!(!Display::None.shows_auth());
    }
}
