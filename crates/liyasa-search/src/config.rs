//! The `search` object of `liyasa.json` (CFG-50..CFG-54, §8.6).
//!
//! Declared here rather than taken from `liyasa-config`: §34.7 puts
//! `liyasa-search` below it in the dependency tree, so the keys are mirrored
//! from `schemas/liyasa.schema.json`, which is the source of truth (CFG-94),
//! and `tests/cfg_50.rs` holds the two to each other.

use serde::{Deserialize, Serialize};

use crate::doc::DocKind;
use crate::glob::Glob;
use crate::idx::query::Filters;
use crate::idx::search::SearchOptions;
use crate::idx::writer::WriterOptions;

/// CFG-51: a rule that boosts or de-prioritizes what it matches.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct BoostRule {
    /// A path glob or a reader-group name.
    #[serde(rename = "match")]
    pub matches: String,
    /// Above 1 promotes, below 1 de-prioritizes.
    pub factor: f32,
}

/// CFG-53's facets.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum Facet {
    Tab,
    Version,
    Locale,
    Type,
}

/// CFG-54.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum SearchMode {
    /// tantivy and `liyasa-idx` only.
    #[default]
    Keyword,
    /// Keyword results fused with embedding similarity, when the server is
    /// present (SRC-06).
    Hybrid,
}

/// CFG-53's `search.shardSize`, as written: `{ min: "200KB", max: "2MB" }`.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ShardSize {
    pub min: String,
    pub max: String,
}

impl Default for ShardSize {
    fn default() -> Self {
        Self {
            min: "200KB".to_owned(),
            max: "2MB".to_owned(),
        }
    }
}

impl ShardSize {
    pub fn min_bytes(&self) -> u64 {
        parse_bytes(&self.min).unwrap_or(200 * 1024)
    }

    pub fn max_bytes(&self) -> u64 {
        parse_bytes(&self.max).unwrap_or(2 * 1024 * 1024)
    }
}

/// A byte size as the schema's pattern spells it: `512MB`, `1.5 GB`.
pub fn parse_bytes(text: &str) -> Option<u64> {
    let text = text.trim();
    let split = text
        .find(|c: char| !c.is_ascii_digit() && c != '.')
        .unwrap_or(text.len());
    let (number, unit) = text.split_at(split);
    let number: f64 = number.parse().ok()?;
    let scale = match unit.trim().to_ascii_uppercase().as_str() {
        "" | "B" => 1u64,
        "KB" => 1024,
        "MB" => 1024 * 1024,
        "GB" => 1024 * 1024 * 1024,
        _ => return None,
    };
    Some((number * scale as f64) as u64)
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", default)]
pub struct SearchSettings {
    /// CFG-50. Localizable by the theme; the index does not read it.
    pub placeholder: Option<String>,
    /// CFG-55, owned by the theme; carried so a round trip through this type
    /// does not drop the key.
    pub shortcut: Option<String>,
    pub boost: Vec<BoostRule>,
    pub exclude: Vec<String>,
    pub max_results: usize,
    pub snippets: bool,
    pub filters: Vec<Facet>,
    pub shard_size: ShardSize,
    pub mode: SearchMode,
}

impl Default for SearchSettings {
    fn default() -> Self {
        Self {
            placeholder: None,
            shortcut: None,
            boost: Vec::new(),
            exclude: Vec::new(),
            // The schema's defaults, which the CFG-53 test holds it to.
            max_results: 20,
            snippets: true,
            filters: Vec::new(),
            shard_size: ShardSize::default(),
            mode: SearchMode::default(),
        }
    }
}

impl SearchSettings {
    /// CFG-52: is this route kept out of the index?
    pub fn excludes(&self, route: &str) -> bool {
        self.exclude
            .iter()
            .any(|pattern| Glob::new(pattern.clone()).matches(route))
    }

    /// CFG-51: the factor for a route. Rules compose by multiplication, so
    /// `{ "/api/**": 2 }` and `{ "/api/legacy/**": 0.5 }` together leave the
    /// legacy subtree where it started rather than fighting over it.
    pub fn boost_for(&self, route: &str, groups: &[String]) -> f32 {
        let mut factor = 1.0;
        for rule in &self.boost {
            let hit = Glob::new(rule.matches.clone()).matches(route)
                || groups.iter().any(|group| group == &rule.matches);
            if hit && rule.factor > 0.0 {
                factor *= rule.factor;
            }
        }
        factor
    }

    /// Patterns that matched nothing in this build, for `W1005`. A typo in a
    /// boost is otherwise silent.
    pub fn unmatched(&self, routes: &[String]) -> Vec<String> {
        let mut out = Vec::new();
        for pattern in self
            .boost
            .iter()
            .map(|rule| &rule.matches)
            .chain(self.exclude.iter())
        {
            let glob = Glob::new(pattern.clone());
            if !routes.iter().any(|route| glob.matches(route)) {
                out.push(pattern.clone());
            }
        }
        out.sort();
        out.dedup();
        out
    }

    pub fn writer_options(&self) -> WriterOptions {
        WriterOptions {
            shard_min_bytes: self.shard_size.min_bytes(),
            shard_max_bytes: self.shard_size.max_bytes(),
            snippets: self.snippets,
            ..WriterOptions::default()
        }
    }

    pub fn search_options(&self) -> SearchOptions {
        SearchOptions {
            max_results: self.max_results,
            snippets: self.snippets,
            ..SearchOptions::default()
        }
    }

    /// Drops filters the operator did not enable, so a REST caller cannot
    /// facet by a dimension the site does not offer.
    pub fn allow(&self, filters: &mut Filters) {
        let enabled = |facet: Facet| self.filters.is_empty() || self.filters.contains(&facet);
        if !enabled(Facet::Tab) {
            filters.tab = None;
        }
        if !enabled(Facet::Version) {
            filters.version = None;
        }
        if !enabled(Facet::Locale) {
            filters.locale = None;
        }
        if !enabled(Facet::Type) {
            filters.kind = None;
        }
    }

    pub fn is_hybrid(&self) -> bool {
        self.mode == SearchMode::Hybrid
    }
}

impl Facet {
    pub const fn as_str(self) -> &'static str {
        match self {
            Facet::Tab => "tab",
            Facet::Version => "version",
            Facet::Locale => "locale",
            Facet::Type => "type",
        }
    }

    /// The content types RX-32's `type` facet offers.
    pub const KINDS: [DocKind; 3] = [DocKind::Page, DocKind::Endpoint, DocKind::Changelog];
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn byte_sizes_parse_the_way_the_schema_writes_them() {
        assert_eq!(parse_bytes("200KB"), Some(200 * 1024));
        assert_eq!(parse_bytes("2MB"), Some(2 * 1024 * 1024));
        assert_eq!(parse_bytes("1.5 GB"), Some(1_610_612_736));
        assert_eq!(parse_bytes("512"), Some(512));
        assert_eq!(parse_bytes("512B"), Some(512));
        assert_eq!(parse_bytes("nonsense"), None);
        assert_eq!(parse_bytes("10TB"), None);
    }

    #[test]
    fn the_default_shard_range_is_the_one_cfg_53_names() {
        let size = ShardSize::default();
        assert_eq!(size.min_bytes(), 200 * 1024);
        assert_eq!(size.max_bytes(), 2 * 1024 * 1024);
    }

    #[test]
    fn an_exclude_glob_keeps_a_route_out() {
        let settings = SearchSettings {
            exclude: vec!["/internal/**".to_owned()],
            ..SearchSettings::default()
        };
        assert!(settings.excludes("/internal/runbook"));
        assert!(!settings.excludes("/guides/auth"));
    }

    #[test]
    fn a_boost_matches_a_path_or_a_group() {
        let settings = SearchSettings {
            boost: vec![
                BoostRule {
                    matches: "/api/**".to_owned(),
                    factor: 2.0,
                },
                BoostRule {
                    matches: "beta".to_owned(),
                    factor: 0.5,
                },
            ],
            ..SearchSettings::default()
        };
        assert_eq!(settings.boost_for("/api/users", &[]), 2.0);
        assert_eq!(settings.boost_for("/guides/auth", &[]), 1.0);
        assert_eq!(
            settings.boost_for("/guides/auth", &["beta".to_owned()]),
            0.5
        );
        assert_eq!(settings.boost_for("/api/users", &["beta".to_owned()]), 1.0);
    }

    #[test]
    fn a_pattern_that_matched_nothing_is_reported() {
        let settings = SearchSettings {
            boost: vec![BoostRule {
                matches: "/apu/**".to_owned(),
                factor: 2.0,
            }],
            exclude: vec!["/internal/**".to_owned()],
            ..SearchSettings::default()
        };
        let routes = vec!["/api/users".to_owned(), "/internal/runbook".to_owned()];
        assert_eq!(settings.unmatched(&routes), ["/apu/**"]);
    }

    #[test]
    fn the_settings_round_trip_through_the_config_shape() {
        let json = r#"{
            "placeholder": "Search the docs",
            "shortcut": "mod+k",
            "boost": [{ "match": "/api/**", "factor": 2 }],
            "exclude": ["/internal/**"],
            "maxResults": 8,
            "snippets": false,
            "filters": ["tab", "version"],
            "shardSize": { "min": "100KB", "max": "1MB" },
            "mode": "hybrid"
        }"#;
        let settings: SearchSettings = serde_json::from_str(json).expect("parses");
        assert_eq!(settings.max_results, 8);
        assert!(!settings.snippets);
        assert_eq!(settings.filters, [Facet::Tab, Facet::Version]);
        assert_eq!(settings.shard_size.max_bytes(), 1024 * 1024);
        assert!(settings.is_hybrid());
        assert_eq!(settings.boost[0].factor, 2.0);

        let back = serde_json::to_string(&settings).expect("serializes");
        assert_eq!(
            serde_json::from_str::<SearchSettings>(&back).expect("parses again"),
            settings
        );
    }

    #[test]
    fn an_empty_object_is_the_documented_defaults() {
        let settings: SearchSettings = serde_json::from_str("{}").expect("parses");
        assert_eq!(settings, SearchSettings::default());
        assert_eq!(settings.max_results, 20);
        assert!(settings.snippets);
        assert_eq!(settings.mode, SearchMode::Keyword);
    }

    #[test]
    fn options_are_derived_from_the_settings() {
        let settings = SearchSettings {
            max_results: 5,
            snippets: false,
            shard_size: ShardSize {
                min: "64KB".to_owned(),
                max: "512KB".to_owned(),
            },
            ..SearchSettings::default()
        };
        assert_eq!(settings.search_options().max_results, 5);
        assert!(!settings.writer_options().snippets);
        assert_eq!(settings.writer_options().shard_max_bytes, 512 * 1024);
    }

    #[test]
    fn a_facet_the_site_did_not_enable_is_dropped() {
        let settings = SearchSettings {
            filters: vec![Facet::Version],
            ..SearchSettings::default()
        };
        let mut filters = Filters {
            tab: Some("docs".to_owned()),
            version: Some("v2".to_owned()),
            ..Filters::default()
        };
        settings.allow(&mut filters);
        assert_eq!(filters.version.as_deref(), Some("v2"));
        assert_eq!(filters.tab, None, "`tab` is not in `search.filters`");
    }

    #[test]
    fn no_filters_configured_means_every_facet_is_available() {
        let mut filters = Filters {
            tab: Some("docs".to_owned()),
            kind: Some(DocKind::Endpoint),
            ..Filters::default()
        };
        SearchSettings::default().allow(&mut filters);
        assert_eq!(filters.tab.as_deref(), Some("docs"));
        assert_eq!(filters.kind, Some(DocKind::Endpoint));
    }
}
