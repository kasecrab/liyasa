//! The run's options, named as the reference tool names them (SPEC-01).
//!
//! The option names are `afdocs`'s (`maxLinksToTest`, `samplingStrategy`,
//! `thresholds`, `coverageExclusions`, `parityExclusions`) so that the two
//! tools can be pointed at the same site with the same settings and their
//! reports compared check for check.

use serde::{Deserialize, Serialize};

use crate::agents::size;

/// How pages were chosen. Explicit selection turns off the insufficient-data
/// rule: an operator who named four URLs meant those four.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum Sampling {
    /// Discover and sample; the insufficient-data rule applies.
    #[default]
    Auto,
    /// A list the operator curated.
    Curated,
    /// Every discovered page, no sampling.
    None,
}

impl Sampling {
    /// Whether pages are the operator's choice rather than the tool's.
    pub fn is_selected(self) -> bool {
        !matches!(self, Self::Auto)
    }
}

/// The numeric limits the checks compare against.
#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", default)]
pub struct Thresholds {
    /// `page-size-markdown` warn band.
    pub markdown_warn_chars: usize,
    /// `page-size-markdown` failure.
    pub markdown_error_chars: usize,
    /// `page-size-html`, measured with `<script>` and `<style>` stripped.
    pub html_bytes: u64,
    /// `page-size-transfer`, the served body after decompression.
    pub transfer_bytes: u64,
    /// `page-size-transfer` hard ceiling.
    pub transfer_error_bytes: u64,
    /// `llms-txt-size`.
    pub llms_txt_chars: usize,
    /// `llms-txt-coverage`: the share of indexable pages that must be listed.
    pub coverage: f64,
    /// `content-start-position`: content must begin in this share of the
    /// converted output.
    pub content_start: f64,
    /// Above this share of failed page fetches, the run is a partial sample.
    pub fetch_failure_rate: f64,
}

impl Default for Thresholds {
    fn default() -> Self {
        Self {
            markdown_warn_chars: size::WARN_CHARS,
            markdown_error_chars: size::ERROR_CHARS,
            html_bytes: 1024 * 1024,
            transfer_bytes: 1024 * 1024,
            transfer_error_bytes: 10 * 1024 * 1024,
            llms_txt_chars: crate::agents::llms::INDEX_MAX_CHARS,
            coverage: 1.0,
            content_start: 0.10,
            fetch_failure_rate: 0.20,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", default)]
pub struct Options {
    /// How many `llms.txt` links a run fetches before it stops.
    pub max_links_to_test: usize,
    pub sampling_strategy: Sampling,
    pub thresholds: Thresholds,
    /// Route patterns `llms-txt-coverage` does not count against the site.
    pub coverage_exclusions: Vec<String>,
    /// Route patterns `markdown-content-parity` does not compare.
    pub parity_exclusions: Vec<String>,
    /// Explicit URLs to score as given, instead of sampling the site. No
    /// command sets this yet: `liyasa test --agents` runs with the defaults.
    pub urls: Vec<String>,
}

impl Default for Options {
    fn default() -> Self {
        Self {
            max_links_to_test: 100,
            sampling_strategy: Sampling::default(),
            thresholds: Thresholds::default(),
            coverage_exclusions: Vec::new(),
            parity_exclusions: Vec::new(),
            urls: Vec::new(),
        }
    }
}

impl Options {
    /// Whether the pages under test were chosen rather than sampled.
    pub fn pages_selected(&self) -> bool {
        self.sampling_strategy.is_selected() || !self.urls.is_empty()
    }

    pub fn excluded_from_coverage(&self, route: &str) -> bool {
        matches_any(&self.coverage_exclusions, route)
    }

    pub fn excluded_from_parity(&self, route: &str) -> bool {
        matches_any(&self.parity_exclusions, route)
    }
}

fn matches_any(patterns: &[String], route: &str) -> bool {
    patterns.iter().any(|pattern| glob(pattern, route))
}

/// `*` matches any run of characters, including `/`. Anything else is literal.
///
/// The reference tool takes the same shape of pattern; a fuller glob would be
/// a behaviour difference, which is the one thing these options exist to avoid.
pub fn glob(pattern: &str, text: &str) -> bool {
    let mut parts = pattern.split('*');
    let Some(first) = parts.next() else {
        return pattern == text;
    };
    if !text.starts_with(first) {
        return false;
    }
    let mut rest = &text[first.len()..];
    let mut last: Option<&str> = None;
    for part in parts {
        last = Some(part);
        if part.is_empty() {
            continue;
        }
        match rest.find(part) {
            Some(at) => rest = &rest[at + part.len()..],
            None => return false,
        }
    }
    match last {
        // The pattern had no `*` at all: it must have matched the whole text.
        None => rest.is_empty(),
        // It ended with `*`, so whatever is left is matched.
        Some("") => true,
        // It ended with a literal, which must be the end of the text.
        Some(part) => text.ends_with(part),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn spec_01_the_defaults_are_the_thresholds_the_prd_names() {
        let options = Options::default();
        assert_eq!(options.thresholds.markdown_warn_chars, 50_000);
        assert_eq!(options.thresholds.markdown_error_chars, 100_000);
        assert_eq!(options.thresholds.llms_txt_chars, 50_000);
        assert_eq!(options.thresholds.html_bytes, 1024 * 1024);
        assert_eq!(options.thresholds.transfer_error_bytes, 10 * 1024 * 1024);
        assert_eq!(options.thresholds.content_start, 0.10);
        assert_eq!(options.thresholds.fetch_failure_rate, 0.20);
    }

    #[test]
    fn spec_01_the_option_names_are_the_reference_tools() {
        let json = serde_json::to_value(Options::default()).expect("serializes");
        for key in [
            "maxLinksToTest",
            "samplingStrategy",
            "thresholds",
            "coverageExclusions",
            "parityExclusions",
        ] {
            assert!(json.get(key).is_some(), "{key}");
        }
        let thresholds = &json["thresholds"];
        for key in ["markdownWarnChars", "llmsTxtChars", "fetchFailureRate"] {
            assert!(thresholds.get(key).is_some(), "{key}");
        }
    }

    #[test]
    fn spec_01_options_round_trip_through_json() {
        let json = r#"{
            "maxLinksToTest": 25,
            "samplingStrategy": "curated",
            "coverageExclusions": ["/internal/*"],
            "parityExclusions": ["/api/*"],
            "thresholds": { "coverage": 0.95 }
        }"#;
        let options: Options = serde_json::from_str(json).expect("parses");
        assert_eq!(options.max_links_to_test, 25);
        assert_eq!(options.sampling_strategy, Sampling::Curated);
        assert_eq!(options.thresholds.coverage, 0.95);
        // An unnamed threshold keeps its default.
        assert_eq!(options.thresholds.markdown_warn_chars, 50_000);
        assert!(options.excluded_from_coverage("/internal/runbook"));
        assert!(!options.excluded_from_coverage("/guide/install"));
        assert!(options.excluded_from_parity("/api/pets"));
    }

    #[test]
    fn spec_04_explicit_urls_count_as_a_selection() {
        let mut options = Options::default();
        assert!(!options.pages_selected());
        options.urls = vec!["https://example.com/guide".to_owned()];
        assert!(options.pages_selected());

        let curated = Options {
            sampling_strategy: Sampling::Curated,
            ..Options::default()
        };
        assert!(curated.pages_selected());
    }

    #[test]
    fn spec_01_exclusion_patterns_match_the_way_the_reference_tool_does() {
        assert!(glob("/internal/*", "/internal/runbook"));
        assert!(glob("/internal/*", "/internal/a/b/c"));
        assert!(!glob("/internal/*", "/guide/internal"));
        assert!(glob("*.md", "/guide/install.md"));
        assert!(!glob("*.md", "/guide/install.html"));
        assert!(glob("/guide/install", "/guide/install"));
        assert!(!glob("/guide/install", "/guide/install/extra"));
        assert!(glob("/a/*/c", "/a/b/c"));
        assert!(!glob("/a/*/c", "/a/b/d"));
    }
}
