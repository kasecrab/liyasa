//! Tiers, their quotas, and the policy layer that answers "may this happen"
//! (ORG-30, ORG-32, ORG-33).
//!
//! ORG-32 is the reason this is a table rather than a set of `if cloud` tests
//! scattered through the handlers: an OSS instance runs [`Plan::unlimited`]
//! and takes exactly the same code path a paying cloud project takes. There is
//! no branch anywhere that asks which edition this is.
//!
//! Every metered resource carries a cost basis beside its quota (ORG-33), so
//! "what does a free project cost us" is a question the type can answer rather
//! than a spreadsheet nobody committed.

use std::collections::BTreeSet;

use serde::{Deserialize, Serialize};

/// A resource that is metered. Adding one here forces a quota on every tier
/// and a cost basis, which is what ORG-33 asks for in words.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum Resource {
    Projects,
    Seats,
    /// Thousands of model tokens, through managed access. BYOK does not meter
    /// here (ORG-31); it does still meter [`Resource::SandboxMinutes`].
    ModelTokens,
    SandboxMinutes,
    /// PDF, Mermaid and screenshot jobs.
    RenderJobs,
    /// Gigabyte-months.
    StorageBytes,
    /// Gigabytes served.
    BandwidthBytes,
    PreviewDeployments,
}

impl Resource {
    pub const ALL: &'static [Resource] = &[
        Resource::Projects,
        Resource::Seats,
        Resource::ModelTokens,
        Resource::SandboxMinutes,
        Resource::RenderJobs,
        Resource::StorageBytes,
        Resource::BandwidthBytes,
        Resource::PreviewDeployments,
    ];

    pub fn as_str(self) -> &'static str {
        match self {
            Resource::Projects => "projects",
            Resource::Seats => "seats",
            Resource::ModelTokens => "modelTokens",
            Resource::SandboxMinutes => "sandboxMinutes",
            Resource::RenderJobs => "renderJobs",
            Resource::StorageBytes => "storageBytes",
            Resource::BandwidthBytes => "bandwidthBytes",
            Resource::PreviewDeployments => "previewDeployments",
        }
    }

    pub fn parse(text: &str) -> Option<Resource> {
        Resource::ALL
            .iter()
            .copied()
            .find(|r| r.as_str().eq_ignore_ascii_case(text))
    }

    /// What one unit is, and what it costs the operator in micro-dollars.
    /// `Projects` and `Seats` are not metered consumption and cost nothing by
    /// themselves; what they gate costs.
    pub fn cost_basis(self) -> CostBasis {
        match self {
            Resource::Projects => CostBasis::new("project", 0),
            Resource::Seats => CostBasis::new("seat", 0),
            Resource::ModelTokens => CostBasis::new("1k tokens", 2_000),
            Resource::SandboxMinutes => CostBasis::new("minute", 1_500),
            Resource::RenderJobs => CostBasis::new("job", 500),
            Resource::StorageBytes => CostBasis::new("GB-month", 20_000),
            Resource::BandwidthBytes => CostBasis::new("GB", 8_000),
            Resource::PreviewDeployments => CostBasis::new("preview", 2_000),
        }
    }
}

/// The published cost of one unit of a resource, in micro-dollars (ORG-33).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub struct CostBasis {
    pub unit: &'static str,
    pub micros_per_unit: u64,
}

impl CostBasis {
    const fn new(unit: &'static str, micros_per_unit: u64) -> Self {
        Self {
            unit,
            micros_per_unit,
        }
    }

    pub fn micros_for(&self, units: u64) -> u64 {
        self.micros_per_unit.saturating_mul(units)
    }
}

/// Something a plan either has or does not, as opposed to something it has a
/// quantity of (ORG-30).
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum Feature {
    CustomDomains,
    Previews,
    /// Free carries Liyasa branding; turning it off is a paid feature.
    RemoveBranding,
    Sso,
    Scim,
    AuditStreaming,
    StaticEgress,
    Sla,
    Invoicing,
}

impl Feature {
    pub const ALL: &'static [Feature] = &[
        Feature::CustomDomains,
        Feature::Previews,
        Feature::RemoveBranding,
        Feature::Sso,
        Feature::Scim,
        Feature::AuditStreaming,
        Feature::StaticEgress,
        Feature::Sla,
        Feature::Invoicing,
    ];

    pub fn as_str(self) -> &'static str {
        match self {
            Feature::CustomDomains => "customDomains",
            Feature::Previews => "previews",
            Feature::RemoveBranding => "removeBranding",
            Feature::Sso => "sso",
            Feature::Scim => "scim",
            Feature::AuditStreaming => "auditStreaming",
            Feature::StaticEgress => "staticEgress",
            Feature::Sla => "sla",
            Feature::Invoicing => "invoicing",
        }
    }

    pub fn parse(text: &str) -> Option<Feature> {
        Feature::ALL
            .iter()
            .copied()
            .find(|f| f.as_str().eq_ignore_ascii_case(text))
    }
}

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum Tier {
    Free,
    Pro,
    Enterprise,
    /// What an OSS instance runs, and the default (ORG-32).
    #[default]
    Unlimited,
}

impl Tier {
    pub const ALL: &'static [Tier] = &[Tier::Free, Tier::Pro, Tier::Enterprise, Tier::Unlimited];

    pub fn as_str(self) -> &'static str {
        match self {
            Tier::Free => "free",
            Tier::Pro => "pro",
            Tier::Enterprise => "enterprise",
            Tier::Unlimited => "unlimited",
        }
    }

    pub fn parse(text: &str) -> Option<Tier> {
        Tier::ALL
            .iter()
            .copied()
            .find(|t| t.as_str().eq_ignore_ascii_case(text))
    }
}

/// What happens when a tier runs past a quota.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum Overage {
    /// ORG-33: overage on Free pauses the metered feature rather than billing.
    Pause,
    Bill,
}

/// One tier's whole policy. Construct through [`Plan::of`] rather than by
/// hand, so no code path can invent a tier with quotas nobody published.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Plan {
    pub tier: Tier,
    quotas: Vec<(Resource, Option<u64>)>,
    features: BTreeSet<Feature>,
    pub overage: Overage,
    /// ORG-30 gives Pro thirteen months. `None` is "kept as long as the
    /// instance keeps it", which is what an OSS operator's own disk means.
    pub analytics_retention_days: Option<u32>,
    /// ORG-31's monthly pool. `None` is unmetered assistant access.
    pub monthly_credits: Option<u64>,
    /// How long a preview deployment lives before it is collected (ORG-33).
    pub preview_lifetime_days: Option<u32>,
}

/// The published ceiling on what one Free project is expected to cost in a
/// month, in micro-dollars (ORG-33). The Free quotas are set against this, and
/// [`Plan::free`]'s expected cost is asserted below it by a test rather than
/// by an argument in a document.
pub const FREE_MONTHLY_COST_CEILING_MICROS: u64 = 1_500_000;

impl Plan {
    pub fn of(tier: Tier) -> Plan {
        match tier {
            Tier::Free => Plan::free(),
            Tier::Pro => Plan::pro(),
            Tier::Enterprise => Plan::enterprise(),
            Tier::Unlimited => Plan::unlimited(),
        }
    }

    /// ORG-30: one project, the Liyasa subdomain, community support, branding
    /// on. ORG-33: every cap is chosen so the expected monthly cost stays
    /// under [`FREE_MONTHLY_COST_CEILING_MICROS`], and running past one pauses
    /// the feature instead of billing.
    pub fn free() -> Plan {
        Plan {
            tier: Tier::Free,
            quotas: vec![
                (Resource::Projects, Some(1)),
                (Resource::Seats, Some(3)),
                (Resource::ModelTokens, Some(250)),
                (Resource::SandboxMinutes, Some(200)),
                (Resource::RenderJobs, Some(200)),
                (Resource::StorageBytes, Some(2)),
                (Resource::BandwidthBytes, Some(20)),
                (Resource::PreviewDeployments, Some(0)),
            ],
            features: BTreeSet::new(),
            overage: Overage::Pause,
            analytics_retention_days: Some(90),
            monthly_credits: Some(50),
            preview_lifetime_days: None,
        }
    }

    /// ORG-30: per seat, custom domains, previews, analytics retention of
    /// thirteen months.
    pub fn pro() -> Plan {
        Plan {
            tier: Tier::Pro,
            quotas: vec![
                (Resource::Projects, None),
                (Resource::Seats, None),
                (Resource::ModelTokens, Some(5_000)),
                (Resource::SandboxMinutes, Some(3_000)),
                (Resource::RenderJobs, Some(10_000)),
                (Resource::StorageBytes, Some(100)),
                (Resource::BandwidthBytes, Some(1_000)),
                (Resource::PreviewDeployments, Some(50)),
            ],
            features: [
                Feature::CustomDomains,
                Feature::Previews,
                Feature::RemoveBranding,
            ]
            .into_iter()
            .collect(),
            overage: Overage::Bill,
            analytics_retention_days: Some(THIRTEEN_MONTHS_IN_DAYS),
            monthly_credits: Some(2_000),
            preview_lifetime_days: Some(14),
        }
    }

    /// ORG-30: SSO, SCIM, audit streaming, static egress, SLAs, invoicing.
    pub fn enterprise() -> Plan {
        Plan {
            tier: Tier::Enterprise,
            quotas: Resource::ALL.iter().map(|r| (*r, None)).collect(),
            features: Feature::ALL.iter().copied().collect(),
            overage: Overage::Bill,
            analytics_retention_days: Some(THIRTEEN_MONTHS_IN_DAYS),
            monthly_credits: Some(20_000),
            preview_lifetime_days: Some(30),
        }
    }

    /// ORG-32: what an OSS instance runs, and the default. No quota, every
    /// feature, and no credit pool — an operator running their own server is
    /// not metered by us for using it.
    pub fn unlimited() -> Plan {
        Plan {
            tier: Tier::Unlimited,
            quotas: Resource::ALL.iter().map(|r| (*r, None)).collect(),
            features: Feature::ALL.iter().copied().collect(),
            overage: Overage::Bill,
            analytics_retention_days: None,
            monthly_credits: None,
            preview_lifetime_days: None,
        }
    }

    /// `None` is unlimited.
    pub fn quota(&self, resource: Resource) -> Option<u64> {
        self.quotas
            .iter()
            .find(|(r, _)| *r == resource)
            .and_then(|(_, limit)| *limit)
    }

    pub fn allows(&self, feature: Feature) -> bool {
        self.features.contains(&feature)
    }

    pub fn features(&self) -> impl Iterator<Item = Feature> + '_ {
        self.features.iter().copied()
    }

    pub fn quotas(&self) -> impl Iterator<Item = (Resource, Option<u64>)> + '_ {
        self.quotas.iter().copied()
    }

    /// The policy layer ORG-32 names, in one function. `used` is what this
    /// period has already consumed and `units` is what is being asked for.
    pub fn check(&self, resource: Resource, used: u64, units: u64) -> Decision {
        let Some(limit) = self.quota(resource) else {
            return Decision::Allowed { remaining: None };
        };
        let after = used.saturating_add(units);
        if after <= limit {
            return Decision::Allowed {
                remaining: Some(limit - after),
            };
        }
        let over_by = after - limit;
        match self.overage {
            Overage::Pause => Decision::Paused {
                resource,
                limit,
                used,
            },
            Overage::Bill => Decision::Billed { resource, over_by },
        }
    }

    /// What one project on this plan is expected to cost in a month if it
    /// consumes every quota (ORG-33). Unlimited quotas contribute nothing
    /// because there is no cap to multiply by, which is why this is only
    /// meaningful for a capped tier.
    pub fn expected_monthly_cost_micros(&self) -> u64 {
        self.quotas
            .iter()
            .filter_map(|(resource, limit)| {
                limit.map(|units| resource.cost_basis().micros_for(units))
            })
            .fold(0u64, |total, micros| total.saturating_add(micros))
    }
}

/// ORG-30's retention for Pro, as days. Thirteen months of thirty-and-a-bit
/// days each; the number is the contract, so it is written once.
pub const THIRTEEN_MONTHS_IN_DAYS: u32 = 395;

impl Default for Plan {
    fn default() -> Self {
        Plan::unlimited()
    }
}

/// What the policy layer decided.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "decision", rename_all = "camelCase")]
pub enum Decision {
    Allowed {
        /// `None` when the resource is unlimited on this plan.
        remaining: Option<u64>,
    },
    /// ORG-33: Free pauses the metered feature rather than billing for it.
    Paused {
        resource: Resource,
        limit: u64,
        used: u64,
    },
    Billed {
        resource: Resource,
        over_by: u64,
    },
}

impl Decision {
    pub fn is_allowed(&self) -> bool {
        !matches!(self, Decision::Paused { .. })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn an_oss_instance_is_unlimited_without_being_asked() {
        // ORG-32: the default is the policy an operator's own server runs
        // under, so no handler needs to know which edition it is in.
        let plan = Plan::default();
        assert_eq!(plan.tier, Tier::Unlimited);
        for resource in Resource::ALL {
            assert_eq!(plan.quota(*resource), None, "{}", resource.as_str());
            assert_eq!(
                plan.check(*resource, u64::MAX, u64::MAX),
                Decision::Allowed { remaining: None }
            );
        }
        for feature in Feature::ALL {
            assert!(plan.allows(*feature), "{}", feature.as_str());
        }
    }

    #[test]
    fn free_pauses_a_metered_feature_and_pro_bills_for_it() {
        // ORG-33's last clause, which is the difference between a surprise
        // invoice and a stopped feature.
        let free = Plan::free();
        assert_eq!(
            free.check(Resource::SandboxMinutes, 199, 5),
            Decision::Paused {
                resource: Resource::SandboxMinutes,
                limit: 200,
                used: 199,
            }
        );
        assert!(!free.check(Resource::SandboxMinutes, 199, 5).is_allowed());

        let pro = Plan::pro();
        assert_eq!(
            pro.check(Resource::SandboxMinutes, 2_999, 5),
            Decision::Billed {
                resource: Resource::SandboxMinutes,
                over_by: 4,
            }
        );
        assert!(pro.check(Resource::SandboxMinutes, 2_999, 5).is_allowed());
    }

    #[test]
    fn a_request_that_fits_reports_what_is_left() {
        assert_eq!(
            Plan::free().check(Resource::RenderJobs, 50, 10),
            Decision::Allowed {
                remaining: Some(140)
            }
        );
    }

    #[test]
    fn byok_does_not_exempt_sandbox_minutes() {
        // Named in ORG-33 in terms. Sandbox minutes are the operator's
        // machines whoever paid for the model.
        assert!(Plan::free().quota(Resource::SandboxMinutes).is_some());
        assert_eq!(Plan::free().quota(Resource::SandboxMinutes), Some(200));
        assert_eq!(Plan::pro().quota(Resource::SandboxMinutes), Some(3_000));
    }

    #[test]
    fn a_free_project_costs_less_than_the_published_ceiling() {
        // ORG-33: the caps exist to hold this, so raising one without raising
        // the ceiling has to fail here rather than on an invoice.
        let cost = Plan::free().expected_monthly_cost_micros();
        assert!(
            cost <= FREE_MONTHLY_COST_CEILING_MICROS,
            "a fully consumed free project costs {cost} micros against a ceiling of \
             {FREE_MONTHLY_COST_CEILING_MICROS}"
        );
        assert!(cost > 0, "a ceiling nothing counts against proves nothing");
    }

    #[test]
    fn every_resource_has_a_cost_basis_and_a_quota_on_every_capped_tier() {
        // ORG-33 says "every metered resource has a cost basis and a plan
        // quota"; a resource added without either would otherwise be free and
        // unbounded by omission.
        for resource in Resource::ALL {
            let basis = resource.cost_basis();
            assert!(!basis.unit.is_empty(), "{}", resource.as_str());
            for tier in [Tier::Free, Tier::Pro] {
                let plan = Plan::of(tier);
                assert!(
                    plan.quotas().any(|(r, _)| r == *resource),
                    "{} has no row on {}",
                    resource.as_str(),
                    tier.as_str()
                );
            }
        }
    }

    #[test]
    fn the_tiers_carry_the_features_org_30_names() {
        let free = Plan::free();
        assert!(!free.allows(Feature::CustomDomains));
        assert!(!free.allows(Feature::RemoveBranding), "branding is on");
        assert_eq!(free.quota(Resource::Projects), Some(1));

        let pro = Plan::pro();
        assert!(pro.allows(Feature::CustomDomains));
        assert!(pro.allows(Feature::Previews));
        assert_eq!(pro.analytics_retention_days, Some(THIRTEEN_MONTHS_IN_DAYS));
        assert!(!pro.allows(Feature::Sso), "SSO is an Enterprise feature");

        let enterprise = Plan::enterprise();
        for feature in [
            Feature::Sso,
            Feature::Scim,
            Feature::AuditStreaming,
            Feature::StaticEgress,
            Feature::Sla,
            Feature::Invoicing,
        ] {
            assert!(enterprise.allows(feature), "{}", feature.as_str());
        }
    }

    #[test]
    fn a_tier_round_trips_through_its_name() {
        for tier in Tier::ALL {
            assert_eq!(Tier::parse(tier.as_str()), Some(*tier));
        }
        assert_eq!(Tier::parse("Pro"), Some(Tier::Pro));
        assert_eq!(Tier::parse("platinum"), None);
        for resource in Resource::ALL {
            assert_eq!(Resource::parse(resource.as_str()), Some(*resource));
        }
        for feature in Feature::ALL {
            assert_eq!(Feature::parse(feature.as_str()), Some(*feature));
        }
    }
}
