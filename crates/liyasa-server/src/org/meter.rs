//! What has been consumed, what it cost, and whether a metered feature is
//! paused (ORG-33, HOST-12).
//!
//! The pause is sticky for the period. ORG-33 says overage on Free "pauses the
//! metered feature rather than billing", and a pause that lifted as soon as
//! the next request asked for fewer units would not be a pause — it would be a
//! per-request refusal that a small enough request slips past.

use std::collections::{BTreeMap, BTreeSet};

use liyasa_core::diagnostics::{Code, Diagnostic};
use serde::{Deserialize, Serialize};

use super::plan::{Decision, Plan, Resource};

/// Consumption for one period, broken down the two ways ORG-33 asks for: by
/// resource, and by project so the operator team can see cost per project.
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct Usage {
    by_resource: BTreeMap<Resource, u64>,
    by_project: BTreeMap<(String, Resource), u64>,
}

impl Usage {
    pub fn record(&mut self, project: &str, resource: Resource, units: u64) {
        *self.by_resource.entry(resource).or_default() += units;
        *self
            .by_project
            .entry((project.to_owned(), resource))
            .or_default() += units;
    }

    pub fn used(&self, resource: Resource) -> u64 {
        self.by_resource.get(&resource).copied().unwrap_or(0)
    }

    pub fn used_by(&self, project: &str, resource: Resource) -> u64 {
        self.by_project
            .get(&(project.to_owned(), resource))
            .copied()
            .unwrap_or(0)
    }

    pub fn resources(&self) -> impl Iterator<Item = (Resource, u64)> + '_ {
        self.by_resource.iter().map(|(r, u)| (*r, *u))
    }

    pub fn projects(&self) -> BTreeSet<&str> {
        self.by_project.keys().map(|(p, _)| p.as_str()).collect()
    }

    /// ORG-33's cost basis, applied. Micro-dollars.
    pub fn cost_micros(&self) -> u64 {
        self.by_resource
            .iter()
            .map(|(resource, units)| resource.cost_basis().micros_for(*units))
            .fold(0, u64::saturating_add)
    }

    /// ORG-33: "The cloud dashboard shows cost per project to the operator
    /// team."
    pub fn cost_micros_for(&self, project: &str) -> u64 {
        self.by_project
            .iter()
            .filter(|((p, _), _)| p == project)
            .map(|((_, resource), units)| resource.cost_basis().micros_for(*units))
            .fold(0, u64::saturating_add)
    }
}

/// The plan policy of ORG-32 applied to real consumption.
#[derive(Debug, Clone, Default)]
pub struct Meter {
    pub plan: Plan,
    pub usage: Usage,
    paused: BTreeSet<Resource>,
}

impl Meter {
    pub fn new(plan: Plan) -> Self {
        Self {
            plan,
            usage: Usage::default(),
            paused: BTreeSet::new(),
        }
    }

    pub fn is_paused(&self, resource: Resource) -> bool {
        self.paused.contains(&resource)
    }

    pub fn paused(&self) -> impl Iterator<Item = Resource> + '_ {
        self.paused.iter().copied()
    }

    /// Asks for `units` of `resource` and records them if the answer is yes.
    /// A paused resource stays paused until [`Meter::new_period`].
    pub fn request(&mut self, project: &str, resource: Resource, units: u64) -> Decision {
        if self.is_paused(resource) {
            let limit = self.plan.quota(resource).unwrap_or(0);
            return Decision::Paused {
                resource,
                limit,
                used: self.usage.used(resource),
            };
        }
        let decision = self.plan.check(resource, self.usage.used(resource), units);
        match &decision {
            Decision::Paused { .. } => {
                self.paused.insert(resource);
            }
            Decision::Allowed { .. } | Decision::Billed { .. } => {
                self.usage.record(project, resource, units);
            }
        }
        decision
    }

    /// A new billing period: consumption resets and every pause lifts.
    pub fn new_period(&self) -> Meter {
        Meter::new(self.plan.clone())
    }
}

/// The diagnostic a paused resource raises, for a caller that needs to say why
/// rather than just refuse.
pub fn paused_diagnostic(resource: Resource, limit: u64) -> Diagnostic {
    let code = Code::new("E0850").expect("E0850 is registered");
    Diagnostic::new(
        code,
        format!(
            "this plan's quota of {limit} {} is spent, so `{}` is paused for the rest of the period",
            resource.cost_basis().unit,
            resource.as_str()
        ),
    )
    .help("upgrade the plan, or wait for the quota to reset at the start of the next period")
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::org::plan::Tier;

    #[test]
    fn a_paused_resource_stays_paused_for_the_period() {
        // The failure this is about: a one-minute request slipping through
        // after a two-hundred-minute request was refused would mean the
        // feature was never paused, only that one call was.
        let mut meter = Meter::new(Plan::free());
        assert!(matches!(
            meter.request("docs", Resource::SandboxMinutes, 201),
            Decision::Paused { .. }
        ));
        assert!(meter.is_paused(Resource::SandboxMinutes));
        assert!(matches!(
            meter.request("docs", Resource::SandboxMinutes, 1),
            Decision::Paused { .. }
        ));
        assert_eq!(
            meter.usage.used(Resource::SandboxMinutes),
            0,
            "a paused request consumes nothing"
        );

        // Only the resource that ran out is paused.
        assert!(matches!(
            meter.request("docs", Resource::RenderJobs, 1),
            Decision::Allowed { .. }
        ));

        let fresh = meter.new_period();
        assert!(!fresh.is_paused(Resource::SandboxMinutes));
        assert_eq!(fresh.usage.used(Resource::SandboxMinutes), 0);
    }

    #[test]
    fn a_billed_plan_records_what_it_went_over_by() {
        let mut meter = Meter::new(Plan::pro());
        assert!(matches!(
            meter.request("docs", Resource::SandboxMinutes, 3_010),
            Decision::Billed { over_by: 10, .. }
        ));
        assert!(!meter.is_paused(Resource::SandboxMinutes));
        assert_eq!(meter.usage.used(Resource::SandboxMinutes), 3_010);
    }

    #[test]
    fn cost_is_reported_per_project_and_in_total() {
        let mut meter = Meter::new(Plan::of(Tier::Unlimited));
        meter.request("docs", Resource::SandboxMinutes, 10);
        meter.request("api", Resource::SandboxMinutes, 30);
        meter.request("api", Resource::RenderJobs, 100);

        // 1500 micros a minute, 500 a job.
        assert_eq!(meter.usage.cost_micros_for("docs"), 15_000);
        assert_eq!(meter.usage.cost_micros_for("api"), 45_000 + 50_000);
        assert_eq!(meter.usage.cost_micros(), 15_000 + 95_000);
        assert_eq!(meter.usage.cost_micros_for("absent"), 0);
        assert_eq!(
            meter.usage.projects().into_iter().collect::<Vec<_>>(),
            vec!["api", "docs"]
        );
    }

    #[test]
    fn an_unlimited_plan_meters_without_refusing() {
        // ORG-32: an OSS instance still counts, because the operator may want
        // to know; it is never told no.
        let mut meter = Meter::new(Plan::unlimited());
        assert_eq!(
            meter.request("docs", Resource::SandboxMinutes, 100_000),
            Decision::Allowed { remaining: None }
        );
        assert_eq!(meter.usage.used(Resource::SandboxMinutes), 100_000);
        assert_eq!(meter.paused().count(), 0);
    }

    #[test]
    fn the_pause_says_what_ran_out_and_what_to_do() {
        let diagnostic = paused_diagnostic(Resource::SandboxMinutes, 200);
        assert_eq!(diagnostic.code.as_str(), "E0850");
        assert!(diagnostic.message.contains("200"), "{}", diagnostic.message);
        assert!(diagnostic.help.is_some());
    }
}
