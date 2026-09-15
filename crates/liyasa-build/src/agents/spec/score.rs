//! The reference scoring model (PRD §25.1, SPEC-04).
//!
//! `score = (sum of check scores) / (sum of weights for non-skipped checks) ×
//! 100`, with four rules that override the arithmetic: score caps, the
//! insufficient-data rule, the discovery cluster coefficient, and the
//! dependency skips. The gaps §25.1 leaves are filled by
//! `plan/rfcs/1005-spec-scoring-gaps.md`.

use std::collections::BTreeMap;

use super::checks::{self, Category, Check, Requires, Scope};

/// How a check came out.
#[derive(Debug, Clone, PartialEq)]
pub enum Outcome {
    Pass,
    Warn,
    Fail,
    /// A multi-page check: how many of the sampled pages satisfied it. Scores
    /// proportionally, so three oversized pages out of fifty are not a zero.
    Partial {
        passed: usize,
        total: usize,
    },
    /// A prerequisite failed. Excluded from both halves of the fraction.
    Skipped {
        reason: String,
    },
    /// Nothing to measure, or too few pages to measure it on.
    NotApplicable {
        reason: String,
    },
    /// Every sampled response was indeterminate; §25.1 makes this a warn.
    Indeterminate,
}

impl Outcome {
    pub fn is_pass(&self) -> bool {
        matches!(self, Self::Pass)
            || matches!(self, Self::Partial { passed, total } if passed == total)
    }

    pub fn is_fail(&self) -> bool {
        matches!(self, Self::Fail)
            || matches!(self, Self::Partial { passed: 0, total } if *total > 0)
    }

    pub fn counts(&self) -> bool {
        !matches!(self, Self::Skipped { .. } | Self::NotApplicable { .. })
    }

    /// The share of the weight this outcome earns, before the cluster
    /// coefficient.
    fn share(&self, check: &Check) -> f64 {
        let warn = check.warn.unwrap_or(0.0);
        match self {
            Self::Pass => 1.0,
            Self::Fail => 0.0,
            Self::Warn | Self::Indeterminate => warn,
            Self::Partial { passed, total } => {
                if *total == 0 {
                    0.0
                } else if check.scope == Scope::SingleResource {
                    // All or nothing: a single resource either satisfies the
                    // check or does not.
                    f64::from(passed == total)
                } else {
                    *passed as f64 / *total as f64
                }
            }
            Self::Skipped { .. } | Self::NotApplicable { .. } => 0.0,
        }
    }
}

/// One check's result, as the runner produced it.
#[derive(Debug, Clone, PartialEq)]
pub struct CheckResult {
    pub id: &'static str,
    pub outcome: Outcome,
    pub detail: String,
    /// `rendering-strategy` failed, so this result was computed against markup
    /// that is not what a reader sees. Reported, never silently dropped.
    pub unreliable: bool,
}

impl CheckResult {
    pub fn new(id: &'static str, outcome: Outcome, detail: impl Into<String>) -> Self {
        Self {
            id,
            outcome,
            detail: detail.into(),
            unreliable: false,
        }
    }

    pub fn check(&self) -> Option<&'static Check> {
        checks::get(self.id)
    }
}

/// What the run saw, for the rules that look past a single check.
#[derive(Debug, Clone, PartialEq)]
pub struct RunFacts {
    pub pages_discovered: usize,
    /// Pages were chosen by the operator (`--urls`, a curated list, or no
    /// sampling), so the insufficient-data rule does not apply.
    pub pages_selected: bool,
    /// `(serverRendered + sparseContent × 0.5) / total`.
    pub rendering_proportion: f64,
    /// Share of discovered pages behind an authentication gate.
    pub gated_proportion: f64,
    /// No route returned content an agent can read.
    pub no_viable_path: bool,
    /// Share of page fetches that failed.
    pub fetch_failure_rate: f64,
    /// `llms.txt` entries pointing outside `seo.canonicalOrigin`, which an
    /// agent cannot treat as this site's content (SPEC-05).
    pub cross_origin_llms_links: usize,
}

impl Default for RunFacts {
    fn default() -> Self {
        Self {
            pages_discovered: 0,
            pages_selected: false,
            rendering_proportion: 1.0,
            gated_proportion: 0.0,
            no_viable_path: false,
            fetch_failure_rate: 0.0,
            cross_origin_llms_links: 0,
        }
    }
}

/// Under this many discovered pages, automatic sampling is not a sample.
pub const MIN_PAGES: usize = 5;

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, PartialOrd, Ord)]
pub enum Grade {
    A,
    B,
    C,
    D,
    #[default]
    F,
}

impl Grade {
    /// A ≥ 90, B ≥ 80, C ≥ 70, D ≥ 60, F below
    /// (`plan/rfcs/1005-spec-scoring-gaps.md`).
    pub fn of(score: u32) -> Self {
        match score {
            90..=u32::MAX => Self::A,
            80..=89 => Self::B,
            70..=79 => Self::C,
            60..=69 => Self::D,
            _ => Self::F,
        }
    }

    pub fn letter(self) -> char {
        match self {
            Self::A => 'A',
            Self::B => 'B',
            Self::C => 'C',
            Self::D => 'D',
            Self::F => 'F',
        }
    }
}

/// A rule that held the score down, and the ceiling it imposed.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Cap {
    pub ceiling: u32,
    pub reason: String,
}

#[derive(Debug, Clone, Default, PartialEq)]
pub struct Score {
    /// Over the 23 checks `afdocs` scores, which is the number to compare.
    pub comparable: u32,
    /// Over all 28, with the unpublished five weighted from spec severity.
    pub full: u32,
    pub grade: Grade,
    pub categories: BTreeMap<&'static str, u32>,
    /// Every cap that applied; the lowest ceiling is the one in effect.
    pub caps: Vec<Cap>,
    /// 1.0, 0.8, 0.5, or 0.0 — what the three Markdown-quality checks were
    /// scaled by.
    pub discovery_coefficient: f64,
}

impl Score {
    pub fn ceiling(&self) -> Option<u32> {
        self.caps.iter().map(|cap| cap.ceiling).min()
    }
}

/// Applies the dependency and insufficient-data rules to a run's results,
/// returning them with skips and not-applicables filled in.
pub fn resolve(results: &[CheckResult], facts: &RunFacts) -> Vec<CheckResult> {
    let passed: BTreeMap<&str, bool> = results
        .iter()
        .map(|result| (result.id, result.outcome.is_pass()))
        .collect();
    let rendering_failed = results
        .iter()
        .any(|r| r.id == "rendering-strategy" && r.outcome.is_fail());
    let auth_not_passing = results
        .iter()
        .any(|r| r.id == "auth-gate-detection" && !r.outcome.is_pass());
    let tabbed = results.iter().any(|r| {
        r.id == "tabbed-content-serialization"
            && !matches!(r.outcome, Outcome::NotApplicable { .. })
    });
    let thin = !facts.pages_selected && facts.pages_discovered < MIN_PAGES;

    let mut out = Vec::with_capacity(results.len());
    for result in results {
        let Some(check) = result.check() else {
            continue;
        };
        let mut result = result.clone();

        if let Some(reason) = unmet(check, &passed, auth_not_passing, tabbed) {
            result.outcome = Outcome::Skipped { reason };
            out.push(result);
            continue;
        }
        if thin && check.scope == Scope::MultiPage {
            result.outcome = Outcome::NotApplicable {
                reason: format!(
                    "only {} page(s) discovered; page-level checks need {MIN_PAGES}",
                    facts.pages_discovered
                ),
            };
            out.push(result);
            continue;
        }
        // §25.1: these two are flagged, not skipped, when the markup they read
        // is not what a reader sees.
        if rendering_failed && matches!(check.id, "page-size-html" | "content-start-position") {
            result.unreliable = true;
        }
        out.push(result);
    }
    out
}

fn unmet(
    check: &Check,
    passed: &BTreeMap<&str, bool>,
    auth_not_passing: bool,
    tabbed: bool,
) -> Option<String> {
    let held = |id: &str| passed.get(id).copied().unwrap_or(false);
    match check.requires {
        Requires::Nothing => None,
        Requires::All(ids) => ids
            .iter()
            .find(|id| !held(id))
            .map(|id| format!("`{id}` did not pass")),
        Requires::Any(ids) => (!ids.iter().any(|id| held(id))).then(|| {
            format!(
                "none of {} passed",
                ids.iter()
                    .map(|id| format!("`{id}`"))
                    .collect::<Vec<_>>()
                    .join(", ")
            )
        }),
        Requires::TabbedContent => (!tabbed).then(|| "no tabbed content to judge".to_owned()),
        Requires::AuthGateNotPassing => {
            (!auth_not_passing).then(|| "`auth-gate-detection` passed".to_owned())
        }
    }
}

/// 1.0 when content negotiation passes, 0.8 when either directive check
/// passes, 0.5 when `llms.txt` links are `.md` URLs, 0.0 when none holds.
pub fn discovery_coefficient(results: &[CheckResult]) -> f64 {
    let passed = |id: &str| {
        results
            .iter()
            .any(|result| result.id == id && result.outcome.is_pass())
    };
    if passed("content-negotiation") {
        1.0
    } else if passed("llms-txt-directive-html") || passed("llms-txt-directive-md") {
        0.8
    } else if passed("llms-txt-links-markdown") {
        0.5
    } else {
        0.0
    }
}

/// Scores a resolved result set.
pub fn score(results: &[CheckResult], facts: &RunFacts) -> Score {
    let coefficient = discovery_coefficient(results);
    let mut comparable = Fraction::default();
    let mut full = Fraction::default();
    let mut categories: BTreeMap<Category, Fraction> = BTreeMap::new();

    for result in results {
        let Some(check) = result.check() else {
            continue;
        };
        if !result.outcome.counts() {
            continue;
        }
        let scale = if check.cluster { coefficient } else { 1.0 };
        if scale == 0.0 {
            // The cluster is excluded entirely when no discovery path holds.
            continue;
        }
        let share = result.outcome.share(check);
        let weight = check.full_weight() * scale;
        full.add(share * weight, weight);
        categories
            .entry(check.category)
            .or_default()
            .add(share * weight, weight);
        if let Some(points) = check.comparable_weight() {
            let weight = points * scale;
            comparable.add(share * weight, weight);
        }
    }

    let caps = caps(results, facts);
    let ceiling = caps.iter().map(|cap| cap.ceiling).min();
    let comparable = apply(comparable.percent(), ceiling);
    let full = apply(full.percent(), ceiling);

    Score {
        comparable,
        full,
        grade: Grade::of(comparable),
        categories: categories
            .into_iter()
            .map(|(category, fraction)| (category.title(), apply(fraction.percent(), ceiling)))
            .collect(),
        caps,
        discovery_coefficient: coefficient,
    }
}

fn apply(score: u32, ceiling: Option<u32>) -> u32 {
    match ceiling {
        Some(ceiling) => score.min(ceiling),
        None => score,
    }
}

/// The four score caps of §25.1. The lowest applicable one wins.
fn caps(results: &[CheckResult], facts: &RunFacts) -> Vec<Cap> {
    let mut out = Vec::new();
    if results
        .iter()
        .any(|r| r.id == "llms-txt-exists" && r.outcome.is_fail())
    {
        out.push(Cap {
            ceiling: 59,
            reason: "`llms-txt-exists` failed".to_owned(),
        });
    }
    if facts.no_viable_path {
        out.push(Cap {
            ceiling: 39,
            reason: "no viable content path: nothing served content an agent can read".to_owned(),
        });
    }
    if !facts.pages_selected && facts.pages_discovered < MIN_PAGES {
        out.push(Cap {
            ceiling: 59,
            reason: format!(
                "only {} page(s) discovered, fewer than the {MIN_PAGES} a sample needs",
                facts.pages_discovered
            ),
        });
    }
    if facts.rendering_proportion <= 0.25 {
        out.push(Cap {
            ceiling: 39,
            reason: format!(
                "{:.0}% of pages are server-rendered",
                facts.rendering_proportion * 100.0
            ),
        });
    } else if facts.rendering_proportion <= 0.50 {
        out.push(Cap {
            ceiling: 59,
            reason: format!(
                "{:.0}% of pages are server-rendered",
                facts.rendering_proportion * 100.0
            ),
        });
    }
    if facts.gated_proportion >= 0.75 {
        out.push(Cap {
            ceiling: 39,
            reason: format!(
                "{:.0}% of pages are behind an authentication gate",
                facts.gated_proportion * 100.0
            ),
        });
    } else if facts.gated_proportion >= 0.50 {
        out.push(Cap {
            ceiling: 59,
            reason: format!(
                "{:.0}% of pages are behind an authentication gate",
                facts.gated_proportion * 100.0
            ),
        });
    }
    out
}

#[derive(Debug, Clone, Copy, Default)]
struct Fraction {
    earned: f64,
    possible: f64,
}

impl Fraction {
    fn add(&mut self, earned: f64, possible: f64) {
        self.earned += earned;
        self.possible += possible;
    }

    fn percent(self) -> u32 {
        if self.possible <= 0.0 {
            return 0;
        }
        (self.earned / self.possible * 100.0).round() as u32
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Every check passing, which is the reference site.
    fn all_passing() -> Vec<CheckResult> {
        checks::CHECKS
            .iter()
            .map(|check| CheckResult::new(check.id, Outcome::Pass, "ok"))
            .collect()
    }

    fn facts() -> RunFacts {
        RunFacts {
            pages_discovered: 50,
            ..RunFacts::default()
        }
    }

    fn with(results: &mut [CheckResult], id: &str, outcome: Outcome) {
        let at = results
            .iter()
            .position(|r| r.id == id)
            .unwrap_or_else(|| panic!("{id}"));
        results[at].outcome = outcome;
    }

    fn run(results: Vec<CheckResult>, facts: &RunFacts) -> Score {
        let resolved = resolve(&results, facts);
        score(&resolved, facts)
    }

    #[test]
    fn spec_04_a_perfect_site_scores_one_hundred_and_grades_a() {
        let score = run(all_passing(), &facts());
        assert_eq!(score.comparable, 100);
        assert_eq!(score.full, 100);
        assert_eq!(score.grade, Grade::A);
        assert_eq!(score.grade.letter(), 'A');
        assert!(score.caps.is_empty(), "{:?}", score.caps);
    }

    #[test]
    fn spec_04_every_category_is_scored() {
        let score = run(all_passing(), &facts());
        assert_eq!(score.categories.len(), 7);
        for (category, value) in &score.categories {
            assert_eq!(*value, 100, "{category}");
        }
    }

    #[test]
    fn spec_04_a_warn_earns_its_coefficient_and_not_half() {
        let mut results = all_passing();
        // `llms-txt-links-markdown` is Weight::High (7) with a 0.25 warn.
        with(&mut results, "llms-txt-links-markdown", Outcome::Warn);
        let score = run(results, &facts());
        // 7 points of weight lose 75% of themselves out of 133 comparable
        // points: 100 - (5.25 / 133 × 100) ≈ 96.
        assert_eq!(score.comparable, 96);
    }

    #[test]
    fn spec_04_a_multi_page_check_scores_proportionally() {
        let mut results = all_passing();
        with(
            &mut results,
            "page-size-markdown",
            Outcome::Partial {
                passed: 47,
                total: 50,
            },
        );
        // Three of fifty pages oversized is about 94% of that check's weight,
        // not zero.
        let check = checks::get("page-size-markdown").expect("page-size-markdown");
        let share = Outcome::Partial {
            passed: 47,
            total: 50,
        }
        .share(check);
        assert!((share - 0.94).abs() < 0.005, "{share}");

        let score = run(results, &facts());
        assert_eq!(score.categories["Page Size"], 99);
        assert!(score.comparable >= 99, "{}", score.comparable);
    }

    #[test]
    fn spec_04_a_single_resource_check_is_all_or_nothing() {
        let coverage = checks::get("llms-txt-coverage").expect("llms-txt-coverage");
        assert_eq!(coverage.scope, Scope::SingleResource);
        let partial = Outcome::Partial {
            passed: 47,
            total: 50,
        };
        assert_eq!(partial.share(coverage), 0.0);
        assert_eq!(
            Outcome::Partial {
                passed: 50,
                total: 50
            }
            .share(coverage),
            1.0
        );
    }

    #[test]
    fn spec_04_no_llms_txt_caps_the_score_at_fifty_nine() {
        let mut results = all_passing();
        with(&mut results, "llms-txt-exists", Outcome::Fail);
        let score = run(results, &facts());
        assert_eq!(score.ceiling(), Some(59));
        assert!(score.comparable <= 59, "{}", score.comparable);
        assert_eq!(score.grade, Grade::F);
    }

    #[test]
    fn spec_04_the_llms_txt_quality_checks_skip_when_the_file_is_missing() {
        let mut results = all_passing();
        with(&mut results, "llms-txt-exists", Outcome::Fail);
        let resolved = resolve(&results, &facts());
        for id in [
            "llms-txt-valid",
            "llms-txt-size",
            "llms-txt-links-resolve",
            "llms-txt-links-markdown",
            "llms-txt-coverage",
        ] {
            let result = resolved.iter().find(|r| r.id == id).expect(id);
            assert!(
                matches!(result.outcome, Outcome::Skipped { .. }),
                "{id}: {:?}",
                result.outcome
            );
        }
    }

    #[test]
    fn spec_04_an_spa_shell_caps_the_score_at_thirty_nine() {
        let facts = RunFacts {
            rendering_proportion: 0.2,
            ..facts()
        };
        let score = run(all_passing(), &facts);
        assert_eq!(score.ceiling(), Some(39));
    }

    #[test]
    fn spec_04_a_half_rendered_site_caps_the_score_at_fifty_nine() {
        let facts = RunFacts {
            rendering_proportion: 0.5,
            ..facts()
        };
        assert_eq!(run(all_passing(), &facts).ceiling(), Some(59));
    }

    #[test]
    fn spec_04_a_gated_site_caps_the_score() {
        let mostly = RunFacts {
            gated_proportion: 0.8,
            ..facts()
        };
        assert_eq!(run(all_passing(), &mostly).ceiling(), Some(39));
        let half = RunFacts {
            gated_proportion: 0.5,
            ..facts()
        };
        assert_eq!(run(all_passing(), &half).ceiling(), Some(59));
    }

    #[test]
    fn spec_04_a_four_page_site_caps_the_score_and_drops_page_level_checks() {
        let facts = RunFacts {
            pages_discovered: 4,
            ..RunFacts::default()
        };
        let resolved = resolve(&all_passing(), &facts);
        let page_level = resolved
            .iter()
            .find(|r| r.id == "page-size-html")
            .expect("page-size-html");
        assert!(
            matches!(page_level.outcome, Outcome::NotApplicable { .. }),
            "{:?}",
            page_level.outcome
        );
        let site_level = resolved
            .iter()
            .find(|r| r.id == "llms-txt-exists")
            .expect("llms-txt-exists");
        assert_eq!(site_level.outcome, Outcome::Pass);
        assert_eq!(score(&resolved, &facts).ceiling(), Some(59));
    }

    #[test]
    fn spec_04_explicitly_selected_pages_escape_the_insufficient_data_rule() {
        let facts = RunFacts {
            pages_discovered: 2,
            pages_selected: true,
            ..RunFacts::default()
        };
        let resolved = resolve(&all_passing(), &facts);
        assert_eq!(
            resolved
                .iter()
                .find(|r| r.id == "page-size-html")
                .expect("page-size-html")
                .outcome,
            Outcome::Pass
        );
        assert!(score(&resolved, &facts).caps.is_empty());
    }

    #[test]
    fn spec_04_the_lowest_cap_wins() {
        let mut results = all_passing();
        with(&mut results, "llms-txt-exists", Outcome::Fail);
        let facts = RunFacts {
            no_viable_path: true,
            ..facts()
        };
        let score = run(results, &facts);
        assert_eq!(score.caps.len(), 2);
        assert_eq!(score.ceiling(), Some(39));
        assert!(score.comparable <= 39);
    }

    #[test]
    fn spec_04_the_discovery_coefficient_scales_the_cluster() {
        let mut results = all_passing();
        with(&mut results, "content-negotiation", Outcome::Fail);
        with(&mut results, "llms-txt-directive-html", Outcome::Fail);
        with(&mut results, "llms-txt-directive-md", Outcome::Fail);
        let score = run(results, &facts());
        assert_eq!(score.discovery_coefficient, 0.5);
    }

    #[test]
    fn spec_04_the_coefficient_is_one_when_content_negotiation_passes() {
        assert_eq!(discovery_coefficient(&all_passing()), 1.0);
    }

    #[test]
    fn spec_04_the_coefficient_falls_to_point_eight_on_a_directive_alone() {
        let mut results = all_passing();
        with(&mut results, "content-negotiation", Outcome::Fail);
        assert_eq!(discovery_coefficient(&results), 0.8);
    }

    #[test]
    fn spec_04_the_cluster_is_excluded_when_no_discovery_path_holds() {
        let mut results = all_passing();
        for id in [
            "content-negotiation",
            "llms-txt-directive-html",
            "llms-txt-directive-md",
            "llms-txt-links-markdown",
        ] {
            with(&mut results, id, Outcome::Fail);
        }
        let facts = facts();
        let resolved = resolve(&results, &facts);
        let score = score(&resolved, &facts);
        assert_eq!(score.discovery_coefficient, 0.0);
        // The three cluster checks contribute nothing either way, so the score
        // reflects only what the run could actually judge.
        assert!(score.comparable < 100, "{}", score.comparable);
    }

    #[test]
    fn spec_04_markdown_url_support_is_never_scaled() {
        assert!(!checks::get("markdown-url-support").expect("it").cluster);
    }

    #[test]
    fn spec_04_auth_alternative_access_runs_only_when_the_gate_is_not_clean() {
        let resolved = resolve(&all_passing(), &facts());
        assert!(matches!(
            resolved
                .iter()
                .find(|r| r.id == "auth-alternative-access")
                .expect("it")
                .outcome,
            Outcome::Skipped { .. }
        ));

        let mut results = all_passing();
        with(&mut results, "auth-gate-detection", Outcome::Warn);
        let resolved = resolve(&results, &facts());
        assert_eq!(
            resolved
                .iter()
                .find(|r| r.id == "auth-alternative-access")
                .expect("it")
                .outcome,
            Outcome::Pass
        );
    }

    #[test]
    fn spec_04_section_header_quality_skips_without_tabbed_content() {
        let mut results = all_passing();
        with(
            &mut results,
            "tabbed-content-serialization",
            Outcome::NotApplicable {
                reason: "no tabs".to_owned(),
            },
        );
        let resolved = resolve(&results, &facts());
        assert!(matches!(
            resolved
                .iter()
                .find(|r| r.id == "section-header-quality")
                .expect("it")
                .outcome,
            Outcome::Skipped { .. }
        ));
    }

    #[test]
    fn spec_04_a_failed_rendering_strategy_flags_rather_than_skips() {
        let mut results = all_passing();
        with(&mut results, "rendering-strategy", Outcome::Fail);
        let resolved = resolve(&results, &facts());
        for id in ["page-size-html", "content-start-position"] {
            let result = resolved.iter().find(|r| r.id == id).expect(id);
            assert!(result.unreliable, "{id}");
            assert_eq!(result.outcome, Outcome::Pass, "{id}");
        }
    }

    #[test]
    fn spec_04_the_comparable_score_ignores_the_unweighted_five() {
        let mut results = all_passing();
        for id in [
            "page-size-transfer",
            "single-fetch-completeness",
            "markdown-link-portability",
            "embedded-data-serialization",
            "bot-protection-interference",
        ] {
            with(&mut results, id, Outcome::Fail);
        }
        let score = run(results, &facts());
        assert_eq!(score.comparable, 100, "the comparable score must not move");
        assert!(score.full < 100, "{}", score.full);
    }

    #[test]
    fn spec_04_a_skipped_check_leaves_both_halves_of_the_fraction() {
        let mut results = all_passing();
        with(
            &mut results,
            "redirect-behavior",
            Outcome::NotApplicable {
                reason: "no redirects".to_owned(),
            },
        );
        assert_eq!(run(results, &facts()).comparable, 100);
    }

    #[test]
    fn spec_04_the_grade_bands_land_on_the_caps() {
        assert_eq!(Grade::of(100), Grade::A);
        assert_eq!(Grade::of(90), Grade::A);
        assert_eq!(Grade::of(89), Grade::B);
        assert_eq!(Grade::of(70), Grade::C);
        assert_eq!(Grade::of(60), Grade::D);
        assert_eq!(Grade::of(59), Grade::F);
        assert_eq!(Grade::of(39), Grade::F);
    }
}
