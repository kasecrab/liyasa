//! The report: interaction effects, run-level flags, and what gets printed
//! (SPEC-04, SPEC-05).
//!
//! A run that prints twelve failing checks has told the reader what broke but
//! not why. Seven combinations of failures are one problem wearing several
//! names, and the report says the problem instead.

use std::fmt::Write as _;

use liyasa_core::diagnostics::{Diagnostic, Diagnostics, code};

use super::checks::{self, Category};
use super::options::Options;
use super::score::{CheckResult, Outcome, RunFacts, Score, discovery_coefficient};

/// One interaction effect (SPEC-05), reported instead of the check failures
/// that produced it.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Effect {
    UndiscoverableMarkdown,
    NoViableContentPath,
    OversizedWithoutMarkdownEscape,
    SinglePageSample,
    CrossOriginLlmsLinks,
    BotProtectionDegradingScan,
    DynamicContentRenderedStatically,
}

impl Effect {
    pub fn title(self) -> &'static str {
        match self {
            Self::UndiscoverableMarkdown => "Markdown is served but cannot be discovered",
            Self::NoViableContentPath => "No viable content path",
            Self::OversizedWithoutMarkdownEscape => "Oversized pages with no Markdown escape",
            Self::SinglePageSample => "The run saw one page",
            Self::CrossOriginLlmsLinks => "`llms.txt` points off-origin",
            Self::BotProtectionDegradingScan => "Bot protection degraded the scan",
            Self::DynamicContentRenderedStatically => "Generated content rendered as static prose",
        }
    }

    /// The checks this effect explains, which the report shows as consequences
    /// rather than as separate problems.
    pub fn instead_of(self) -> &'static [&'static str] {
        match self {
            Self::UndiscoverableMarkdown => &[
                "content-negotiation",
                "llms-txt-directive-html",
                "llms-txt-directive-md",
                "llms-txt-links-markdown",
            ],
            Self::NoViableContentPath => &[
                "markdown-url-support",
                "content-negotiation",
                "rendering-strategy",
            ],
            Self::OversizedWithoutMarkdownEscape => &["page-size-html", "markdown-url-support"],
            Self::SinglePageSample => &[],
            Self::CrossOriginLlmsLinks => &["llms-txt-links-resolve", "llms-txt-coverage"],
            Self::BotProtectionDegradingScan => &["bot-protection-interference"],
            Self::DynamicContentRenderedStatically => &[
                "markdown-content-parity",
                "single-fetch-completeness",
                "markdown-link-portability",
                "embedded-data-serialization",
            ],
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Finding {
    pub effect: Effect,
    pub message: String,
    pub fix: String,
}

/// The whole of a `liyasa test --agents` run.
#[derive(Debug, Clone, PartialEq)]
pub struct Report {
    pub spec_version: &'static str,
    /// The version `agents.specVersion` tracks, when it differs from the
    /// implemented one (SPEC-03).
    pub tracked_version: Option<String>,
    pub results: Vec<CheckResult>,
    pub score: Score,
    pub facts: RunFacts,
    pub findings: Vec<Finding>,
}

impl Report {
    pub fn new(
        results: Vec<CheckResult>,
        score: Score,
        facts: RunFacts,
        tracked_version: Option<String>,
    ) -> Self {
        let findings = interactions(&results, &facts);
        Self {
            spec_version: checks::version(),
            tracked_version,
            results,
            score,
            facts,
            findings,
        }
    }

    pub fn result(&self, id: &str) -> Option<&CheckResult> {
        self.results.iter().find(|result| result.id == id)
    }

    /// Whether the multi-page results were computed from a sample the run could
    /// not complete.
    pub fn is_partial_sample(&self, options: &Options) -> bool {
        self.facts.fetch_failure_rate > options.thresholds.fetch_failure_rate
    }

    pub fn diagnostics(&self, options: &Options) -> Diagnostics {
        let mut out = Diagnostics::new();
        for finding in &self.findings {
            out.push(
                Diagnostic::new(
                    code::W0411,
                    format!("{}: {}", finding.effect.title(), finding.message),
                )
                .help(finding.fix.clone()),
            );
        }
        if self.is_partial_sample(options) {
            out.push(
                Diagnostic::new(
                    code::W0412,
                    format!(
                        "{:.0}% of page fetches failed; every multi-page result is a partial sample",
                        self.facts.fetch_failure_rate * 100.0
                    ),
                )
                .help("re-run against a host that is not rate-limiting the scan, or narrow the sample with `--urls`"),
            );
        }
        if let Some(tracked) = self
            .tracked_version
            .as_deref()
            .filter(|tracked| *tracked != self.spec_version)
        {
            out.push(
                Diagnostic::new(
                    code::W0410,
                    format!(
                        "`agents.specVersion` tracks {tracked}, but this build implements {}",
                        self.spec_version
                    ),
                )
                .help(
                    "set `agents.specVersion` to the implemented version, or pin a Liyasa release \
                     that implements the tracked one",
                ),
            );
        }
        out
    }

    /// The printed report: the score, the grade, the categories, then the
    /// checks, with the unweighted five in their own block (SPEC-04).
    pub fn text(&self) -> String {
        let mut out = String::new();
        let _ = writeln!(
            out,
            "Agent readiness — spec v{}: {} ({})",
            self.spec_version,
            self.score.comparable,
            self.score.grade.letter()
        );
        let _ = writeln!(
            out,
            "  comparable score {} over the checks afdocs scores; full score {} over all {}",
            self.score.comparable,
            self.score.full,
            self.results.len()
        );
        if self.score.discovery_coefficient < 1.0 {
            let _ = writeln!(
                out,
                "  Markdown-quality checks scaled by {:.2}: agents must find the Markdown first",
                self.score.discovery_coefficient
            );
        }
        for cap in &self.score.caps {
            let _ = writeln!(out, "  capped at {}: {}", cap.ceiling, cap.reason);
        }

        out.push_str("\nCategories\n");
        for category in Category::ORDER {
            if let Some(value) = self.score.categories.get(category.title()) {
                let _ = writeln!(out, "  {:<26} {value}", category.title());
            }
        }

        out.push_str("\nChecks\n");
        for category in Category::ORDER {
            for check in checks::in_category(category) {
                let Some(result) = self.result(check.id) else {
                    continue;
                };
                if !check.is_comparable() {
                    continue;
                }
                out.push_str(&line(result));
            }
        }

        out.push_str("\nNot scored by afdocs — weighted from the spec severity\n");
        for check in checks::CHECKS.iter().filter(|check| !check.is_comparable()) {
            if let Some(result) = self.result(check.id) {
                out.push_str(&line(result));
            }
        }

        if !self.findings.is_empty() {
            out.push_str("\nFindings\n");
            for finding in &self.findings {
                let _ = writeln!(out, "  {}", finding.effect.title());
                let _ = writeln!(out, "    {}", finding.message);
                let _ = writeln!(out, "    fix: {}", finding.fix);
            }
        }
        out
    }
}

fn line(result: &CheckResult) -> String {
    let verdict = match &result.outcome {
        Outcome::Pass => "pass".to_owned(),
        Outcome::Warn | Outcome::Indeterminate => "warn".to_owned(),
        Outcome::Fail => "fail".to_owned(),
        Outcome::Partial { passed, total } if passed == total => "pass".to_owned(),
        Outcome::Partial { passed, total } => format!("{passed}/{total}"),
        Outcome::Skipped { .. } => "skip".to_owned(),
        Outcome::NotApplicable { .. } => "n/a".to_owned(),
    };
    let flag = if result.unreliable {
        " (unreliable)"
    } else {
        ""
    };
    format!(
        "  {verdict:<6} {:<30} {}{flag}\n",
        result.id,
        detail(result)
    )
}

fn detail(result: &CheckResult) -> &str {
    match &result.outcome {
        Outcome::Skipped { reason } | Outcome::NotApplicable { reason } => reason,
        _ => &result.detail,
    }
}

/// The seven combinations SPEC-05 reports as one problem each.
pub fn interactions(results: &[CheckResult], facts: &RunFacts) -> Vec<Finding> {
    let mut out = Vec::new();
    let outcome = |id: &str| results.iter().find(|r| r.id == id).map(|r| &r.outcome);
    let passed = |id: &str| outcome(id).is_some_and(Outcome::is_pass);
    let failed = |id: &str| outcome(id).is_some_and(Outcome::is_fail);

    if facts.no_viable_path {
        out.push(Finding {
            effect: Effect::NoViableContentPath,
            message: "no route returned content an agent can read: neither a Markdown route, nor \
                      content negotiation, nor server-rendered HTML"
                .to_owned(),
            fix: "serve `<route>.md` for every page; a static host needs a rewrite rule, a server \
                  needs the Markdown route enabled"
                .to_owned(),
        });
    } else if passed("markdown-url-support") && discovery_coefficient(results) == 0.0 {
        out.push(Finding {
            effect: Effect::UndiscoverableMarkdown,
            message: "every page has a Markdown route, but nothing points at one: content \
                      negotiation does not answer, no page carries the discovery directive, and \
                      `llms.txt` does not link `.md` URLs"
                .to_owned(),
            fix: "add the discovery blockquote to each Markdown page and the hidden directive to \
                  each HTML page; both are one line and both are what an agent looks for"
                .to_owned(),
        });
    }

    if failed("page-size-html") && !passed("markdown-url-support") {
        out.push(Finding {
            effect: Effect::OversizedWithoutMarkdownEscape,
            message: "pages are over the size budget and there is no Markdown route to read \
                      instead, so an agent has to parse the whole document or give up"
                .to_owned(),
            fix: "publish `<route>.md`; it is the escape hatch the size checks assume exists"
                .to_owned(),
        });
    }

    if facts.pages_discovered == 1 {
        out.push(Finding {
            effect: Effect::SinglePageSample,
            message: "one page was discovered, so every multi-page result describes that page \
                      and not the site"
                .to_owned(),
            fix: "check that navigation links are real anchors and that `llms.txt` lists the \
                  pages, then re-run"
                .to_owned(),
        });
    }

    if facts.cross_origin_llms_links > 0 {
        out.push(Finding {
            effect: Effect::CrossOriginLlmsLinks,
            message: format!(
                "{} `llms.txt` entr{} point outside `seo.canonicalOrigin`, so the coverage and \
                 resolve checks are measuring somebody else's site",
                facts.cross_origin_llms_links,
                if facts.cross_origin_llms_links == 1 {
                    "y"
                } else {
                    "ies"
                }
            ),
            fix: "build every entry from `seo.canonicalOrigin`, or set it to the origin the site \
                  is actually served from"
                .to_owned(),
        });
    }

    if matches!(
        outcome("bot-protection-interference"),
        Some(Outcome::Warn | Outcome::Fail)
    ) {
        out.push(Finding {
            effect: Effect::BotProtectionDegradingScan,
            message: "the host answered part of the scan with a challenge or a hold, so every \
                      multi-page result is computed from a partial sample"
                .to_owned(),
            fix: "exempt documentation routes from behavioural bot management, and answer an \
                  over-limit request with `429` and `Retry-After` rather than an interstitial"
                .to_owned(),
        });
    }

    let dynamic = [
        "markdown-content-parity",
        "single-fetch-completeness",
        "markdown-link-portability",
        "embedded-data-serialization",
    ];
    if dynamic.iter().all(|id| failed(id)) {
        out.push(Finding {
            effect: Effect::DynamicContentRenderedStatically,
            message: "parity, single-fetch, portability, and bulk attribution fail together, \
                      which is one generated page published as if it were prose rather than four \
                      separate problems"
                .to_owned(),
            fix: "render the generated content through the same pipeline as the rest of the page, \
                  or publish it as data an agent can fetch on its own"
                .to_owned(),
        });
    }

    out
}

#[cfg(test)]
mod tests {
    use super::super::score::{self, RunFacts};
    use super::*;

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

    fn report(results: Vec<CheckResult>, facts: RunFacts) -> Report {
        let resolved = score::resolve(&results, &facts);
        let score = score::score(&resolved, &facts);
        Report::new(resolved, score, facts, None)
    }

    fn effects(report: &Report) -> Vec<Effect> {
        report.findings.iter().map(|f| f.effect).collect()
    }

    #[test]
    fn spec_05_a_clean_run_reports_no_interaction_effect() {
        let report = report(all_passing(), facts());
        assert!(report.findings.is_empty(), "{:?}", report.findings);
        assert!(report.diagnostics(&Options::default()).is_empty());
    }

    #[test]
    fn spec_05_undiscoverable_markdown_is_one_finding_not_four() {
        let mut results = all_passing();
        for id in [
            "content-negotiation",
            "llms-txt-directive-html",
            "llms-txt-directive-md",
            "llms-txt-links-markdown",
        ] {
            with(&mut results, id, Outcome::Fail);
        }
        let report = report(results, facts());
        assert_eq!(effects(&report), [Effect::UndiscoverableMarkdown]);
        assert_eq!(
            Effect::UndiscoverableMarkdown.instead_of().len(),
            4,
            "the finding stands in for four check failures"
        );
    }

    #[test]
    fn spec_05_no_viable_content_path_is_reported_as_itself() {
        let mut results = all_passing();
        for id in [
            "markdown-url-support",
            "content-negotiation",
            "rendering-strategy",
        ] {
            with(&mut results, id, Outcome::Fail);
        }
        let facts = RunFacts {
            no_viable_path: true,
            rendering_proportion: 0.0,
            ..facts()
        };
        let report = report(results, facts);
        assert!(effects(&report).contains(&Effect::NoViableContentPath));
        assert!(!effects(&report).contains(&Effect::UndiscoverableMarkdown));
    }

    #[test]
    fn spec_05_oversized_pages_with_no_markdown_escape() {
        let mut results = all_passing();
        with(&mut results, "page-size-html", Outcome::Fail);
        with(&mut results, "markdown-url-support", Outcome::Fail);
        let report = report(results, facts());
        assert!(effects(&report).contains(&Effect::OversizedWithoutMarkdownEscape));
    }

    #[test]
    fn spec_05_a_single_page_sample_is_flagged() {
        let facts = RunFacts {
            pages_discovered: 1,
            ..RunFacts::default()
        };
        assert!(effects(&report(all_passing(), facts)).contains(&Effect::SinglePageSample));
    }

    #[test]
    fn spec_05_cross_origin_llms_links_are_named() {
        let facts = RunFacts {
            cross_origin_llms_links: 3,
            ..facts()
        };
        let report = report(all_passing(), facts);
        let finding = report
            .findings
            .iter()
            .find(|f| f.effect == Effect::CrossOriginLlmsLinks)
            .expect("the finding");
        assert!(finding.message.contains('3'), "{}", finding.message);
        assert!(finding.message.contains("entries"), "{}", finding.message);
    }

    #[test]
    fn spec_05_bot_protection_is_reported_once() {
        let mut results = all_passing();
        with(&mut results, "bot-protection-interference", Outcome::Warn);
        assert!(effects(&report(results, facts())).contains(&Effect::BotProtectionDegradingScan));
    }

    #[test]
    fn spec_05_four_failures_on_generated_content_are_one_pipeline_problem() {
        let mut results = all_passing();
        for id in [
            "markdown-content-parity",
            "single-fetch-completeness",
            "markdown-link-portability",
            "embedded-data-serialization",
        ] {
            with(&mut results, id, Outcome::Fail);
        }
        let report = report(results, facts());
        assert!(effects(&report).contains(&Effect::DynamicContentRenderedStatically));
    }

    #[test]
    fn spec_05_three_of_the_four_are_not_the_pipeline_problem() {
        let mut results = all_passing();
        for id in [
            "markdown-content-parity",
            "single-fetch-completeness",
            "markdown-link-portability",
        ] {
            with(&mut results, id, Outcome::Fail);
        }
        assert!(
            !effects(&report(results, facts())).contains(&Effect::DynamicContentRenderedStatically)
        );
    }

    #[test]
    fn spec_05_a_quarter_of_fetches_failing_flags_the_run() {
        let facts = RunFacts {
            fetch_failure_rate: 0.25,
            ..facts()
        };
        let report = report(all_passing(), facts);
        let options = Options::default();
        assert!(report.is_partial_sample(&options));
        let codes: Vec<&str> = report
            .diagnostics(&options)
            .iter()
            .map(|d| d.code.as_str())
            .collect();
        assert_eq!(codes, ["W0412"]);
    }

    #[test]
    fn spec_05_a_fifth_of_fetches_failing_does_not() {
        let facts = RunFacts {
            fetch_failure_rate: 0.20,
            ..facts()
        };
        assert!(!report(all_passing(), facts).is_partial_sample(&Options::default()));
    }

    #[test]
    fn spec_05_every_finding_becomes_one_diagnostic() {
        let mut results = all_passing();
        with(&mut results, "bot-protection-interference", Outcome::Fail);
        let facts = RunFacts {
            pages_discovered: 1,
            cross_origin_llms_links: 1,
            ..RunFacts::default()
        };
        let report = report(results, facts);
        let diagnostics = report.diagnostics(&Options::default());
        assert_eq!(
            diagnostics
                .iter()
                .filter(|d| d.code.as_str() == "W0411")
                .count(),
            report.findings.len()
        );
        assert!(diagnostics.iter().all(|d| d.help.is_some()));
    }

    #[test]
    fn spec_03_a_tracked_version_that_differs_warns() {
        let resolved = score::resolve(&all_passing(), &facts());
        let score = score::score(&resolved, &facts());
        let report = Report::new(resolved, score, facts(), Some("0.5.0".to_owned()));
        let codes: Vec<&str> = report
            .diagnostics(&Options::default())
            .iter()
            .map(|d| d.code.as_str())
            .collect();
        assert_eq!(codes, ["W0410"]);
    }

    #[test]
    fn spec_03_the_tracked_version_matching_says_nothing() {
        let resolved = score::resolve(&all_passing(), &facts());
        let score = score::score(&resolved, &facts());
        let report = Report::new(resolved, score, facts(), Some("0.6.0".to_owned()));
        assert!(report.diagnostics(&Options::default()).is_empty());
    }

    #[test]
    fn spec_04_the_printed_report_carries_the_score_the_grade_and_the_categories() {
        let text = report(all_passing(), facts()).text();
        assert!(
            text.contains("Agent readiness — spec v0.6.0: 100 (A)"),
            "{text}"
        );
        for category in Category::ORDER {
            assert!(text.contains(category.title()), "{text}");
        }
        assert!(
            text.contains("Not scored by afdocs — weighted from the spec severity"),
            "{text}"
        );
        for id in [
            "page-size-transfer",
            "single-fetch-completeness",
            "markdown-link-portability",
            "embedded-data-serialization",
            "bot-protection-interference",
        ] {
            let block = text
                .split("Not scored by afdocs")
                .nth(1)
                .expect("the separate block");
            assert!(
                block.contains(id),
                "{id} is missing from the separate block"
            );
        }
    }

    #[test]
    fn spec_04_the_report_names_the_caps_that_fired() {
        let mut results = all_passing();
        with(&mut results, "llms-txt-exists", Outcome::Fail);
        let text = report(results, facts()).text();
        assert!(text.contains("capped at 59"), "{text}");
        assert!(text.contains("`llms-txt-exists` failed"), "{text}");
    }

    #[test]
    fn spec_04_an_unreliable_result_says_so() {
        let mut results = all_passing();
        with(&mut results, "rendering-strategy", Outcome::Fail);
        let text = report(results, facts()).text();
        assert!(text.contains("(unreliable)"), "{text}");
    }

    #[test]
    fn spec_04_a_proportional_result_shows_its_fraction() {
        let mut results = all_passing();
        with(
            &mut results,
            "page-size-markdown",
            Outcome::Partial {
                passed: 47,
                total: 50,
            },
        );
        let text = report(results, facts()).text();
        assert!(text.contains("47/50"), "{text}");
    }
}
