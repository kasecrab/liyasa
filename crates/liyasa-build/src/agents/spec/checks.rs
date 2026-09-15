//! The 28 checks of spec v0.6.0, with the spec's severities and the reference
//! tool's weights (PRD §25, SPEC-01, SPEC-03).
//!
//! Two numbers are attached to every check and this table keeps them apart.
//! The **severity** is the spec's own field and has two values. The **weight**
//! is `afdocs`'s scoring input, and the two disagree on ten of the twenty-three
//! checks `afdocs` scores; five checks have no published weight at all.

use super::SPEC_VERSION;

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub enum Category {
    ContentDiscoverability,
    MarkdownAvailability,
    PageSize,
    ContentStructure,
    UrlStability,
    Observability,
    Authentication,
}

impl Category {
    pub fn title(self) -> &'static str {
        match self {
            Self::ContentDiscoverability => "Content Discoverability",
            Self::MarkdownAvailability => "Markdown Availability",
            Self::PageSize => "Page Size",
            Self::ContentStructure => "Content Structure",
            Self::UrlStability => "URL Stability",
            Self::Observability => "Observability",
            Self::Authentication => "Authentication",
        }
    }

    /// Checks run in category order, which is what makes the dependency rules
    /// evaluable in one pass.
    pub const ORDER: [Self; 7] = [
        Self::ContentDiscoverability,
        Self::MarkdownAvailability,
        Self::PageSize,
        Self::ContentStructure,
        Self::UrlStability,
        Self::Observability,
        Self::Authentication,
    ];
}

/// The spec's own severity field. v0.6.0 defines exactly these two: there is no
/// `Critical` and no `Low` severity, whatever the weight tiers are called.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Severity {
    High,
    Medium,
}

impl Severity {
    /// The weight a check is given when `afdocs` has published none.
    pub fn implied_weight(self) -> Weight {
        match self {
            Self::High => Weight::High,
            Self::Medium => Weight::Medium,
        }
    }
}

/// `afdocs`'s point tiers.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Weight {
    Critical,
    High,
    Medium,
    Low,
}

impl Weight {
    pub fn points(self) -> f64 {
        match self {
            Self::Critical => 10.0,
            Self::High => 7.0,
            Self::Medium => 4.0,
            Self::Low => 2.0,
        }
    }
}

/// Whether a check measures one resource or a sample of pages. A single
/// resource is all or nothing; a sample scores proportionally.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Scope {
    SingleResource,
    MultiPage,
}

/// What has to hold before a check is worth running.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Requires {
    Nothing,
    /// Every named check must have passed.
    All(&'static [&'static str]),
    /// At least one named check must have passed.
    Any(&'static [&'static str]),
    /// `tabbed-content-serialization` found tabbed content to judge.
    TabbedContent,
    /// `auth-gate-detection` warned or failed.
    AuthGateNotPassing,
}

/// One row of the spec's check table.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Check {
    pub id: &'static str,
    pub category: Category,
    pub severity: Severity,
    /// `None` when `afdocs` has not published a weight; the score then comes
    /// from the severity and is reported separately (SPEC-04).
    pub weight: Option<Weight>,
    /// What a warn earns, as a share of the weight. `None` is strictly pass or
    /// fail.
    pub warn: Option<f64>,
    pub scope: Scope,
    pub requires: Requires,
    /// Scaled by the discovery coefficient: the check measures the quality of a
    /// path agents must first find.
    pub cluster: bool,
}

impl Check {
    /// The weight the comparable score uses, or `None` for a check `afdocs`
    /// does not score.
    pub fn comparable_weight(&self) -> Option<f64> {
        self.weight.map(Weight::points)
    }

    /// The weight the full score uses: the published one, or the one the spec
    /// severity implies.
    pub fn full_weight(&self) -> f64 {
        self.weight
            .unwrap_or_else(|| self.severity.implied_weight())
            .points()
    }

    pub fn is_comparable(&self) -> bool {
        self.weight.is_some()
    }
}

/// What a warn earns for a check §25.1 does not tier: the "genuine functional
/// degradation" coefficient, which is the tier every unlisted check's failure
/// mode resembles (`plan/rfcs/1005-spec-scoring-gaps.md`).
pub const DEFAULT_WARN: f64 = 0.50;

const NEEDS_LLMS_TXT: &[&str] = &["llms-txt-exists"];
const NEEDS_MARKDOWN_PATH: &[&str] = &["markdown-url-support", "content-negotiation"];
const NEEDS_ANY_MARKDOWN_PATH: &[&str] = &[
    "markdown-url-support",
    "content-negotiation",
    "llms-txt-links-markdown",
];

/// The three checks the discovery coefficient scales: each measures the quality
/// of a path an agent must find before the quality matters.
pub const DISCOVERY_CLUSTER: [&str; 3] = [
    "page-size-markdown",
    "markdown-code-fence-validity",
    "markdown-content-parity",
];

pub const CHECKS: [Check; 28] = [
    // ---- Content Discoverability ----
    Check {
        id: "llms-txt-exists",
        category: Category::ContentDiscoverability,
        severity: Severity::High,
        weight: Some(Weight::Critical),
        warn: Some(0.50),
        scope: Scope::SingleResource,
        requires: Requires::Nothing,
        cluster: false,
    },
    Check {
        id: "llms-txt-valid",
        category: Category::ContentDiscoverability,
        severity: Severity::Medium,
        weight: Some(Weight::Medium),
        warn: Some(0.75),
        scope: Scope::SingleResource,
        requires: Requires::All(NEEDS_LLMS_TXT),
        cluster: false,
    },
    Check {
        id: "llms-txt-size",
        category: Category::ContentDiscoverability,
        severity: Severity::High,
        weight: Some(Weight::High),
        warn: Some(0.50),
        scope: Scope::SingleResource,
        requires: Requires::All(NEEDS_LLMS_TXT),
        cluster: false,
    },
    Check {
        id: "llms-txt-links-resolve",
        category: Category::ContentDiscoverability,
        severity: Severity::High,
        weight: Some(Weight::High),
        warn: Some(0.75),
        scope: Scope::SingleResource,
        requires: Requires::All(NEEDS_LLMS_TXT),
        cluster: false,
    },
    Check {
        id: "llms-txt-links-markdown",
        category: Category::ContentDiscoverability,
        severity: Severity::Medium,
        weight: Some(Weight::High),
        warn: Some(0.25),
        scope: Scope::SingleResource,
        requires: Requires::All(NEEDS_LLMS_TXT),
        cluster: false,
    },
    Check {
        id: "llms-txt-directive-html",
        category: Category::ContentDiscoverability,
        severity: Severity::High,
        weight: Some(Weight::High),
        warn: Some(0.60),
        scope: Scope::MultiPage,
        requires: Requires::Nothing,
        cluster: false,
    },
    Check {
        id: "llms-txt-directive-md",
        category: Category::ContentDiscoverability,
        severity: Severity::Medium,
        weight: Some(Weight::Medium),
        warn: Some(0.60),
        scope: Scope::MultiPage,
        requires: Requires::Any(NEEDS_MARKDOWN_PATH),
        cluster: false,
    },
    // ---- Markdown Availability ----
    Check {
        id: "markdown-url-support",
        category: Category::MarkdownAvailability,
        severity: Severity::High,
        weight: Some(Weight::High),
        warn: Some(0.50),
        scope: Scope::MultiPage,
        requires: Requires::Nothing,
        cluster: false,
    },
    Check {
        id: "content-negotiation",
        category: Category::MarkdownAvailability,
        severity: Severity::Medium,
        weight: Some(Weight::Medium),
        warn: Some(0.75),
        scope: Scope::MultiPage,
        requires: Requires::Nothing,
        cluster: false,
    },
    // ---- Page Size ----
    Check {
        id: "rendering-strategy",
        category: Category::PageSize,
        severity: Severity::High,
        weight: Some(Weight::Critical),
        warn: Some(0.50),
        scope: Scope::MultiPage,
        requires: Requires::Nothing,
        cluster: false,
    },
    Check {
        id: "page-size-markdown",
        category: Category::PageSize,
        severity: Severity::High,
        weight: Some(Weight::High),
        warn: Some(0.50),
        scope: Scope::MultiPage,
        requires: Requires::Any(NEEDS_MARKDOWN_PATH),
        cluster: true,
    },
    Check {
        id: "page-size-html",
        category: Category::PageSize,
        severity: Severity::High,
        weight: Some(Weight::High),
        warn: Some(0.50),
        scope: Scope::MultiPage,
        requires: Requires::Nothing,
        cluster: false,
    },
    Check {
        id: "page-size-transfer",
        category: Category::PageSize,
        severity: Severity::Medium,
        weight: None,
        warn: Some(DEFAULT_WARN),
        scope: Scope::MultiPage,
        requires: Requires::Nothing,
        cluster: false,
    },
    Check {
        id: "content-start-position",
        category: Category::PageSize,
        severity: Severity::High,
        weight: Some(Weight::Medium),
        warn: Some(0.50),
        scope: Scope::MultiPage,
        requires: Requires::Nothing,
        cluster: false,
    },
    Check {
        id: "single-fetch-completeness",
        category: Category::PageSize,
        severity: Severity::Medium,
        weight: None,
        warn: Some(DEFAULT_WARN),
        scope: Scope::MultiPage,
        requires: Requires::Any(NEEDS_ANY_MARKDOWN_PATH),
        cluster: false,
    },
    // ---- Content Structure ----
    Check {
        id: "tabbed-content-serialization",
        category: Category::ContentStructure,
        severity: Severity::High,
        weight: Some(Weight::Medium),
        warn: Some(0.50),
        scope: Scope::MultiPage,
        requires: Requires::Nothing,
        cluster: false,
    },
    Check {
        id: "section-header-quality",
        category: Category::ContentStructure,
        severity: Severity::Medium,
        weight: Some(Weight::Low),
        warn: Some(0.50),
        scope: Scope::MultiPage,
        requires: Requires::TabbedContent,
        cluster: false,
    },
    Check {
        id: "markdown-code-fence-validity",
        category: Category::ContentStructure,
        severity: Severity::Medium,
        weight: Some(Weight::Medium),
        // §25.1: strictly pass or fail.
        warn: None,
        scope: Scope::MultiPage,
        requires: Requires::Any(NEEDS_MARKDOWN_PATH),
        cluster: true,
    },
    Check {
        id: "markdown-link-portability",
        category: Category::ContentStructure,
        severity: Severity::Medium,
        weight: None,
        warn: Some(DEFAULT_WARN),
        scope: Scope::MultiPage,
        requires: Requires::Any(NEEDS_ANY_MARKDOWN_PATH),
        cluster: false,
    },
    Check {
        id: "embedded-data-serialization",
        category: Category::ContentStructure,
        severity: Severity::Medium,
        weight: None,
        warn: Some(DEFAULT_WARN),
        scope: Scope::MultiPage,
        requires: Requires::Nothing,
        cluster: false,
    },
    // ---- URL Stability ----
    Check {
        id: "http-status-codes",
        category: Category::UrlStability,
        severity: Severity::Medium,
        weight: Some(Weight::High),
        // §25.1: pass or fail, except that an all-indeterminate sample warns.
        warn: Some(DEFAULT_WARN),
        scope: Scope::MultiPage,
        requires: Requires::Nothing,
        cluster: false,
    },
    Check {
        id: "redirect-behavior",
        category: Category::UrlStability,
        severity: Severity::Medium,
        weight: Some(Weight::Medium),
        warn: Some(0.60),
        scope: Scope::MultiPage,
        requires: Requires::Nothing,
        cluster: false,
    },
    // ---- Observability ----
    Check {
        id: "llms-txt-coverage",
        category: Category::Observability,
        severity: Severity::High,
        weight: Some(Weight::Medium),
        warn: Some(0.75),
        scope: Scope::SingleResource,
        requires: Requires::All(NEEDS_LLMS_TXT),
        cluster: false,
    },
    Check {
        id: "markdown-content-parity",
        category: Category::Observability,
        severity: Severity::Medium,
        weight: Some(Weight::Medium),
        warn: Some(0.75),
        scope: Scope::MultiPage,
        requires: Requires::Any(NEEDS_MARKDOWN_PATH),
        cluster: true,
    },
    Check {
        id: "cache-header-hygiene",
        category: Category::Observability,
        severity: Severity::Medium,
        weight: Some(Weight::Low),
        warn: Some(0.50),
        scope: Scope::MultiPage,
        requires: Requires::Nothing,
        cluster: false,
    },
    // ---- Authentication ----
    Check {
        id: "auth-gate-detection",
        category: Category::Authentication,
        severity: Severity::High,
        weight: Some(Weight::Critical),
        warn: Some(0.50),
        scope: Scope::MultiPage,
        requires: Requires::Nothing,
        cluster: false,
    },
    Check {
        id: "auth-alternative-access",
        category: Category::Authentication,
        severity: Severity::Medium,
        weight: Some(Weight::Medium),
        warn: Some(0.50),
        scope: Scope::MultiPage,
        requires: Requires::AuthGateNotPassing,
        cluster: false,
    },
    Check {
        id: "bot-protection-interference",
        category: Category::Authentication,
        severity: Severity::High,
        // `afdocs` does not score it at all yet.
        weight: None,
        warn: Some(DEFAULT_WARN),
        scope: Scope::MultiPage,
        // It inverts the dependency direction: no prerequisite, and its own
        // result flags every multi-page result instead.
        requires: Requires::Nothing,
        cluster: false,
    },
];

pub fn get(id: &str) -> Option<&'static Check> {
    CHECKS.iter().find(|check| check.id == id)
}

/// Every check ID, in category order, for the release job that compares the
/// implemented set with the tracked spec version (SPEC-03).
pub fn ids() -> Vec<&'static str> {
    let mut out: Vec<&'static Check> = CHECKS.iter().collect();
    out.sort_by_key(|check| check.category);
    out.into_iter().map(|check| check.id).collect()
}

pub fn in_category(category: Category) -> impl Iterator<Item = &'static Check> {
    CHECKS
        .iter()
        .filter(move |check| check.category == category)
}

/// The version this table implements.
pub fn version() -> &'static str {
    SPEC_VERSION
}

/// What the release's `spec-compliance` job compares: the check set this build
/// implements against the set the tracked spec version publishes (SPEC-03).
///
/// `published` comes from the spec's own check-summary page, read by the job;
/// this function is the comparison, not the fetch.
pub fn compare(published: &[&str]) -> Option<liyasa_core::Diagnostic> {
    let ours: std::collections::BTreeSet<&str> = CHECKS.iter().map(|check| check.id).collect();
    let theirs: std::collections::BTreeSet<&str> = published.iter().copied().collect();
    let missing: Vec<&str> = theirs.difference(&ours).copied().collect();
    let extra: Vec<&str> = ours.difference(&theirs).copied().collect();
    if missing.is_empty() && extra.is_empty() {
        return None;
    }
    let mut parts = Vec::new();
    if !missing.is_empty() {
        parts.push(format!("not implemented: {}", missing.join(", ")));
    }
    if !extra.is_empty() {
        parts.push(format!("no longer published: {}", extra.join(", ")));
    }
    Some(
        liyasa_core::Diagnostic::new(
            liyasa_core::diagnostics::code::W0410,
            format!(
                "the implemented check set differs from spec v{SPEC_VERSION}: {}",
                parts.join("; ")
            ),
        )
        .help("add the new checks and bump `agents.specVersion`, or pin the tracked version"),
    )
}

#[cfg(test)]
mod tests {
    use std::collections::BTreeSet;

    use super::*;

    #[test]
    fn spec_01_the_table_holds_all_twenty_eight_checks() {
        assert_eq!(CHECKS.len(), 28);
        let ids: BTreeSet<&str> = CHECKS.iter().map(|check| check.id).collect();
        assert_eq!(ids.len(), 28, "a check ID is repeated");
    }

    #[test]
    fn spec_01_the_categories_hold_the_counts_the_spec_publishes() {
        for (category, count) in [
            (Category::ContentDiscoverability, 7),
            (Category::MarkdownAvailability, 2),
            (Category::PageSize, 6),
            (Category::ContentStructure, 5),
            (Category::UrlStability, 2),
            (Category::Observability, 3),
            (Category::Authentication, 3),
        ] {
            assert_eq!(in_category(category).count(), count, "{}", category.title());
        }
    }

    #[test]
    fn spec_01_twenty_three_checks_carry_a_published_weight() {
        assert_eq!(CHECKS.iter().filter(|c| c.is_comparable()).count(), 23);
        let unpublished: Vec<&str> = CHECKS
            .iter()
            .filter(|c| !c.is_comparable())
            .map(|c| c.id)
            .collect();
        assert_eq!(
            unpublished,
            [
                "page-size-transfer",
                "single-fetch-completeness",
                "markdown-link-portability",
                "embedded-data-serialization",
                "bot-protection-interference",
            ]
        );
    }

    #[test]
    fn spec_01_severity_and_weight_disagree_on_exactly_ten_scored_checks() {
        let disagreeing: Vec<&str> = CHECKS
            .iter()
            .filter(|check| {
                check
                    .weight
                    .is_some_and(|weight| weight != check.severity.implied_weight())
            })
            .map(|check| check.id)
            .collect();
        assert_eq!(
            disagreeing,
            [
                "llms-txt-exists",
                "llms-txt-links-markdown",
                "rendering-strategy",
                "content-start-position",
                "tabbed-content-serialization",
                "section-header-quality",
                "http-status-codes",
                "llms-txt-coverage",
                "cache-header-hygiene",
                "auth-gate-detection",
            ]
        );
    }

    #[test]
    fn spec_01_the_warn_tiers_match_the_scoring_table() {
        for (coefficient, ids) in [
            (
                0.75,
                &[
                    "llms-txt-valid",
                    "content-negotiation",
                    "llms-txt-links-resolve",
                    "llms-txt-coverage",
                    "markdown-content-parity",
                ][..],
            ),
            (
                0.60,
                &[
                    "llms-txt-directive-html",
                    "llms-txt-directive-md",
                    "redirect-behavior",
                ][..],
            ),
            (0.25, &["llms-txt-links-markdown"][..]),
        ] {
            for id in ids {
                let check = get(id).unwrap_or_else(|| panic!("{id}"));
                assert_eq!(check.warn, Some(coefficient), "{id}");
            }
        }
        assert_eq!(
            get("markdown-code-fence-validity").map(|c| c.warn),
            Some(None)
        );
    }

    #[test]
    fn spec_01_the_discovery_cluster_is_the_three_the_spec_names() {
        let cluster: Vec<&str> = CHECKS
            .iter()
            .filter(|check| check.cluster)
            .map(|check| check.id)
            .collect();
        assert_eq!(cluster, DISCOVERY_CLUSTER);
        assert!(
            !get("markdown-url-support")
                .expect("markdown-url-support")
                .cluster,
            "markdown-url-support is deliberately not scaled"
        );
    }

    #[test]
    fn spec_01_every_prerequisite_names_a_check_that_exists() {
        for check in &CHECKS {
            let named: &[&str] = match check.requires {
                Requires::All(ids) | Requires::Any(ids) => ids,
                _ => &[],
            };
            for id in named {
                assert!(get(id).is_some(), "{} requires {id}", check.id);
            }
        }
    }

    #[test]
    fn spec_01_one_prerequisite_points_forward_across_categories() {
        // §25.1 runs checks in category order and makes `llms-txt-directive-md`
        // (Content Discoverability) depend on `markdown-url-support` and
        // `content-negotiation` (Markdown Availability), which run after it. A
        // runner therefore resolves prerequisites over the finished set rather
        // than as it goes; this test pins the one place that matters so the
        // next reader does not take the ordering for a bug.
        let position = |id: &str| {
            ids()
                .iter()
                .position(|listed| *listed == id)
                .unwrap_or_else(|| panic!("{id}"))
        };
        let mut forward = Vec::new();
        for check in &CHECKS {
            let named: &[&str] = match check.requires {
                Requires::All(ids) | Requires::Any(ids) => ids,
                _ => &[],
            };
            for id in named {
                if position(id) > position(check.id) {
                    forward.push(check.id);
                    break;
                }
            }
        }
        assert_eq!(forward, ["llms-txt-directive-md"]);
    }

    #[test]
    fn spec_03_the_table_names_the_version_it_implements() {
        assert_eq!(version(), "0.6.0");
        assert_eq!(ids().len(), 28);
    }

    #[test]
    fn spec_03_a_matching_check_set_is_silent() {
        assert!(compare(&ids()).is_none());
    }

    #[test]
    fn spec_03_a_check_the_spec_added_is_reported() {
        let mut published = ids();
        published.push("structured-data-presence");
        let diagnostic = compare(&published).expect("a diagnostic");
        assert_eq!(diagnostic.code.as_str(), "W0410");
        assert!(
            diagnostic
                .message
                .contains("not implemented: structured-data-presence"),
            "{}",
            diagnostic.message
        );
    }

    #[test]
    fn spec_03_a_check_the_spec_dropped_is_reported() {
        let published: Vec<&str> = ids()
            .into_iter()
            .filter(|id| *id != "cache-header-hygiene")
            .collect();
        let diagnostic = compare(&published).expect("a diagnostic");
        assert!(
            diagnostic
                .message
                .contains("no longer published: cache-header-hygiene"),
            "{}",
            diagnostic.message
        );
    }
}
