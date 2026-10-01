//! The changelog table and the budget verdict.
//!
//! NFR-01 asks for the figures to be *published*, so the output is Markdown a
//! release note can paste, not a log line. The JSON beside it is what the next
//! release compares against.

use std::fmt::Write as _;
use std::time::Duration;

use crate::budget::{Budget, Limit, Metric, SIX_SIX, THIRTY_ONE};
use crate::measure::Measurement;

/// One budget against one run.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Verdict {
    Met,
    Missed {
        measured: String,
        allowed: String,
    },
    /// The machine has fewer cores than the figure assumes, or the platform
    /// publishes no high-water mark. Not a pass and not a failure.
    NotApplicable(String),
    /// The run did not cover that size.
    NotMeasured,
}

impl Verdict {
    pub fn missed(&self) -> bool {
        matches!(self, Self::Missed { .. })
    }
}

pub fn judge(budget: &Budget, runs: &[Measurement]) -> Verdict {
    let Some(run) = runs.iter().find(|m| m.pages == budget.pages) else {
        return Verdict::NotMeasured;
    };
    if run.cores < budget.cores {
        return Verdict::NotApplicable(format!(
            "the figure assumes {} cores and this machine has {}",
            budget.cores, run.cores
        ));
    }
    let Some(measured) = run.metric(budget.metric) else {
        return Verdict::NotApplicable("the platform publishes no high-water mark".to_owned());
    };
    let within = match (measured, budget.limit) {
        (Limit::Time(had), Limit::Time(allowed)) => had <= allowed,
        (Limit::Resident(had), Limit::Resident(allowed)) => had <= allowed,
        // A time budget cannot judge a memory measurement; saying so beats
        // returning a verdict from a comparison that did not happen.
        _ => {
            return Verdict::NotApplicable("the budget and the measurement disagree".to_owned());
        }
    };
    if within {
        Verdict::Met
    } else {
        Verdict::Missed {
            measured: show(measured),
            allowed: show(budget.limit),
        }
    }
}

pub fn show(limit: Limit) -> String {
    match limit {
        Limit::Time(d) if d.as_millis() < 1_000 => format!("{} ms", d.as_millis()),
        Limit::Time(d) => format!("{:.2} s", d.as_secs_f64()),
        Limit::Resident(bytes) => {
            let mb = bytes as f64 / (1024.0 * 1024.0);
            if mb < 1024.0 {
                format!("{mb:.0} MB")
            } else {
                format!("{:.2} GB", mb / 1024.0)
            }
        }
    }
}

/// The table a release note publishes.
pub fn table(runs: &[Measurement]) -> String {
    let mut out = String::from(
        "| Site | Clean build | Warm build | One-page edit (p50) | One-page edit (p95) | Navigation change | Peak resident |\n\
         |---|---|---|---|---|---|---|\n",
    );
    for run in runs {
        let resident = run
            .peak_resident_bytes
            .map_or_else(|| "—".to_owned(), |b| show(Limit::Resident(b)));
        let _ = writeln!(
            out,
            "| {} pages | {} | {} | {} | {} | {} | {resident} |",
            thousands(run.pages),
            show(Limit::Time(std::time::Duration::from_millis(run.clean_ms))),
            show(Limit::Time(std::time::Duration::from_millis(run.warm_ms))),
            show(Limit::Time(std::time::Duration::from_millis(
                run.edit_p50_ms
            ))),
            show(Limit::Time(std::time::Duration::from_millis(
                run.edit_p95_ms
            ))),
            show(Limit::Time(std::time::Duration::from_millis(
                run.navigation_ms
            ))),
        );
    }
    out
}

/// The §6.6 budgets, each against the run that covers it.
pub fn budgets(runs: &[Measurement]) -> String {
    let mut out =
        String::from("| §6.6 scenario | Budget | Measured | Verdict |\n|---|---|---|---|\n");
    for budget in SIX_SIX {
        let verdict = judge(budget, runs);
        let (measured, note) = match &verdict {
            Verdict::Met => (
                runs.iter()
                    .find(|m| m.pages == budget.pages)
                    .and_then(|m| m.metric(budget.metric))
                    .map_or_else(|| "—".to_owned(), show),
                "met".to_owned(),
            ),
            Verdict::Missed { measured, .. } => (measured.clone(), "**missed**".to_owned()),
            Verdict::NotApplicable(why) => ("—".to_owned(), format!("not applicable: {why}")),
            Verdict::NotMeasured => ("—".to_owned(), "not measured in this run".to_owned()),
        };
        let _ = writeln!(
            out,
            "| {} | {} | {measured} | {note} |",
            budget.scenario,
            show(budget.limit)
        );
    }
    out
}

/// The §30.1 rows this suite does not measure, with the reason.
///
/// Printed beside the measured table rather than left out. §30.1 has six
/// requirements and only NFR-01's build figures are measurable from a build
/// harness, so a release that published only what this suite measures would
/// show one row and silently omit eight — and an omitted budget reads as one
/// nobody set, not one nobody measured.
pub fn unmeasured() -> String {
    let mut out = String::from("| §30.1 runtime budget | Budget | Needs |\n|---|---|---|\n");
    for runtime in THIRTY_ONE {
        let _ = writeln!(
            out,
            "| {} | {} | {} |",
            runtime.row,
            show(runtime.limit),
            runtime.needs
        );
    }
    out
}

/// The editor preview's native lower bound (NFR-05).
///
/// Its own table, deliberately NOT a row in `budgets()` and NOT a row in
/// `unmeasured()`. It is neither: the budget is a browser figure and this is a
/// native one, so presenting it as the row's measurement would overclaim, and
/// presenting it as unmeasured would throw away a number that can falsify the
/// budget. The `Verdict` column says which of the two things a reader is looking
/// at, in words, because a number beside a budget gets read as that budget.
pub fn editor_lower_bound(preview: &crate::editor::Preview, slowest: Duration) -> String {
    let budget = Duration::from_millis(50);
    let verdict = if preview.elapsed > budget {
        "**misses** — the browser cannot be faster than this"
    } else {
        "not lost here; the browser layer is untested"
    };
    let mut out = String::from(
        "| Editor preview, native lower bound | Browser budget | Median | Slowest | Verdict |\n\
         |---|---|---|---|---|\n",
    );
    let _ = writeln!(
        out,
        "| {} words, {} bytes of HTML | {} | {} | {} | {verdict} |",
        thousands(preview.words),
        thousands(preview.html_bytes),
        show(Limit::Time(budget)),
        show(Limit::Time(preview.elapsed)),
        show(Limit::Time(slowest))
    );
    out
}

/// What the engine reported about the work, so a suspiciously fast figure can
/// be checked against the page count it came from.
pub fn provenance(runs: &[Measurement]) -> String {
    let mut out = String::from(
        "| Site | Pages built | Variants | Warm cache misses | Edit samples | Cores |\n|---|---|---|---|---|---|\n",
    );
    for run in runs {
        let _ = writeln!(
            out,
            "| {} pages | {} | {} | {} | {} | {} |",
            thousands(run.pages),
            run.built_pages,
            run.variants,
            run.warm_cache_misses,
            run.edit_samples,
            run.cores
        );
    }
    out
}

pub fn missed(runs: &[Measurement]) -> Vec<&'static Budget> {
    SIX_SIX
        .iter()
        .filter(|budget| judge(budget, runs).missed())
        .collect()
}

pub fn metric_label(metric: Metric) -> &'static str {
    metric.label()
}

fn thousands(n: usize) -> String {
    let digits = n.to_string();
    let mut out = String::new();
    for (at, digit) in digits.chars().enumerate() {
        if at > 0 && (digits.len() - at).is_multiple_of(3) {
            out.push(',');
        }
        out.push(digit);
    }
    out
}
