//! Organizations, projects, members, invitations and credentials
//! (ORG-01, ORG-02, ORG-03), and the subdomain HOST-10 serves a project at.
//!
//! ORG-02's clause is the one with teeth: a project-level role overrides the
//! organization default. That is expressed as [`Member::grant_for`] rather
//! than as a rule handlers apply, so a surface that forgets to consult the
//! override cannot exist — there is no other way to get at a member's grant
//! for a project.

use std::collections::BTreeMap;
use std::time::Duration;

use liyasa_core::diagnostics::{Code, Diagnostic};
use serde::{Deserialize, Serialize};

use crate::auth::clock::Clock;
use crate::auth::roles::{Grant, Permission, Role};

use super::plan::{Feature, Plan, Resource};
use super::region::Region;

/// HOST-10: a project is served at `<project>.liyasa.site`.
pub const SUBDOMAIN_SUFFIX: &str = "liyasa.site";

/// Labels the platform answers for itself, which a project therefore cannot
/// take.
pub const RESERVED_LABELS: &[&str] = &[
    "www",
    "api",
    "app",
    "admin",
    "dashboard",
    "status",
    "assets",
    "cdn",
    "mail",
    "liyasa",
];

/// ORG-03: deletion has a cooling-off period, during which the workspace can
/// be restored and its data can still be exported.
pub const COOLING_OFF: Duration = Duration::from_secs(30 * 24 * 60 * 60);

/// HOST-10's subdomain rule, which is the DNS label rule.
pub fn check_label(label: &str) -> Result<(), Diagnostic> {
    let code = Code::new("E0857").expect("E0857 is registered");
    let refuse = |why: &str| {
        Err(
            Diagnostic::new(code, format!("`{label}` cannot be a subdomain label: {why}"))
                .help(format!(
                    "pick a label of lowercase letters, digits and hyphens, and set the display \
                     name separately; the project is served at <label>.{SUBDOMAIN_SUFFIX}"
                )),
        )
    };
    if label.is_empty() || label.len() > 63 {
        return refuse("a label is between one and sixty-three characters");
    }
    if !label
        .bytes()
        .all(|b| b.is_ascii_lowercase() || b.is_ascii_digit() || b == b'-')
    {
        return refuse("only lowercase letters, digits and hyphens are allowed");
    }
    if label.starts_with('-') || label.ends_with('-') {
        return refuse("a label may not start or end with a hyphen");
    }
    if RESERVED_LABELS.contains(&label) {
        return refuse("the platform answers for that name itself");
    }
    Ok(())
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Project {
    pub slug: String,
    pub title: String,
    /// Chosen at creation and never afterwards (HOST-11). There is no setter.
    pub region: Region,
    pub created_ms: i64,
}

impl Project {
    pub fn subdomain(&self) -> String {
        format!("{}.{SUBDOMAIN_SUFFIX}", self.slug)
    }
}

/// ORG-01: an invitation is by email or by link.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "camelCase")]
pub enum InviteKind {
    Email { address: String },
    /// Anyone holding the link may accept it once.
    Link,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum InviteState {
    Pending,
    Accepted,
    Revoked,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Invite {
    pub id: String,
    pub kind: InviteKind,
    pub grant: Grant,
    pub created_ms: i64,
    pub expires_ms: i64,
    pub state: InviteState,
    /// Set when the invitation is accepted.
    pub accepted_by: Option<String>,
}

impl Invite {
    /// An invitation holds a seat while it can still be accepted (ORG-01).
    pub fn holds_a_seat(&self, now_ms: i64) -> bool {
        self.state == InviteState::Pending && now_ms < self.expires_ms
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum CredentialKind {
    ApiKey,
    PersonalAccessToken,
    DeployKey,
}

/// ORG-01's "revoke credentials". The secret is not here: this is the record
/// that one exists and whether it still works.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Credential {
    pub id: String,
    pub label: String,
    pub kind: CredentialKind,
    pub owner: String,
    pub created_ms: i64,
    pub revoked_ms: Option<i64>,
}

impl Credential {
    pub fn is_live(&self) -> bool {
        self.revoked_ms.is_none()
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Member {
    pub user: String,
    pub email: String,
    /// The organization default (ORG-02).
    pub grant: Grant,
    /// Per project, overriding the default. ORG-02 says "override", not
    /// "add to": a project entry replaces the organization grant rather than
    /// unioning with it, so a member can be given less on one project.
    overrides: BTreeMap<String, Grant>,
}

impl Member {
    pub fn new(user: impl Into<String>, email: impl Into<String>, grant: Grant) -> Self {
        Self {
            user: user.into(),
            email: email.into(),
            grant,
            overrides: BTreeMap::new(),
        }
    }

    pub fn with_override(mut self, project: impl Into<String>, grant: Grant) -> Self {
        self.overrides.insert(project.into(), grant);
        self
    }

    pub fn set_override(&mut self, project: impl Into<String>, grant: Grant) {
        self.overrides.insert(project.into(), grant);
    }

    pub fn clear_override(&mut self, project: &str) {
        self.overrides.remove(project);
    }

    /// ORG-02. The only way to read a member's grant for a project.
    pub fn grant_for(&self, project: &str) -> &Grant {
        self.overrides.get(project).unwrap_or(&self.grant)
    }

    pub fn allows(&self, project: &str, permission: Permission) -> bool {
        self.grant_for(project).allows(permission)
    }

    pub fn overrides(&self) -> impl Iterator<Item = (&str, &Grant)> {
        self.overrides.iter().map(|(p, g)| (p.as_str(), g))
    }
}

/// ORG-03's workspace settings.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Settings {
    pub name: String,
    pub icon: Option<String>,
    /// What a new project takes when it does not name one (HOST-11).
    pub default_region: Region,
}

impl Settings {
    pub fn new(name: impl Into<String>) -> Self {
        Self {
            name: name.into(),
            icon: None,
            default_region: Region::default(),
        }
    }
}

/// ORG-03: deletion with a cooling-off period and an export.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Deletion {
    pub requested_by: String,
    pub requested_ms: i64,
    /// Nothing is destroyed before this instant, and the workspace can be
    /// restored until it.
    pub effective_ms: i64,
}

#[derive(Debug, Clone)]
pub struct Organization {
    pub id: String,
    pub settings: Settings,
    pub plan: Plan,
    projects: BTreeMap<String, Project>,
    members: BTreeMap<String, Member>,
    invites: BTreeMap<String, Invite>,
    credentials: BTreeMap<String, Credential>,
    deletion: Option<Deletion>,
    clock: Clock,
    counter: u64,
}

impl Organization {
    pub fn new(id: impl Into<String>, settings: Settings, plan: Plan) -> Self {
        Self {
            id: id.into(),
            settings,
            plan,
            projects: BTreeMap::new(),
            members: BTreeMap::new(),
            invites: BTreeMap::new(),
            credentials: BTreeMap::new(),
            deletion: None,
            clock: Clock::default(),
            counter: 0,
        }
    }

    pub fn with_clock(mut self, clock: Clock) -> Self {
        self.clock = clock;
        self
    }

    pub fn clock(&self) -> &Clock {
        &self.clock
    }

    fn next_id(&mut self, prefix: &str) -> String {
        self.counter += 1;
        format!("{prefix}_{:06}", self.counter)
    }

    // ---- projects (ORG-02) ----

    pub fn projects(&self) -> impl Iterator<Item = &Project> {
        self.projects.values()
    }

    pub fn project(&self, slug: &str) -> Option<&Project> {
        self.projects.get(slug)
    }

    /// ORG-02, HOST-11: a project is created with a region, and the plan's
    /// project quota is checked before anything is written.
    pub fn create_project(
        &mut self,
        slug: &str,
        title: &str,
        region: Option<Region>,
    ) -> Result<&Project, Diagnostic> {
        check_label(slug)?;
        if self.projects.contains_key(slug) {
            let code = Code::new("E0856").expect("E0856 is registered");
            return Err(Diagnostic::new(
                code,
                format!("`{slug}` already names a project in this organization"),
            )
            .help("pick another label, or delete the existing project first"));
        }
        if let Some(limit) = self.plan.quota(Resource::Projects)
            && self.projects.len() as u64 >= limit
        {
            let code = Code::new("E0856").expect("E0856 is registered");
            return Err(Diagnostic::new(
                code,
                format!(
                    "the {} plan allows {limit} project(s) and this organization has {}",
                    self.plan.tier.as_str(),
                    self.projects.len()
                ),
            )
            .help("delete a project you no longer need, or move to a plan with no project limit"));
        }
        let project = Project {
            slug: slug.to_owned(),
            title: title.to_owned(),
            region: region.unwrap_or(self.settings.default_region),
            created_ms: self.clock.now_ms(),
        };
        self.projects.insert(slug.to_owned(), project);
        Ok(self.projects.get(slug).expect("just inserted"))
    }

    pub fn delete_project(&mut self, slug: &str) -> Option<Project> {
        for member in self.members.values_mut() {
            member.clear_override(slug);
        }
        self.projects.remove(slug)
    }

    // ---- members and seats (ORG-01) ----

    pub fn members(&self) -> impl Iterator<Item = &Member> {
        self.members.values()
    }

    pub fn member(&self, user: &str) -> Option<&Member> {
        self.members.get(user)
    }

    /// Members plus invitations that can still be accepted (ORG-01).
    pub fn seats_taken(&self) -> u64 {
        let now = self.clock.now_ms();
        self.members.len() as u64
            + self
                .invites
                .values()
                .filter(|invite| invite.holds_a_seat(now))
                .count() as u64
    }

    fn check_seat(&self) -> Result<(), Diagnostic> {
        let Some(limit) = self.plan.quota(Resource::Seats) else {
            return Ok(());
        };
        if self.seats_taken() < limit {
            return Ok(());
        }
        let code = Code::new("E0851").expect("E0851 is registered");
        Err(Diagnostic::new(
            code,
            format!(
                "the {} plan carries {limit} seats and all of them are taken",
                self.plan.tier.as_str()
            ),
        )
        .help("revoke a pending invitation, remove a member, or move to a plan that bills per seat"))
    }

    pub fn add_member(&mut self, member: Member) -> Result<(), Diagnostic> {
        if !self.members.contains_key(&member.user) {
            self.check_seat()?;
        }
        self.members.insert(member.user.clone(), member);
        Ok(())
    }

    /// ORG-01: change a role.
    pub fn set_role(&mut self, user: &str, grant: Grant) -> bool {
        match self.members.get_mut(user) {
            Some(member) => {
                member.grant = grant;
                true
            }
            None => false,
        }
    }

    /// ORG-02: a project-level role that overrides the organization default.
    pub fn set_project_role(&mut self, user: &str, project: &str, grant: Grant) -> bool {
        match self.members.get_mut(user) {
            Some(member) => {
                member.set_override(project, grant);
                true
            }
            None => false,
        }
    }

    /// ORG-01: remove a member. Their credentials go with them — a revoked
    /// member whose API key still works has not been removed.
    pub fn remove_member(&mut self, user: &str) -> Option<Member> {
        let removed = self.members.remove(user)?;
        let now = self.clock.now_ms();
        for credential in self.credentials.values_mut() {
            if credential.owner == removed.user && credential.is_live() {
                credential.revoked_ms = Some(now);
            }
        }
        Some(removed)
    }

    // ---- invitations (ORG-01) ----

    pub fn invites(&self) -> impl Iterator<Item = &Invite> {
        self.invites.values()
    }

    pub fn invite(&mut self, kind: InviteKind, grant: Grant, valid_for: Duration) -> Result<&Invite, Diagnostic> {
        self.check_seat()?;
        let now = self.clock.now_ms();
        let id = self.next_id("inv");
        let invite = Invite {
            id: id.clone(),
            kind,
            grant,
            created_ms: now,
            expires_ms: now.saturating_add(crate::auth::clock::millis(valid_for)),
            state: InviteState::Pending,
            accepted_by: None,
        };
        self.invites.insert(id.clone(), invite);
        Ok(self.invites.get(&id).expect("just inserted"))
    }

    pub fn revoke_invite(&mut self, id: &str) -> bool {
        match self.invites.get_mut(id) {
            Some(invite) if invite.state == InviteState::Pending => {
                invite.state = InviteState::Revoked;
                true
            }
            _ => false,
        }
    }

    /// Accepting turns the seat the invitation was holding into a member, so
    /// the seat count does not move.
    pub fn accept_invite(
        &mut self,
        id: &str,
        user: &str,
        email: &str,
    ) -> Result<&Member, Diagnostic> {
        let now = self.clock.now_ms();
        let Some(invite) = self.invites.get(id) else {
            let code = Code::new("E0851").expect("E0851 is registered");
            return Err(Diagnostic::new(code, format!("no invitation `{id}`"))
                .help("ask an administrator to send another one"));
        };
        if !invite.holds_a_seat(now) {
            let code = Code::new("E0851").expect("E0851 is registered");
            let why = match invite.state {
                InviteState::Pending => "expired",
                InviteState::Accepted => "already been accepted",
                InviteState::Revoked => "been revoked",
            };
            return Err(
                Diagnostic::new(code, format!("invitation `{id}` has {why}"))
                    .help("ask an administrator to send another one"),
            );
        }
        let grant = invite.grant.clone();
        let invite = self.invites.get_mut(id).expect("checked above");
        invite.state = InviteState::Accepted;
        invite.accepted_by = Some(user.to_owned());
        self.members
            .insert(user.to_owned(), Member::new(user, email, grant));
        Ok(self.members.get(user).expect("just inserted"))
    }

    // ---- credentials (ORG-01) ----

    pub fn credentials(&self) -> impl Iterator<Item = &Credential> {
        self.credentials.values()
    }

    pub fn issue_credential(
        &mut self,
        kind: CredentialKind,
        label: &str,
        owner: &str,
    ) -> &Credential {
        let now = self.clock.now_ms();
        let id = self.next_id("cred");
        self.credentials.insert(
            id.clone(),
            Credential {
                id: id.clone(),
                label: label.to_owned(),
                kind,
                owner: owner.to_owned(),
                created_ms: now,
                revoked_ms: None,
            },
        );
        self.credentials.get(&id).expect("just inserted")
    }

    pub fn revoke_credential(&mut self, id: &str) -> bool {
        let now = self.clock.now_ms();
        match self.credentials.get_mut(id) {
            Some(credential) if credential.is_live() => {
                credential.revoked_ms = Some(now);
                true
            }
            _ => false,
        }
    }

    // ---- plan features (ORG-30, ORG-32) ----

    /// The one place a handler asks whether the plan carries something.
    pub fn require(&self, feature: Feature) -> Result<(), Diagnostic> {
        if self.plan.allows(feature) {
            return Ok(());
        }
        let code = Code::new("E0853").expect("E0853 is registered");
        Err(Diagnostic::new(
            code,
            format!(
                "`{}` is not included in the {} plan",
                feature.as_str(),
                self.plan.tier.as_str()
            ),
        )
        .help("move to a plan that includes it, or run your own instance, which has every feature"))
    }

    // ---- deletion and export (ORG-03) ----

    pub fn deletion(&self) -> Option<&Deletion> {
        self.deletion.as_ref()
    }

    pub fn request_deletion(&mut self, by: &str) -> &Deletion {
        let now = self.clock.now_ms();
        self.deletion = Some(Deletion {
            requested_by: by.to_owned(),
            requested_ms: now,
            effective_ms: now.saturating_add(crate::auth::clock::millis(COOLING_OFF)),
        });
        self.deletion.as_ref().expect("just set")
    }

    pub fn cancel_deletion(&mut self) -> bool {
        self.deletion.take().is_some()
    }

    /// Whether the cooling-off period has run out. A workspace inside it is
    /// still fully readable, which is what makes the export worth having.
    pub fn is_erasable(&self) -> bool {
        self.deletion
            .as_ref()
            .is_some_and(|d| self.clock.now_ms() >= d.effective_ms)
    }

    /// ORG-03's export: everything this organization holds, as JSON. Secrets
    /// are not in the model at all, so there is nothing here to redact.
    pub fn export(&self) -> serde_json::Value {
        serde_json::json!({
            "id": self.id,
            "settings": self.settings,
            "plan": self.plan,
            "projects": self.projects.values().collect::<Vec<_>>(),
            "members": self.members.values().collect::<Vec<_>>(),
            "invites": self.invites.values().collect::<Vec<_>>(),
            "credentials": self.credentials.values().collect::<Vec<_>>(),
            "deletion": self.deletion,
            "exportedAt": self.clock.now_ms(),
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::org::plan::Tier;

    fn org(tier: Tier) -> Organization {
        Organization::new("org_1", Settings::new("Acme"), Plan::of(tier)).with_clock(Clock::manual())
    }

    #[test]
    fn a_project_level_role_overrides_the_organization_default() {
        // ORG-02. The override replaces rather than adds: a member who is an
        // editor across the organization can be a viewer on one project.
        let member = Member::new("u1", "u1@acme.com", Grant::role(Role::Editor))
            .with_override("secret", Grant::role(Role::Viewer));
        assert!(member.allows("docs", Permission::ContentPublish));
        assert!(!member.allows("secret", Permission::ContentPublish));
        assert!(member.allows("secret", Permission::DashboardRead));
        assert_eq!(member.grant_for("docs").role, Role::Editor);
        assert_eq!(member.grant_for("secret").role, Role::Viewer);
    }

    #[test]
    fn a_project_takes_the_workspace_region_unless_it_names_one() {
        // ORG-03 gives the workspace a default region; HOST-11 lets a project
        // choose at creation.
        let mut org = org(Tier::Pro);
        org.settings.default_region = Region::Eu;
        assert_eq!(
            org.create_project("docs", "Docs", None)
                .expect("a project")
                .region,
            Region::Eu
        );
        assert_eq!(
            org.create_project("api-reference", "API", Some(Region::Apac))
                .expect("a project")
                .region,
            Region::Apac
        );
    }

    #[test]
    fn a_project_is_served_at_its_subdomain() {
        // HOST-10.
        let mut org = org(Tier::Pro);
        let project = org.create_project("acme-docs", "Acme", None).expect("a project");
        assert_eq!(project.subdomain(), "acme-docs.liyasa.site");
    }

    #[test]
    fn a_name_that_is_not_a_dns_label_is_refused_before_the_project_exists() {
        let mut org = org(Tier::Pro);
        for bad in ["", "Docs", "-docs", "docs-", "docs.api", "www", &"x".repeat(64)] {
            let refused = org
                .create_project(bad, "Title", None)
                .expect_err(&format!("`{bad}` is not a label"));
            assert_eq!(refused.code.as_str(), "E0857");
        }
        assert_eq!(org.projects().count(), 0);
        assert!(org.create_project("docs-2", "Title", None).is_ok());
    }

    #[test]
    fn free_allows_one_project_and_says_so_rather_than_failing_later() {
        let mut org = org(Tier::Free);
        assert!(org.create_project("docs", "Docs", None).is_ok());
        let refused = org
            .create_project("api-reference", "API", None)
            .expect_err("free carries one project");
        assert_eq!(refused.code.as_str(), "E0856");
        assert!(refused.message.contains("free"), "{}", refused.message);
        assert_eq!(org.projects().count(), 1);
    }

    #[test]
    fn a_pending_invitation_holds_a_seat_and_accepting_it_does_not_take_a_second() {
        // ORG-01: otherwise a plan's seat limit is enforced against members
        // only, and an administrator can invite past it and be surprised.
        let mut org = org(Tier::Free);
        org.add_member(Member::new("u1", "u1@acme.com", Grant::role(Role::Owner)))
            .expect("a seat");
        org.invite(
            InviteKind::Email {
                address: "u2@acme.com".to_owned(),
            },
            Grant::role(Role::Editor),
            Duration::from_secs(3600),
        )
        .expect("a seat");
        assert_eq!(org.seats_taken(), 2);

        let id = org.invites().next().expect("the invitation").id.clone();
        org.accept_invite(&id, "u2", "u2@acme.com")
            .expect("an accepted invitation");
        assert_eq!(org.seats_taken(), 2, "the held seat became the member");
        assert_eq!(org.member("u2").expect("a member").grant.role, Role::Editor);
    }

    #[test]
    fn the_seat_limit_counts_invitations_and_refuses_the_one_past_it() {
        let mut org = org(Tier::Free);
        for n in 0..3 {
            org.add_member(Member::new(
                format!("u{n}"),
                format!("u{n}@acme.com"),
                Grant::role(Role::Viewer),
            ))
            .expect("a seat");
        }
        let refused = org
            .invite(InviteKind::Link, Grant::role(Role::Viewer), Duration::from_secs(60))
            .expect_err("three seats are taken");
        assert_eq!(refused.code.as_str(), "E0851");
        assert_eq!(org.invites().count(), 0, "a refused invitation is not stored");

        // Changing an existing member's role does not need a seat.
        assert!(org.set_role("u0", Grant::role(Role::Owner)));
        assert_eq!(org.seats_taken(), 3);
    }

    #[test]
    fn an_expired_invitation_releases_its_seat_and_cannot_be_accepted() {
        let mut org = org(Tier::Free);
        org.invite(InviteKind::Link, Grant::role(Role::Viewer), Duration::from_secs(60))
            .expect("a seat");
        assert_eq!(org.seats_taken(), 1);
        org.clock().advance(Duration::from_secs(120));
        assert_eq!(org.seats_taken(), 0);

        let id = org.invites().next().expect("the invitation").id.clone();
        let refused = org
            .accept_invite(&id, "u1", "u1@acme.com")
            .expect_err("an expired invitation");
        assert_eq!(refused.code.as_str(), "E0851");
        assert!(refused.message.contains("expired"), "{}", refused.message);
    }

    #[test]
    fn a_revoked_invitation_cannot_be_accepted() {
        let mut org = org(Tier::Pro);
        let id = org
            .invite(InviteKind::Link, Grant::role(Role::Viewer), Duration::from_secs(3600))
            .expect("an invitation")
            .id
            .clone();
        assert!(org.revoke_invite(&id));
        assert!(!org.revoke_invite(&id), "revoking twice changes nothing");
        assert!(org.accept_invite(&id, "u1", "u1@acme.com").is_err());
    }

    #[test]
    fn removing_a_member_revokes_the_credentials_they_held() {
        // ORG-01 lists "remove" and "revoke credentials" together, and a
        // member whose API key still works has not been removed.
        let mut org = org(Tier::Pro);
        org.add_member(Member::new("u1", "u1@acme.com", Grant::role(Role::Admin)))
            .expect("a seat");
        org.add_member(Member::new("u2", "u2@acme.com", Grant::role(Role::Admin)))
            .expect("a seat");
        let mine = org
            .issue_credential(CredentialKind::ApiKey, "ci", "u1")
            .id
            .clone();
        let theirs = org
            .issue_credential(CredentialKind::ApiKey, "ci", "u2")
            .id
            .clone();

        org.remove_member("u1").expect("the member");
        let live: Vec<&str> = org
            .credentials()
            .filter(|c| c.is_live())
            .map(|c| c.id.as_str())
            .collect();
        assert_eq!(live, vec![theirs.as_str()]);
        assert!(!mine.is_empty());
    }

    #[test]
    fn a_credential_can_be_revoked_once() {
        let mut org = org(Tier::Pro);
        let id = org
            .issue_credential(CredentialKind::DeployKey, "deploy", "u1")
            .id
            .clone();
        assert!(org.revoke_credential(&id));
        assert!(!org.revoke_credential(&id));
        assert!(!org.credentials().next().expect("the credential").is_live());
    }

    #[test]
    fn deleting_a_project_drops_the_overrides_that_named_it() {
        let mut org = org(Tier::Pro);
        org.create_project("secret", "Secret", None).expect("a project");
        org.add_member(Member::new("u1", "u1@acme.com", Grant::role(Role::Editor)))
            .expect("a seat");
        org.set_project_role("u1", "secret", Grant::role(Role::Viewer));
        assert_eq!(org.member("u1").expect("a member").overrides().count(), 1);

        org.delete_project("secret");
        assert_eq!(
            org.member("u1").expect("a member").overrides().count(),
            0,
            "an override naming a project that no longer exists would silently apply to a \
             project created with the same slug later"
        );
    }

    #[test]
    fn deletion_waits_out_the_cooling_off_period_and_can_be_cancelled() {
        // ORG-03. The workspace stays readable throughout, which is what
        // makes the export in the same clause useful.
        let mut org = org(Tier::Pro);
        org.create_project("docs", "Docs", None).expect("a project");
        org.request_deletion("u1");
        assert!(!org.is_erasable());
        assert!(org.export()["projects"].as_array().is_some_and(|p| p.len() == 1));

        org.clock().advance(COOLING_OFF / 2);
        assert!(!org.is_erasable());
        assert!(org.cancel_deletion());
        assert!(org.deletion().is_none());

        org.request_deletion("u1");
        org.clock().advance(COOLING_OFF + Duration::from_secs(1));
        assert!(org.is_erasable());
    }

    #[test]
    fn the_export_carries_everything_the_workspace_holds() {
        let mut org = org(Tier::Pro);
        org.create_project("docs", "Docs", Some(Region::Eu)).expect("a project");
        org.add_member(Member::new("u1", "u1@acme.com", Grant::role(Role::Owner)))
            .expect("a seat");
        org.issue_credential(CredentialKind::ApiKey, "ci", "u1");
        let export = org.export();
        assert_eq!(export["id"], "org_1");
        assert_eq!(export["settings"]["name"], "Acme");
        assert_eq!(export["plan"]["tier"], "pro");
        assert_eq!(export["projects"][0]["region"], "eu");
        assert_eq!(export["members"][0]["email"], "u1@acme.com");
        assert_eq!(export["credentials"][0]["label"], "ci");
    }

    #[test]
    fn a_feature_the_plan_does_not_carry_is_refused_with_its_own_code() {
        let free = org(Tier::Free);
        let refused = free
            .require(Feature::CustomDomains)
            .expect_err("free has no custom domains");
        assert_eq!(refused.code.as_str(), "E0853");
        assert!(org(Tier::Pro).require(Feature::CustomDomains).is_ok());
        assert!(org(Tier::Pro).require(Feature::Sso).is_err());
        // ORG-32: an instance an operator runs has everything.
        assert!(org(Tier::Unlimited).require(Feature::Sso).is_ok());
    }
}
