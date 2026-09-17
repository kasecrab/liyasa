//! AI credits for managed model access (ORG-31, HOST-13), and the token
//! estimates the published price table is reconciled against (ORG-33).
//!
//! The one thing worth reading twice: BYOK does not spend credits, and it does
//! not spend sandbox minutes either. Those are different resources with
//! different owners — the model bill is the customer's under BYOK and the
//! machine running the verification is still ours (ORG-33).

use std::collections::{BTreeMap, BTreeSet};

use serde::{Deserialize, Serialize};

/// How the model was reached. `Byok` bypasses credits entirely (ORG-31).
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum Access {
    #[default]
    Managed,
    Byok,
}

/// The published size table (ORG-31: "a published table from 5 to 320
/// credits"). The numbers are the contract, so they live here and the
/// documentation is generated from them rather than the other way around.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum TaskSize {
    Minimal,
    Small,
    Medium,
    Large,
    Maximum,
}

impl TaskSize {
    pub const ALL: &'static [TaskSize] = &[
        TaskSize::Minimal,
        TaskSize::Small,
        TaskSize::Medium,
        TaskSize::Large,
        TaskSize::Maximum,
    ];

    pub fn credits(self) -> u64 {
        match self {
            TaskSize::Minimal => 5,
            TaskSize::Small => 15,
            TaskSize::Medium => 40,
            TaskSize::Large => 100,
            TaskSize::Maximum => 320,
        }
    }

    /// The token estimate the price is derived from (ORG-33). An agent run
    /// reconciles against this rather than against the credit number, because
    /// the credit number is a rounding of it.
    pub fn estimated_tokens(self) -> u64 {
        self.credits().saturating_mul(TOKENS_PER_CREDIT)
    }

    pub fn as_str(self) -> &'static str {
        match self {
            TaskSize::Minimal => "minimal",
            TaskSize::Small => "small",
            TaskSize::Medium => "medium",
            TaskSize::Large => "large",
            TaskSize::Maximum => "maximum",
        }
    }

    pub fn parse(text: &str) -> Option<TaskSize> {
        TaskSize::ALL
            .iter()
            .copied()
            .find(|s| s.as_str().eq_ignore_ascii_case(text))
    }

    /// The smallest size whose estimate covers `tokens`, which is how a run
    /// that came in larger than it was quoted is repriced.
    pub fn for_tokens(tokens: u64) -> TaskSize {
        TaskSize::ALL
            .iter()
            .copied()
            .find(|size| size.estimated_tokens() >= tokens)
            .unwrap_or(TaskSize::Maximum)
    }
}

/// How many model tokens one credit is expected to buy (ORG-33).
pub const TOKENS_PER_CREDIT: u64 = 2_500;

/// What a credit costs us and what we charge for it (ORG-33: "credits are
/// priced from provider list price times a published margin, recomputed
/// monthly").
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub struct Pricing {
    /// Provider list price for a thousand tokens, in micro-dollars.
    pub list_micros_per_1k_tokens: u64,
    /// The published margin, in percent over list.
    pub margin_percent: u64,
    /// The month this was computed for, as `YYYY-MM`. A price with no period
    /// cannot be shown to be current, and ORG-33 asks for it monthly.
    pub period: &'static str,
}

impl Pricing {
    pub fn micros_per_credit(&self) -> u64 {
        let list = self
            .list_micros_per_1k_tokens
            .saturating_mul(TOKENS_PER_CREDIT)
            / 1_000;
        list.saturating_mul(100 + self.margin_percent) / 100
    }
}

/// Which surface spent the credit, for the usage dashboard (ORG-31).
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum Surface {
    Assistant,
    Agent,
}

impl Surface {
    pub fn as_str(self) -> &'static str {
        match self {
            Surface::Assistant => "assistant",
            Surface::Agent => "agent",
        }
    }
}

/// What is being charged for.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "camelCase")]
pub enum Spend {
    /// ORG-31: one credit per assistant answer.
    AssistantAnswer,
    AgentTask { size: TaskSize },
}

impl Spend {
    pub fn credits(self) -> u64 {
        match self {
            Spend::AssistantAnswer => 1,
            Spend::AgentTask { size } => size.credits(),
        }
    }

    pub fn surface(self) -> Surface {
        match self {
            Spend::AssistantAnswer => Surface::Assistant,
            Spend::AgentTask { .. } => Surface::Agent,
        }
    }
}

/// What a charge did.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "outcome", rename_all = "camelCase")]
pub enum Charge {
    /// BYOK: no credit moved, because the customer paid the provider.
    Bypassed,
    Charged {
        credits: u64,
        balance: u64,
    },
    /// The overage toggle is on, so the work runs and the credits are billed.
    Overage {
        credits: u64,
        overage_total: u64,
    },
    /// The overage toggle is off and the balance will not cover it.
    Refused {
        needed: u64,
        balance: u64,
    },
}

impl Charge {
    pub fn ran(&self) -> bool {
        !matches!(self, Charge::Refused { .. })
    }
}

/// The percentages ORG-31 alerts at.
pub const ALERT_THRESHOLDS: [u8; 3] = [50, 80, 100];

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
pub struct UsageKey {
    pub surface: Surface,
}

/// One organization's credits for one billing period (ORG-31).
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct Ledger {
    /// The plan's monthly pool. `None` is an unmetered plan, under which every
    /// charge is bypassed exactly as BYOK is (ORG-32).
    pub pool: Option<u64>,
    /// Rolled over from the previous period.
    pub carried: u64,
    pub topped_up: u64,
    /// ORG-31's overage toggle. Off by default: an unexpected bill is worse
    /// than a stopped feature, which is the same call ORG-33 makes for Free.
    pub overage_enabled: bool,
    pub overage_spent: u64,
    spent_by_surface: BTreeMap<Surface, u64>,
    spent_by_project: BTreeMap<String, u64>,
    fired: BTreeSet<u8>,
}

impl Ledger {
    pub fn new(pool: Option<u64>) -> Self {
        Self {
            pool,
            ..Self::default()
        }
    }

    pub fn with_carried(mut self, carried: u64) -> Self {
        self.carried = carried;
        self
    }

    pub fn with_overage(mut self, enabled: bool) -> Self {
        self.overage_enabled = enabled;
        self
    }

    /// Everything spendable this period: the pool, what rolled over, and any
    /// top-up (ORG-31).
    pub fn granted(&self) -> Option<u64> {
        self.pool
            .map(|pool| pool.saturating_add(self.carried).saturating_add(self.topped_up))
    }

    pub fn spent(&self) -> u64 {
        self.spent_by_surface.values().copied().sum()
    }

    /// `None` on an unmetered plan.
    pub fn balance(&self) -> Option<u64> {
        self.granted().map(|g| g.saturating_sub(self.spent()))
    }

    pub fn top_up(&mut self, credits: u64) {
        self.topped_up = self.topped_up.saturating_add(credits);
    }

    pub fn charge(&mut self, spend: Spend, project: &str, access: Access) -> Charge {
        if access == Access::Byok {
            return Charge::Bypassed;
        }
        let Some(granted) = self.granted() else {
            return Charge::Bypassed;
        };
        let credits = spend.credits();
        let spent = self.spent();
        if spent.saturating_add(credits) > granted {
            if !self.overage_enabled {
                return Charge::Refused {
                    needed: credits,
                    balance: granted.saturating_sub(spent),
                };
            }
            self.record(spend.surface(), project, credits);
            self.overage_spent = self.overage_spent.saturating_add(credits);
            return Charge::Overage {
                credits,
                overage_total: self.overage_spent,
            };
        }
        self.record(spend.surface(), project, credits);
        Charge::Charged {
            credits,
            balance: granted.saturating_sub(self.spent()),
        }
    }

    fn record(&mut self, surface: Surface, project: &str, credits: u64) {
        *self.spent_by_surface.entry(surface).or_default() += credits;
        *self.spent_by_project.entry(project.to_owned()).or_default() += credits;
    }

    /// ORG-31's usage dashboard, by feature.
    pub fn by_surface(&self) -> impl Iterator<Item = (Surface, u64)> + '_ {
        self.spent_by_surface.iter().map(|(s, c)| (*s, *c))
    }

    /// ORG-31's usage dashboard, by project.
    pub fn by_project(&self) -> impl Iterator<Item = (&str, u64)> + '_ {
        self.spent_by_project
            .iter()
            .map(|(p, c)| (p.as_str(), *c))
    }

    /// How much of the pool is gone, as a percentage. `None` on an unmetered
    /// plan, where the question has no answer rather than the answer zero.
    pub fn percent_used(&self) -> Option<u8> {
        let granted = self.granted()?;
        if granted == 0 {
            return Some(100);
        }
        Some(u8::try_from((self.spent().saturating_mul(100) / granted).min(100)).unwrap_or(100))
    }

    /// Thresholds crossed since the last call. Each fires once per period, so
    /// a hundred charges past eighty percent send one alert, not a hundred.
    pub fn alerts(&mut self) -> Vec<u8> {
        let Some(percent) = self.percent_used() else {
            return Vec::new();
        };
        let mut fired = Vec::new();
        for threshold in ALERT_THRESHOLDS {
            if percent >= threshold && self.fired.insert(threshold) {
                fired.push(threshold);
            }
        }
        fired
    }

    /// ORG-31: "50% rollover up to 1.5 times the monthly pool". Half of what
    /// was not spent carries, and the carry is capped at half the pool so the
    /// next period's balance never exceeds one and a half pools
    /// (TODO(rfc-2801): the sentence also reads as a cap on the carry itself).
    pub fn rollover(&self) -> u64 {
        let Some(pool) = self.pool else {
            return 0;
        };
        let unused = self.balance().unwrap_or(0);
        (unused / 2).min(pool / 2)
    }

    /// The next period, with the rollover applied and the alerts rearmed.
    /// Top-ups do not carry: they were bought for a period.
    pub fn next_period(&self) -> Ledger {
        Ledger {
            pool: self.pool,
            carried: self.rollover(),
            topped_up: 0,
            overage_enabled: self.overage_enabled,
            overage_spent: 0,
            spent_by_surface: BTreeMap::new(),
            spent_by_project: BTreeMap::new(),
            fired: BTreeSet::new(),
        }
    }
}

/// An agent run reconciled against the estimate it was priced from (ORG-33).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub struct Reconciliation {
    pub quoted: TaskSize,
    pub actual_tokens: u64,
    pub actual: TaskSize,
    /// Credits the run should have cost, minus what it was quoted. Negative is
    /// a refund.
    pub credit_delta: i64,
}

/// ORG-33: the credit range "maps to token estimates published in the docs and
/// reconciled to actuals per run".
pub fn reconcile(quoted: TaskSize, actual_tokens: u64) -> Reconciliation {
    let actual = TaskSize::for_tokens(actual_tokens);
    Reconciliation {
        quoted,
        actual_tokens,
        actual,
        credit_delta: actual.credits() as i64 - quoted.credits() as i64,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_published_table_runs_from_five_to_three_hundred_and_twenty() {
        // ORG-31 names both ends, so both ends are asserted rather than the
        // shape of the table.
        assert_eq!(TaskSize::Minimal.credits(), 5);
        assert_eq!(TaskSize::Maximum.credits(), 320);
        let mut previous = 0;
        for size in TaskSize::ALL {
            assert!(
                size.credits() > previous,
                "{} is not larger than the size below it",
                size.as_str()
            );
            previous = size.credits();
        }
    }

    #[test]
    fn an_assistant_answer_costs_exactly_one_credit() {
        let mut ledger = Ledger::new(Some(100));
        assert_eq!(
            ledger.charge(Spend::AssistantAnswer, "docs", Access::Managed),
            Charge::Charged {
                credits: 1,
                balance: 99
            }
        );
    }

    #[test]
    fn byok_spends_nothing_at_all() {
        // ORG-31's last clause. A BYOK organization with an empty pool still
        // gets its answer.
        let mut ledger = Ledger::new(Some(0));
        let charge = ledger.charge(
            Spend::AgentTask {
                size: TaskSize::Maximum,
            },
            "docs",
            Access::Byok,
        );
        assert_eq!(charge, Charge::Bypassed);
        assert!(charge.ran());
        assert_eq!(ledger.spent(), 0);
    }

    #[test]
    fn an_unmetered_plan_bypasses_credits_the_same_way_byok_does() {
        // ORG-32: an OSS instance has no pool, and that is not "a pool of
        // zero" — a pool of zero would refuse every request.
        let mut ledger = Ledger::new(None);
        assert_eq!(
            ledger.charge(Spend::AssistantAnswer, "docs", Access::Managed),
            Charge::Bypassed
        );
        assert_eq!(ledger.balance(), None);
        assert_eq!(ledger.percent_used(), None);
    }

    #[test]
    fn a_pool_that_will_not_cover_the_task_refuses_unless_overage_is_on() {
        let mut ledger = Ledger::new(Some(10));
        let refused = ledger.charge(
            Spend::AgentTask {
                size: TaskSize::Small,
            },
            "docs",
            Access::Managed,
        );
        assert_eq!(
            refused,
            Charge::Refused {
                needed: 15,
                balance: 10
            }
        );
        assert!(!refused.ran());
        assert_eq!(ledger.spent(), 0, "a refused task spends nothing");

        let mut ledger = Ledger::new(Some(10)).with_overage(true);
        assert_eq!(
            ledger.charge(
                Spend::AgentTask {
                    size: TaskSize::Small
                },
                "docs",
                Access::Managed
            ),
            Charge::Overage {
                credits: 15,
                overage_total: 15
            }
        );
    }

    #[test]
    fn a_top_up_is_spendable_immediately() {
        let mut ledger = Ledger::new(Some(10));
        ledger.top_up(50);
        assert_eq!(ledger.granted(), Some(60));
        assert!(matches!(
            ledger.charge(
                Spend::AgentTask {
                    size: TaskSize::Small
                },
                "docs",
                Access::Managed
            ),
            Charge::Charged { credits: 15, .. }
        ));
    }

    #[test]
    fn each_alert_threshold_fires_once_per_period() {
        let mut ledger = Ledger::new(Some(100));
        for _ in 0..50 {
            ledger.charge(Spend::AssistantAnswer, "docs", Access::Managed);
        }
        assert_eq!(ledger.alerts(), vec![50]);
        assert_eq!(ledger.alerts(), Vec::<u8>::new(), "fifty already fired");
        for _ in 0..31 {
            ledger.charge(Spend::AssistantAnswer, "docs", Access::Managed);
        }
        assert_eq!(ledger.alerts(), vec![80]);
        for _ in 0..19 {
            ledger.charge(Spend::AssistantAnswer, "docs", Access::Managed);
        }
        assert_eq!(ledger.alerts(), vec![100]);
    }

    #[test]
    fn rollover_is_half_of_what_is_left_and_never_takes_the_balance_past_one_and_a_half_pools() {
        let ledger = Ledger::new(Some(1_000));
        assert_eq!(ledger.rollover(), 500, "nothing spent: half the pool carries");
        let next = ledger.next_period();
        assert_eq!(next.granted(), Some(1_500));

        let mut ledger = Ledger::new(Some(1_000));
        for _ in 0..600 {
            ledger.charge(Spend::AssistantAnswer, "docs", Access::Managed);
        }
        assert_eq!(ledger.rollover(), 200, "half of the four hundred left");
        assert_eq!(ledger.next_period().granted(), Some(1_200));
    }

    #[test]
    fn a_top_up_does_not_roll_over_and_the_alerts_rearm() {
        let mut ledger = Ledger::new(Some(100));
        ledger.top_up(1_000);
        for _ in 0..60 {
            ledger.charge(Spend::AssistantAnswer, "docs", Access::Managed);
        }
        assert!(!ledger.alerts().is_empty());
        let next = ledger.next_period();
        assert_eq!(next.topped_up, 0);
        assert_eq!(next.spent(), 0);
        let mut next = next;
        assert_eq!(next.alerts(), Vec::<u8>::new(), "nothing spent yet");
    }

    #[test]
    fn usage_is_reported_by_feature_and_by_project() {
        // ORG-31's dashboard is "by feature and by project", which is two
        // breakdowns of the same spend rather than one.
        let mut ledger = Ledger::new(Some(1_000));
        ledger.charge(Spend::AssistantAnswer, "docs", Access::Managed);
        ledger.charge(Spend::AssistantAnswer, "api", Access::Managed);
        ledger.charge(
            Spend::AgentTask {
                size: TaskSize::Medium,
            },
            "docs",
            Access::Managed,
        );
        let by_surface: Vec<_> = ledger.by_surface().collect();
        assert_eq!(
            by_surface,
            vec![(Surface::Assistant, 2), (Surface::Agent, 40)]
        );
        let by_project: Vec<_> = ledger.by_project().collect();
        assert_eq!(by_project, vec![("api", 1), ("docs", 41)]);
        assert_eq!(
            by_surface.iter().map(|(_, c)| c).sum::<u64>(),
            by_project.iter().map(|(_, c)| c).sum::<u64>(),
            "the two breakdowns are of the same spend"
        );
    }

    #[test]
    fn a_run_larger_than_its_quote_is_repriced_against_the_published_estimate() {
        // ORG-33: reconciled to actuals per run.
        let over = reconcile(TaskSize::Small, TaskSize::Large.estimated_tokens());
        assert_eq!(over.actual, TaskSize::Large);
        assert_eq!(over.credit_delta, 85);

        let under = reconcile(TaskSize::Large, 1);
        assert_eq!(under.actual, TaskSize::Minimal);
        assert_eq!(under.credit_delta, -95);

        let exact = reconcile(TaskSize::Medium, TaskSize::Medium.estimated_tokens());
        assert_eq!(exact.credit_delta, 0);

        let huge = reconcile(TaskSize::Maximum, u64::MAX);
        assert_eq!(huge.actual, TaskSize::Maximum, "the table has a top");
    }

    #[test]
    fn a_credit_is_priced_from_list_times_the_published_margin() {
        // ORG-33. 2000 micros per 1k tokens, 2500 tokens per credit, is 5000
        // micros of list; a forty percent margin makes it 7000.
        let pricing = Pricing {
            list_micros_per_1k_tokens: 2_000,
            margin_percent: 40,
            period: "2026-09",
        };
        assert_eq!(pricing.micros_per_credit(), 7_000);
        assert_eq!(
            Pricing {
                margin_percent: 0,
                ..pricing
            }
            .micros_per_credit(),
            5_000
        );
    }
}
