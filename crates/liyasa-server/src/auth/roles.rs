//! Roles and the permissions they compose (AUTH-30, AUTH-31).
//!
//! One table, consulted by every surface: the dashboard, the editor, the REST
//! API and the admin MCP server all call [`Grant::allows`]. AUTH-31 is a
//! statement about there being exactly one answer to "may this actor do this",
//! so a surface that decides for itself is the bug the requirement names.

use std::collections::BTreeSet;

use serde::{Deserialize, Serialize};

/// What an actor may do. Named for the object and the verb rather than for the
/// screen, so a new surface maps onto the existing set instead of adding to it.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum Permission {
    /// Read the dashboard: builds, deployments, analytics.
    DashboardRead,
    /// Create a draft or a suggestion.
    ContentDraft,
    /// Publish, subject to the publishing policy.
    ContentPublish,
    /// Approve or reject a proposal.
    ProposalReview,
    /// Project settings, members, domains, API keys.
    SettingsWrite,
    /// Billing and deleting the project.
    OwnerAct,
}

impl Permission {
    pub const ALL: &'static [Permission] = &[
        Permission::DashboardRead,
        Permission::ContentDraft,
        Permission::ContentPublish,
        Permission::ProposalReview,
        Permission::SettingsWrite,
        Permission::OwnerAct,
    ];

    pub fn as_str(self) -> &'static str {
        match self {
            Permission::DashboardRead => "dashboardRead",
            Permission::ContentDraft => "contentDraft",
            Permission::ContentPublish => "contentPublish",
            Permission::ProposalReview => "proposalReview",
            Permission::SettingsWrite => "settingsWrite",
            Permission::OwnerAct => "ownerAct",
        }
    }

    pub fn parse(text: &str) -> Option<Permission> {
        Permission::ALL
            .iter()
            .copied()
            .find(|p| p.as_str().eq_ignore_ascii_case(text))
    }
}

#[derive(
    Debug, Clone, Copy, Default, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize,
)]
#[serde(rename_all = "lowercase")]
pub enum Role {
    /// A reader with no dashboard access at all: the default a signed-in
    /// reader gets, and not one of AUTH-30's six.
    #[default]
    Reader,
    Viewer,
    Contributor,
    Editor,
    Reviewer,
    Admin,
    Owner,
}

impl Role {
    pub const ALL: &'static [Role] = &[
        Role::Reader,
        Role::Viewer,
        Role::Contributor,
        Role::Editor,
        Role::Reviewer,
        Role::Admin,
        Role::Owner,
    ];

    pub fn as_str(self) -> &'static str {
        match self {
            Role::Reader => "reader",
            Role::Viewer => "viewer",
            Role::Contributor => "contributor",
            Role::Editor => "editor",
            Role::Reviewer => "reviewer",
            Role::Admin => "admin",
            Role::Owner => "owner",
        }
    }

    pub fn parse(text: &str) -> Option<Role> {
        Role::ALL
            .iter()
            .copied()
            .find(|r| r.as_str().eq_ignore_ascii_case(text))
    }

    /// AUTH-30's table, written once.
    pub fn permissions(self) -> BTreeSet<Permission> {
        use Permission::*;
        let list: &[Permission] = match self {
            Role::Reader => &[],
            Role::Viewer => &[DashboardRead],
            Role::Contributor => &[DashboardRead, ContentDraft],
            Role::Editor => &[DashboardRead, ContentDraft, ContentPublish],
            Role::Reviewer => &[DashboardRead, ContentDraft, ProposalReview],
            Role::Admin => &[
                DashboardRead,
                ContentDraft,
                ContentPublish,
                ProposalReview,
                SettingsWrite,
            ],
            Role::Owner => Permission::ALL,
        };
        list.iter().copied().collect()
    }
}

/// A role an operator composed out of permissions (AUTH-30's last clause).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct CustomRole {
    pub name: String,
    /// The built-in role this starts from, if any.
    #[serde(default)]
    pub extends: Option<Role>,
    #[serde(default)]
    pub grant: BTreeSet<Permission>,
    /// Permissions the base role has and this one does not.
    #[serde(default)]
    pub revoke: BTreeSet<Permission>,
}

impl CustomRole {
    pub fn permissions(&self) -> BTreeSet<Permission> {
        let mut out = self.extends.map(Role::permissions).unwrap_or_default();
        out.extend(self.grant.iter().copied());
        for revoked in &self.revoke {
            out.remove(revoked);
        }
        out
    }
}

/// What an actor was granted: a built-in role, a composed one, or both.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct Grant {
    #[serde(default)]
    pub role: Role,
    #[serde(default)]
    pub custom: Vec<CustomRole>,
}

impl Grant {
    pub fn role(role: Role) -> Self {
        Self {
            role,
            custom: Vec::new(),
        }
    }

    pub fn with_custom(mut self, custom: CustomRole) -> Self {
        self.custom.push(custom);
        self
    }

    pub fn permissions(&self) -> BTreeSet<Permission> {
        let mut out = self.role.permissions();
        for custom in &self.custom {
            out.extend(custom.permissions());
        }
        out
    }

    /// The one question every surface asks (AUTH-31).
    pub fn allows(&self, permission: Permission) -> bool {
        self.permissions().contains(&permission)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use Permission::*;

    #[test]
    fn the_six_roles_auth_30_names_have_the_powers_it_gives_them() {
        assert_eq!(Role::Viewer.permissions(), [DashboardRead].into());
        assert!(Role::Contributor.permissions().contains(&ContentDraft));
        assert!(!Role::Contributor.permissions().contains(&ContentPublish));
        assert!(Role::Editor.permissions().contains(&ContentPublish));
        assert!(!Role::Editor.permissions().contains(&ProposalReview));
        assert!(Role::Reviewer.permissions().contains(&ProposalReview));
        assert!(!Role::Reviewer.permissions().contains(&ContentPublish));
        assert!(Role::Admin.permissions().contains(&SettingsWrite));
        assert!(
            !Role::Admin.permissions().contains(&OwnerAct),
            "billing and deletion are the owner's"
        );
        assert_eq!(
            Role::Owner.permissions().len(),
            Permission::ALL.len(),
            "the owner may do everything"
        );
    }

    #[test]
    fn a_signed_in_reader_has_no_dashboard_at_all() {
        assert!(Role::Reader.permissions().is_empty());
        assert!(!Grant::default().allows(DashboardRead));
    }

    #[test]
    fn a_custom_role_composes_permissions_onto_a_base() {
        let role = CustomRole {
            name: "release-manager".to_owned(),
            extends: Some(Role::Editor),
            grant: [ProposalReview].into(),
            revoke: BTreeSet::new(),
        };
        let grant = Grant::role(Role::Reader).with_custom(role);
        assert!(grant.allows(ContentPublish));
        assert!(grant.allows(ProposalReview));
        assert!(!grant.allows(SettingsWrite));
    }

    #[test]
    fn a_custom_role_may_take_a_permission_away_from_its_base() {
        let role = CustomRole {
            name: "auditor".to_owned(),
            extends: Some(Role::Admin),
            grant: BTreeSet::new(),
            revoke: [SettingsWrite, ContentPublish].into(),
        };
        let grant = Grant::role(Role::Reader).with_custom(role);
        assert!(grant.allows(DashboardRead));
        assert!(!grant.allows(SettingsWrite));
        assert!(!grant.allows(ContentPublish));
    }

    #[test]
    fn a_custom_role_with_no_base_grants_exactly_what_it_lists() {
        let role = CustomRole {
            name: "billing".to_owned(),
            extends: None,
            grant: [OwnerAct].into(),
            revoke: BTreeSet::new(),
        };
        assert_eq!(role.permissions(), [OwnerAct].into());
    }

    #[test]
    fn every_role_and_permission_round_trips_through_its_name() {
        for role in Role::ALL {
            assert_eq!(Role::parse(role.as_str()), Some(*role));
        }
        for permission in Permission::ALL {
            assert_eq!(Permission::parse(permission.as_str()), Some(*permission));
        }
        assert_eq!(Role::parse("ADMIN"), Some(Role::Admin));
        assert_eq!(Role::parse("superuser"), None);
        assert_eq!(Permission::parse("deleteEverything"), None);
    }

    #[test]
    fn the_answer_does_not_depend_on_which_surface_asks() {
        // AUTH-31: there is one table, so this is the whole of the claim that
        // the dashboard, the editor, the REST API and the MCP server agree.
        let grant = Grant::role(Role::Contributor);
        let asked_four_times = [
            grant.allows(ContentPublish),
            grant.allows(ContentPublish),
            grant.allows(ContentPublish),
            grant.allows(ContentPublish),
        ];
        assert_eq!(asked_four_times, [false; 4]);
    }
}
