//! The service level indicators and objectives HOST-10 publishes, and the
//! alert rules derived from them.
//!
//! HOST-10's acceptance criterion is that *each* SLI has a recording rule, a
//! burn-rate alert at both windows, and a runbook link. That is a property of
//! a table, so the table is here and the property is a test — rather than six
//! hand-written alert files where the seventh SLI is the one somebody forgets.
//!
//! Deploying these is `infra/`, which this package does not own; what is here
//! is the source they are generated from. `infra/tests/host_10_slo.rs`, the
//! acceptance test HOST-10 names, does not exist yet and belongs with whoever
//! owns that directory.

use std::time::Duration;

use serde::{Deserialize, Serialize};

/// What an objective is about.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "camelCase")]
pub enum Objective {
    /// Parts per million of requests that must succeed. 99.99% is 999_900.
    Availability { ppm: u32 },
    /// A percentile that must stay under a bound.
    Latency { percentile: u8, budget: Duration },
}

impl Objective {
    /// How much failure the objective allows over `period`. `None` for a
    /// latency objective, whose budget is not a duration of downtime.
    pub fn error_budget(self, period: Duration) -> Option<Duration> {
        match self {
            Objective::Availability { ppm } => {
                let allowed_ppm = u64::from(1_000_000u32.saturating_sub(ppm));
                Some(Duration::from_millis(
                    (period.as_millis() as u64).saturating_mul(allowed_ppm) / 1_000_000,
                ))
            }
            Objective::Latency { .. } => None,
        }
    }

    pub fn describe(self) -> String {
        match self {
            Objective::Availability { ppm } => {
                format!("{}.{:04}% of requests succeed", ppm / 10_000, ppm % 10_000)
            }
            Objective::Latency { percentile, budget } => {
                format!("p{percentile} under {} ms", budget.as_millis())
            }
        }
    }
}

/// One indicator: what is measured, against what, and where to look when it
/// is breached.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub struct Sli {
    pub key: &'static str,
    pub title: &'static str,
    /// What is counted. HOST-10 names the measurement for the first one in
    /// terms, and the others follow the same shape: a ratio or a histogram at
    /// a named place.
    pub measurement: &'static str,
    pub objective: Objective,
    /// The recording rule the alerts evaluate, rather than the raw series.
    pub recording_rule: &'static str,
    pub runbook: &'static str,
}

/// The period an objective is stated over: HOST-10 says "monthly".
pub const PERIOD: Duration = Duration::from_secs(30 * 24 * 60 * 60);

/// The windows HOST-10 alerts over.
pub const WINDOWS: [Duration; 2] = [Duration::from_secs(3_600), Duration::from_secs(6 * 3_600)];

/// The fractions of the error budget that page.
pub const BURN_THRESHOLDS: [u8; 2] = [50, 100];

pub const RUNBOOK_BASE: &str = "https://kasecrab.github.io/liyasa/docs/runbooks";

/// Every SLI HOST-10 names, in the order it names them.
static CATALOGUE: [Sli; 6] = [
    Sli {
        key: "published_site_availability",
        title: "Published sites answer",
        measurement: "successful responses at the CDN edge for static routes",
        objective: Objective::Availability { ppm: 999_900 },
        recording_rule: "liyasa:edge_static_success:ratio_rate5m",
        runbook: "https://kasecrab.github.io/liyasa/docs/runbooks/published-site-availability",
    },
    Sli {
        key: "control_plane_availability",
        title: "Dashboard, editor and API answer",
        measurement: "successful responses from the dashboard, editor and REST API",
        objective: Objective::Availability { ppm: 999_000 },
        recording_rule: "liyasa:control_plane_success:ratio_rate5m",
        runbook: "https://kasecrab.github.io/liyasa/docs/runbooks/control-plane-availability",
    },
    Sli {
        key: "cached_page_ttfb",
        title: "Cached pages start fast",
        measurement: "time to first byte for edge-cached page responses",
        objective: Objective::Latency {
            percentile: 95,
            budget: Duration::from_millis(100),
        },
        recording_rule: "liyasa:edge_cached_ttfb_seconds:p95",
        runbook: "https://kasecrab.github.io/liyasa/docs/runbooks/cached-page-latency",
    },
    Sli {
        key: "deploy_queue_wait",
        title: "Production deploys start promptly",
        measurement: "time between a production deploy being queued and being picked up",
        objective: Objective::Latency {
            percentile: 95,
            budget: Duration::from_secs(30),
        },
        recording_rule: "liyasa:deploy_queue_wait_seconds:p95",
        runbook: "https://kasecrab.github.io/liyasa/docs/runbooks/deploy-queue-wait",
    },
    Sli {
        key: "single_page_deploy",
        title: "A single-page deploy finishes",
        measurement: "end to end duration of a deploy touching one page",
        objective: Objective::Latency {
            percentile: 95,
            budget: Duration::from_secs(60),
        },
        recording_rule: "liyasa:single_page_deploy_seconds:p95",
        runbook: "https://kasecrab.github.io/liyasa/docs/runbooks/single-page-deploy",
    },
    Sli {
        key: "assistant_first_token",
        title: "The assistant starts answering",
        measurement: "time from an assistant request to its first streamed token",
        objective: Objective::Latency {
            percentile: 95,
            budget: Duration::from_secs(3),
        },
        recording_rule: "liyasa:assistant_first_token_seconds:p95",
        runbook: "https://kasecrab.github.io/liyasa/docs/runbooks/assistant-first-token",
    },
];

pub fn catalogue() -> &'static [Sli] {
    &CATALOGUE
}

pub fn sli(key: &str) -> Option<&'static Sli> {
    catalogue().iter().find(|sli| sli.key == key)
}

/// One generated alert rule. HOST-10: "Alerts fire at 50% and 100% error-budget
/// burn over 1 h and 6 h windows and page the on-call engineer; runbooks are
/// linked from every alert."
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct BurnAlert {
    pub name: String,
    pub sli: &'static str,
    pub window: Duration,
    pub budget_percent: u8,
    /// Every alert here pages: HOST-10 says so, and an alert that does not
    /// page is a dashboard.
    pub pages: bool,
    pub expression: String,
    pub runbook: &'static str,
    pub summary: String,
}

/// Every alert HOST-10 asks for: one per SLI per window per threshold.
pub fn alerts() -> Vec<BurnAlert> {
    let mut out = Vec::new();
    for sli in catalogue() {
        for window in WINDOWS {
            for percent in BURN_THRESHOLDS {
                let window_label = label(window);
                out.push(BurnAlert {
                    name: format!("Liyasa{}Burn{percent}Over{window_label}", camel(sli.key)),
                    sli: sli.key,
                    window,
                    budget_percent: percent,
                    pages: true,
                    expression: expression(sli, window, percent),
                    runbook: sli.runbook,
                    summary: format!(
                        "{} has burned {percent}% of its error budget in {window_label} ({})",
                        sli.title,
                        sli.objective.describe()
                    ),
                });
            }
        }
    }
    out
}

fn label(window: Duration) -> String {
    let hours = window.as_secs() / 3_600;
    format!("{hours}h")
}

fn camel(key: &str) -> String {
    key.split('_')
        .map(|word| {
            let mut chars = word.chars();
            match chars.next() {
                Some(first) => first.to_ascii_uppercase().to_string() + chars.as_str(),
                None => String::new(),
            }
        })
        .collect()
}

/// The burn-rate expression: how many times faster than the objective allows
/// this window is consuming the budget.
fn expression(sli: &Sli, window: Duration, percent: u8) -> String {
    let window_label = label(window);
    match sli.objective {
        Objective::Availability { ppm } => {
            let budget = f64::from(1_000_000u32 - ppm) / 1_000_000.0;
            let rate = burn_rate(window, percent);
            format!(
                "(1 - {rule}[{window_label}]) > {rate:.2} * {budget}",
                rule = sli.recording_rule
            )
        }
        Objective::Latency { budget, .. } => {
            let seconds = budget.as_secs_f64();
            let rate = burn_rate(window, percent);
            format!(
                "{rule}[{window_label}] > {seconds} and {rule}_violation_ratio[{window_label}] \
                 > {rate:.2} * 0.05",
                rule = sli.recording_rule
            )
        }
    }
}

/// Burning `percent` of a monthly budget inside `window` is this many times
/// the sustainable rate.
pub fn burn_rate(window: Duration, percent: u8) -> f64 {
    let share = f64::from(percent) / 100.0;
    let periods = PERIOD.as_secs_f64() / window.as_secs_f64();
    share * periods
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn host_10_names_six_indicators_and_every_one_is_here() {
        let keys: Vec<&str> = catalogue().iter().map(|sli| sli.key).collect();
        assert_eq!(keys.len(), 6, "{keys:?}");
        for key in [
            "published_site_availability",
            "control_plane_availability",
            "cached_page_ttfb",
            "deploy_queue_wait",
            "single_page_deploy",
            "assistant_first_token",
        ] {
            assert!(sli(key).is_some(), "{key} is missing");
        }
    }

    #[test]
    fn every_indicator_has_a_recording_rule_a_runbook_and_a_measurement() {
        // HOST-10's acceptance criterion, as a property of the table rather
        // than as six files somebody has to remember to write.
        for sli in catalogue() {
            assert!(!sli.recording_rule.is_empty(), "{}", sli.key);
            assert!(
                sli.recording_rule.starts_with("liyasa:"),
                "{} is not a recording rule name",
                sli.recording_rule
            );
            assert!(
                sli.runbook.starts_with(RUNBOOK_BASE),
                "{} does not link a runbook",
                sli.key
            );
            assert!(!sli.measurement.is_empty(), "{}", sli.key);
        }
    }

    #[test]
    fn every_indicator_has_a_paging_alert_at_both_thresholds_and_both_windows() {
        let alerts = alerts();
        assert_eq!(alerts.len(), 6 * 2 * 2);
        for sli in catalogue() {
            for window in WINDOWS {
                for percent in BURN_THRESHOLDS {
                    let alert = alerts
                        .iter()
                        .find(|a| {
                            a.sli == sli.key && a.window == window && a.budget_percent == percent
                        })
                        .unwrap_or_else(|| {
                            panic!("{} has no {percent}% alert over {window:?}", sli.key)
                        });
                    assert!(alert.pages, "{} does not page", alert.name);
                    assert_eq!(alert.runbook, sli.runbook);
                    assert!(
                        alert.expression.contains(sli.recording_rule),
                        "{}",
                        alert.expression
                    );
                }
            }
        }
    }

    #[test]
    fn alert_names_are_unique() {
        let alerts = alerts();
        let mut names: Vec<&str> = alerts.iter().map(|a| a.name.as_str()).collect();
        let before = names.len();
        names.sort_unstable();
        names.dedup();
        assert_eq!(names.len(), before);
    }

    #[test]
    fn the_objectives_are_the_numbers_host_10_publishes() {
        assert_eq!(
            sli("published_site_availability")
                .expect("the SLI")
                .objective,
            Objective::Availability { ppm: 999_900 }
        );
        assert_eq!(
            sli("control_plane_availability")
                .expect("the SLI")
                .objective,
            Objective::Availability { ppm: 999_000 }
        );
        assert_eq!(
            sli("cached_page_ttfb").expect("the SLI").objective,
            Objective::Latency {
                percentile: 95,
                budget: Duration::from_millis(100)
            }
        );
        assert_eq!(
            sli("deploy_queue_wait").expect("the SLI").objective,
            Objective::Latency {
                percentile: 95,
                budget: Duration::from_secs(30)
            }
        );
        assert_eq!(
            sli("single_page_deploy").expect("the SLI").objective,
            Objective::Latency {
                percentile: 95,
                budget: Duration::from_secs(60)
            }
        );
        assert_eq!(
            sli("assistant_first_token").expect("the SLI").objective,
            Objective::Latency {
                percentile: 95,
                budget: Duration::from_secs(3)
            }
        );
    }

    #[test]
    fn four_nines_a_month_is_four_and_a_third_minutes() {
        let budget = Objective::Availability { ppm: 999_900 }
            .error_budget(PERIOD)
            .expect("an availability budget");
        assert_eq!(budget.as_secs(), 259, "{budget:?}");

        let three_nines = Objective::Availability { ppm: 999_000 }
            .error_budget(PERIOD)
            .expect("an availability budget");
        assert_eq!(three_nines.as_secs(), 2_592);

        assert!(
            Objective::Latency {
                percentile: 95,
                budget: Duration::from_secs(3)
            }
            .error_budget(PERIOD)
            .is_none(),
            "a latency objective's budget is not a duration of downtime"
        );
    }

    #[test]
    fn burning_the_whole_month_in_an_hour_is_the_month_over_the_hour() {
        // 30 days is 720 hours, so spending 100% of the budget in one hour is
        // 720 times the sustainable rate, and 50% in six hours is 60 times.
        assert!((burn_rate(WINDOWS[0], 100) - 720.0).abs() < 0.01);
        assert!((burn_rate(WINDOWS[1], 50) - 60.0).abs() < 0.01);
    }

    #[test]
    fn the_objective_descriptions_read_as_the_published_numbers() {
        assert_eq!(
            Objective::Availability { ppm: 999_900 }.describe(),
            "99.9900% of requests succeed"
        );
        assert_eq!(
            Objective::Latency {
                percentile: 95,
                budget: Duration::from_millis(100)
            }
            .describe(),
            "p95 under 100 ms"
        );
    }
}
