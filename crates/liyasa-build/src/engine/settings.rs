//! The slice of `liyasa.json` the build engine reads.
//!
//! Read from the loaded JSON value rather than from the generated model: the
//! engine wants defaults for everything and cares about a handful of keys, and
//! `liyasa-config` has already validated the document against the schema by the
//! time this runs (CFG-94).

use std::collections::BTreeMap;

use serde_json::Value;

use crate::assets::{Disposition, Hashing};
use crate::images::{Format, Settings as ImageSettings};
use crate::redirects::Input as RedirectInput;
use crate::variants::Caps;

#[derive(Debug, Clone)]
pub struct Settings {
    pub name: String,
    pub description: String,
    pub output: String,
    pub base_path: String,
    pub drafts: bool,
    pub canonical_origin: String,
    pub locale: String,
    pub hashing: Hashing,
    pub downloads: BTreeMap<String, Disposition>,
    pub images: ImageSettings,
    pub caps: Caps,
    pub redirects: Vec<RedirectInput>,
    pub external_allow: Vec<String>,
    /// `build.env`: the only environment variables `env()` may read, and build
    /// inputs in their own right (§6.6.2 rule 6).
    pub env: Vec<String>,
    /// `build.budget.template`: the whole build's template budget (§6.6).
    pub template_budget: std::time::Duration,
    /// `build.budget.templateIncremental`, used when the cache was warm.
    pub incremental_budget: std::time::Duration,
    pub versions: Vec<VersionDecl>,
    pub locales: Vec<String>,
    /// `variables` (CM-24), with `variables.versions.<name>` held back as the
    /// per-version overrides of CM-92
    /// (`plan/rfcs/0606-per-version-variables.md`).
    pub variables: serde_json::Map<String, Value>,
    per_version_variables: serde_json::Map<String, Value>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct VersionDecl {
    pub name: String,
    pub label: String,
    pub default: bool,
    /// The directory the version's tree lives in, when it is not `versions/<name>`.
    pub path: Option<String>,
    /// CM-93's badge. Populated for the default version only until the schema
    /// has a `tag` key (`plan/rfcs/0601-config-keys-the-build-needs.md`).
    pub tag: Option<Tag>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Tag {
    Latest,
    Deprecated,
    Beta,
}

impl Tag {
    pub fn parse(text: &str) -> Option<Self> {
        match text.to_ascii_lowercase().as_str() {
            "latest" => Some(Tag::Latest),
            "deprecated" => Some(Tag::Deprecated),
            "beta" => Some(Tag::Beta),
            _ => None,
        }
    }

    pub fn label(self) -> &'static str {
        match self {
            Tag::Latest => "latest",
            Tag::Deprecated => "deprecated",
            Tag::Beta => "beta",
        }
    }
}

impl Default for Settings {
    fn default() -> Self {
        Self {
            name: "Documentation".to_owned(),
            description: String::new(),
            output: "dist".to_owned(),
            base_path: String::new(),
            drafts: false,
            canonical_origin: String::new(),
            locale: "en".to_owned(),
            hashing: Hashing::None,
            downloads: BTreeMap::new(),
            images: ImageSettings::default(),
            caps: Caps::default(),
            redirects: Vec::new(),
            external_allow: Vec::new(),
            env: Vec::new(),
            template_budget: std::time::Duration::from_secs(60),
            incremental_budget: std::time::Duration::from_secs(10),
            versions: Vec::new(),
            locales: Vec::new(),
            variables: serde_json::Map::new(),
            per_version_variables: serde_json::Map::new(),
        }
    }
}

impl Settings {
    pub fn from_value(value: &Value) -> Self {
        let mut settings = Settings {
            name: string(value, &["name"]).unwrap_or_else(|| "Documentation".to_owned()),
            description: string(value, &["description"]).unwrap_or_default(),
            output: string(value, &["build", "output"]).unwrap_or_else(|| "dist".to_owned()),
            base_path: string(value, &["build", "basePath"]).unwrap_or_default(),
            drafts: bool_at(value, &["build", "drafts"]).unwrap_or(false),
            canonical_origin: string(value, &["seo", "canonicalOrigin"]).unwrap_or_default(),
            hashing: string(value, &["build", "hashing"])
                .as_deref()
                .map(|text| match text {
                    "filename" => Hashing::Filename,
                    "query" => Hashing::Query,
                    // TODO(rfc-0601): the schema's enum has no `"none"`, so an
                    // absent key is what turns hashing off.
                    _ => Hashing::None,
                })
                .unwrap_or_default(),
            ..Settings::default()
        };

        for extension in ["pdf", "zip"] {
            if let Some(text) = string(value, &["build", "downloads", extension]) {
                let disposition = match text.as_str() {
                    "attachment" => Disposition::Attachment,
                    _ => Disposition::Inline,
                };
                settings.downloads.insert(extension.to_owned(), disposition);
            }
        }

        settings.images = ImageSettings {
            breakpoints: numbers(value, &["content", "images", "breakpoints"])
                .unwrap_or_else(|| crate::images::DEFAULT_BREAKPOINTS.to_vec()),
            formats: strings(value, &["content", "images", "formats"])
                .map(|names| names.iter().filter_map(|n| Format::parse(n)).collect())
                .filter(|formats: &Vec<Format>| !formats.is_empty())
                // TODO(rfc-0604): AVIF is planned but not encoded yet.
                .unwrap_or_else(|| vec![Format::Webp]),
            eager: bool_at(value, &["build", "images", "eager"]).unwrap_or(false),
            base_path: settings.base_path.clone(),
        };

        settings.caps = Caps {
            per_page: number(value, &["build", "maxVariantsPerPage"]).unwrap_or(16) as usize,
            site: number(value, &["build", "maxVariants"]).unwrap_or(10_000) as usize,
            iterations: number(value, &["build", "variantDiscoveryIterations"]).unwrap_or(4) as u32,
        };

        settings.env = strings(value, &["build", "env"]).unwrap_or_default();
        if let Some(budget) = string(value, &["build", "budget", "template"])
            .as_deref()
            .and_then(duration)
        {
            settings.template_budget = budget;
        }
        if let Some(budget) = string(value, &["build", "budget", "templateIncremental"])
            .as_deref()
            .and_then(duration)
        {
            settings.incremental_budget = budget;
        }
        (settings.redirects, settings.external_allow) = redirects(value);
        (settings.variables, settings.per_version_variables) = variables(value);
        settings.versions = versions(value);
        settings.locales = locales(value);
        if let Some(first) = settings.locales.first() {
            settings.locale = first.clone();
        }
        settings
    }

    pub fn default_version(&self) -> Option<&VersionDecl> {
        self.versions
            .iter()
            .find(|version| version.default)
            .or_else(|| self.versions.first())
    }

    /// The variables one version's pages expand against: the base, with that
    /// version's overrides on top (CM-92).
    // TODO(rfc-0606): `variables.versions.<name>` is the spelling this package
    // chose; the schema has no key of its own yet.
    pub fn variables_for(&self, version: Option<&str>) -> serde_json::Map<String, Value> {
        let mut out = self.variables.clone();
        let Some(version) = version else {
            return out;
        };
        if let Some(Value::Object(overrides)) = self.per_version_variables.get(version) {
            for (key, value) in overrides {
                out.insert(key.clone(), value.clone());
            }
        }
        out
    }

    pub fn asset_options(&self) -> crate::assets::Options {
        crate::assets::Options {
            hashing: self.hashing,
            downloads: self.downloads.clone(),
            base_path: self.base_path.clone(),
        }
    }
}

/// `500ms`, `30s`, `5m`, `2h`, `180d` — the schema's duration pattern.
pub fn duration(text: &str) -> Option<std::time::Duration> {
    let text = text.trim();
    let (number, unit) = text.split_at(text.find(|ch: char| ch.is_ascii_alphabetic())?);
    let number: u64 = number.parse().ok()?;
    let seconds = match unit {
        "ms" => return Some(std::time::Duration::from_millis(number)),
        "s" => number,
        "m" => number * 60,
        "h" => number * 3_600,
        "d" => number * 86_400,
        _ => return None,
    };
    Some(std::time::Duration::from_secs(seconds))
}

fn at<'a>(value: &'a Value, path: &[&str]) -> Option<&'a Value> {
    let mut current = value;
    for key in path {
        current = current.get(key)?;
    }
    Some(current)
}

fn string(value: &Value, path: &[&str]) -> Option<String> {
    at(value, path)?.as_str().map(str::to_owned)
}

fn bool_at(value: &Value, path: &[&str]) -> Option<bool> {
    at(value, path)?.as_bool()
}

fn number(value: &Value, path: &[&str]) -> Option<i64> {
    at(value, path)?.as_i64()
}

fn numbers(value: &Value, path: &[&str]) -> Option<Vec<u32>> {
    let array = at(value, path)?.as_array()?;
    let out: Vec<u32> = array
        .iter()
        .filter_map(|item| item.as_u64())
        .map(|width| width as u32)
        .collect();
    (!out.is_empty()).then_some(out)
}

fn strings(value: &Value, path: &[&str]) -> Option<Vec<String>> {
    let array = at(value, path)?.as_array()?;
    Some(
        array
            .iter()
            .filter_map(|item| item.as_str().map(str::to_owned))
            .collect(),
    )
}

/// `redirects` is an array in its short form and an object with `rules` in its
/// long one (CM-82).
fn redirects(value: &Value) -> (Vec<RedirectInput>, Vec<String>) {
    let Some(node) = at(value, &["redirects"]) else {
        return (Vec::new(), Vec::new());
    };
    let (rules, allow) = match node {
        Value::Array(rules) => (rules.clone(), Vec::new()),
        Value::Object(_) => (
            node.get("rules")
                .and_then(Value::as_array)
                .cloned()
                .unwrap_or_default(),
            strings(node, &["externalAllow"]).unwrap_or_default(),
        ),
        _ => (Vec::new(), Vec::new()),
    };
    let rules = rules
        .iter()
        .filter_map(|rule| {
            Some(RedirectInput {
                source: rule.get("source")?.as_str()?.to_owned(),
                destination: rule.get("destination")?.as_str()?.to_owned(),
                // TODO(rfc-0601): the PRD spells this `permanent: bool`; the
                // schema has `status`, and both land here.
                status: rule
                    .get("status")
                    .and_then(Value::as_u64)
                    .map(|status| status as u16)
                    .or_else(|| {
                        rule.get("permanent")
                            .and_then(Value::as_bool)
                            .map(|permanent| match permanent {
                                true => crate::redirects::PERMANENT,
                                false => crate::redirects::TEMPORARY,
                            })
                    }),
            })
        })
        .collect();
    (rules, allow)
}

/// `(base variables, per-version overrides)`.
fn variables(
    value: &Value,
) -> (
    serde_json::Map<String, Value>,
    serde_json::Map<String, Value>,
) {
    let Some(Value::Object(map)) = at(value, &["variables"]) else {
        return (serde_json::Map::new(), serde_json::Map::new());
    };
    let mut base = map.clone();
    let per_version = match base.remove("versions") {
        Some(Value::Object(versions)) => versions,
        _ => serde_json::Map::new(),
    };
    (base, per_version)
}

fn versions(value: &Value) -> Vec<VersionDecl> {
    let Some(array) = at(value, &["versions"]).and_then(Value::as_array) else {
        return Vec::new();
    };
    let mut out: Vec<VersionDecl> = array
        .iter()
        .filter_map(|item| match item {
            Value::String(name) => Some(VersionDecl {
                name: name.clone(),
                label: name.clone(),
                default: false,
                path: None,
                tag: None,
            }),
            Value::Object(_) => {
                let name = item.get("name")?.as_str()?.to_owned();
                Some(VersionDecl {
                    label: item
                        .get("label")
                        .and_then(Value::as_str)
                        .unwrap_or(&name)
                        .to_owned(),
                    default: item
                        .get("default")
                        .and_then(Value::as_bool)
                        .unwrap_or(false),
                    path: item.get("path").and_then(Value::as_str).map(str::to_owned),
                    tag: item.get("tag").and_then(Value::as_str).and_then(Tag::parse),
                    name,
                })
            }
            _ => None,
        })
        .collect();

    // CM-90: the first version is the default when none says so, and CM-93's
    // `latest` badge goes to the default until the schema carries a tag.
    if !out.iter().any(|version| version.default)
        && let Some(first) = out.first_mut()
    {
        first.default = true;
    }
    for version in &mut out {
        if version.default && version.tag.is_none() {
            version.tag = Some(Tag::Latest);
        }
    }
    out
}

fn locales(value: &Value) -> Vec<String> {
    let Some(array) = at(value, &["locales"]).and_then(Value::as_array) else {
        return Vec::new();
    };
    let mut default_first: Vec<(bool, String)> = array
        .iter()
        .filter_map(|item| match item {
            Value::String(code) => Some((false, code.clone())),
            Value::Object(_) => Some((
                item.get("default")
                    .and_then(Value::as_bool)
                    .unwrap_or(false),
                item.get("code")?.as_str()?.to_owned(),
            )),
            _ => None,
        })
        .collect();
    default_first.sort_by_key(|(is_default, _)| !is_default);
    default_first.into_iter().map(|(_, code)| code).collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    fn value(json: &str) -> Value {
        serde_json::from_str(json).expect("the fixture is JSON")
    }

    #[test]
    fn an_empty_config_is_all_defaults() {
        let settings = Settings::from_value(&value("{}"));
        assert_eq!(settings.output, "dist");
        assert_eq!(settings.hashing, Hashing::None);
        assert_eq!(settings.caps.per_page, 16);
        assert!(settings.redirects.is_empty());
        assert!(!settings.images.eager);
    }

    #[test]
    fn build_keys_are_read() {
        let settings = Settings::from_value(&value(
            r#"{"build":{"output":"out","basePath":"/docs","hashing":"filename","drafts":true,
                 "maxVariantsPerPage":4,"images":{"eager":true},"env":["API_URL"]}}"#,
        ));
        assert_eq!(settings.output, "out");
        assert_eq!(settings.base_path, "/docs");
        assert_eq!(settings.hashing, Hashing::Filename);
        assert!(settings.drafts);
        assert_eq!(settings.caps.per_page, 4);
        assert!(settings.images.eager);
        assert_eq!(settings.images.base_path, "/docs");
        assert_eq!(settings.env, vec!["API_URL".to_owned()]);
    }

    #[test]
    fn redirects_are_read_in_both_shapes() {
        let short = Settings::from_value(&value(
            r#"{"redirects":[{"source":"/a","destination":"/b"}]}"#,
        ));
        assert_eq!(short.redirects.len(), 1);
        assert!(short.external_allow.is_empty());

        let long = Settings::from_value(&value(
            r#"{"redirects":{"rules":[{"source":"/a","destination":"/b","status":302}],
                 "externalAllow":["status.acme.com"]}}"#,
        ));
        assert_eq!(long.redirects[0].status, Some(302));
        assert_eq!(long.external_allow, vec!["status.acme.com".to_owned()]);
    }

    #[test]
    fn the_prd_spelling_of_permanence_still_lands() {
        let settings = Settings::from_value(&value(
            r#"{"redirects":{"rules":[{"source":"/a","destination":"/b","permanent":false}]}}"#,
        ));
        assert_eq!(settings.redirects[0].status, Some(302));
    }

    #[test]
    fn versions_get_a_default_and_a_latest_tag() {
        let settings = Settings::from_value(&value(
            r#"{"versions":[{"name":"v2","label":"2.x"},{"name":"v1"}]}"#,
        ));
        assert_eq!(settings.versions.len(), 2);
        let default = settings.default_version().expect("a default version");
        assert_eq!(default.name, "v2");
        assert_eq!(default.tag, Some(Tag::Latest));
        assert_eq!(settings.versions[1].tag, None);
    }

    #[test]
    fn an_explicit_default_version_wins() {
        let settings = Settings::from_value(&value(
            r#"{"versions":["v1",{"name":"v2","default":true}]}"#,
        ));
        assert_eq!(
            settings.default_version().map(|v| v.name.as_str()),
            Some("v2")
        );
    }

    #[test]
    fn the_default_locale_comes_first() {
        let settings =
            Settings::from_value(&value(r#"{"locales":["de",{"code":"en","default":true}]}"#));
        assert_eq!(settings.locales, vec!["en".to_owned(), "de".to_owned()]);
        assert_eq!(settings.locale, "en");
    }

    #[test]
    fn the_template_budgets_are_read() {
        let settings = Settings::from_value(&value(
            r#"{"build":{"budget":{"template":"30s","templateIncremental":"500ms"}}}"#,
        ));
        assert_eq!(settings.template_budget, std::time::Duration::from_secs(30));
        assert_eq!(
            settings.incremental_budget,
            std::time::Duration::from_millis(500)
        );
    }

    #[test]
    fn a_duration_reads_every_unit_the_schema_allows() {
        assert_eq!(
            duration("500ms"),
            Some(std::time::Duration::from_millis(500))
        );
        assert_eq!(duration("30s"), Some(std::time::Duration::from_secs(30)));
        assert_eq!(duration("5m"), Some(std::time::Duration::from_secs(300)));
        assert_eq!(duration("2h"), Some(std::time::Duration::from_secs(7_200)));
        assert_eq!(duration("1d"), Some(std::time::Duration::from_secs(86_400)));
        assert_eq!(duration("soon"), None);
    }

    #[test]
    fn variables_are_read_and_a_version_may_override_them() {
        let settings = Settings::from_value(&value(
            r#"{"variables":{"apiUrl":"https://api.acme.com","tier":"cloud",
                 "versions":{"v1":{"apiUrl":"https://api.acme.com/v1"}}}}"#,
        ));
        assert_eq!(settings.variables.len(), 2, "the reserved key is held back");

        let base = settings.variables_for(None);
        assert_eq!(base["apiUrl"], "https://api.acme.com");

        let older = settings.variables_for(Some("v1"));
        assert_eq!(older["apiUrl"], "https://api.acme.com/v1");
        assert_eq!(older["tier"], "cloud", "an override does not drop the rest");

        let current = settings.variables_for(Some("v2"));
        assert_eq!(current["apiUrl"], "https://api.acme.com");
    }

    #[test]
    fn image_formats_and_breakpoints_are_read() {
        let settings = Settings::from_value(&value(
            r#"{"content":{"images":{"breakpoints":[320,640],"formats":["webp"]}}}"#,
        ));
        assert_eq!(settings.images.breakpoints, vec![320, 640]);
        assert_eq!(settings.images.formats, vec![Format::Webp]);
    }
}
