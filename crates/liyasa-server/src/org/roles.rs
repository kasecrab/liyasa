//! Where a dashboard role comes from (defect 65, ORG-01, ORG-02, AUTH-31).
//!
//! WP-15's session layer inserts a [`Principal`] carrying whatever the reader
//! authenticated as, and all four sign-in paths set [`Role::Reader`], whose
//! permission set is empty by construction. So the layer on its own moves
//! every gated endpoint from 401 for everyone to 403 for everyone. The
//! precondition for gating anything is a **role source**, and WP-15 left a
//! trait rather than inventing one:
//!
//! ```text
//! pub trait Roles: Debug + Send + Sync {
//!     fn grant_for(&self, subject: &str) -> Option<Grant>;
//! }
//! ```
//!
//! That source is organization membership, and it is this package's: ORG-01
//! is "change role", ORG-02 is "project-level roles override organization
//! defaults", and [`Member`](super::model::Member) already carries a
//! [`Grant`]. `org` depends on `auth` and not the other way round, so there is
//! no cycle.
//!
//! **Why the trait is not implemented in this file yet.** `auth::layer` is on
//! `wp/15-auth-domains` and not on `main`. Everything here compiles and is
//! tested without it; connecting it is three lines, which land in the same
//! chain as WP-15 rather than ahead of it (RFC 2802):
//!
//! ```text
//! impl crate::auth::layer::Roles for MembershipRoles {
//!     fn grant_for(&self, subject: &str) -> Option<Grant> {
//!         MembershipRoles::grant_for(self, subject)
//!     }
//! }
//! ```
//!
//! [`MembershipRoles::grant_for`] already has that exact signature, so the
//! impl forwards and nothing else moves.

use std::sync::Arc;

use crate::auth::roles::{Grant, Permission};

use super::state::OrgState;

/// Organization membership as a role source.
///
/// The grant it returns is the **organization-level** one. It cannot be the
/// project-level one, because `grant_for` takes a subject and no project —
/// see [`allows_in_project`] for the half this cannot answer.
#[derive(Clone)]
pub struct MembershipRoles {
    state: Arc<OrgState>,
}

impl std::fmt::Debug for MembershipRoles {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        let inner = self.state.read();
        f.debug_struct("MembershipRoles")
            .field("organization", &inner.org.id)
            .field("members", &inner.org.members().count())
            .finish()
    }
}

impl MembershipRoles {
    pub fn new(state: Arc<OrgState>) -> Self {
        Self { state }
    }

    /// The grant this subject holds across the organization, or `None` for
    /// somebody the membership table has never heard of — who stays whatever
    /// they authenticated as, which is a reader with no dashboard permission.
    ///
    /// Looked up per request rather than baked in at sign-in, so removing a
    /// member takes effect on their next request rather than on their next
    /// sign-in. That is the behaviour ORG-01's "remove" has to have: a removal
    /// that leaves a live session administering the organization until its
    /// cookie expires is not a removal.
    ///
    /// A subject is matched on the member id first and the email second. An
    /// OIDC `sub` is not an address, and a magic link knows only an address,
    /// so a source that matched one of them would work for half the sign-in
    /// paths and silently fail for the other half.
    ///
    /// **Two sign-in paths this cannot follow, and neither is fixable here.**
    /// `Roles::grant_for` receives a subject and nothing else — not the
    /// `Principal` — so a source cannot look at `via`, at `data`, or at
    /// anything but the one string it is handed. Read on `wp/15-auth-domains`,
    /// which is where all four paths live:
    ///
    /// * **Magic link.** `Reader::principal` sets the subject to
    ///   `address_hash`, a per-instance salted hash of the address (AUTH-05
    ///   keeps only the hash). It is neither an id nor an address, so no
    ///   member row can name it and a magic-link reader can never hold a
    ///   dashboard role.
    /// * **Shared site password.** `auth/routes.rs` builds
    ///   `Principal::new(format!("password:{env}"))` — one subject for
    ///   everyone who knows the password, identifying a *mode* rather than a
    ///   person. Nobody can be told apart, so nobody can be given a role.
    ///
    /// The second one used to be worse than "cannot match": a member row
    /// created with id `password:<env>` would have handed its grant to every
    /// reader who knew the site password. **WP-15 closed that on
    /// 2026-09-19** by having the flow declare itself — `Principal::shared`
    /// is true for the password flow and `apply_roles` returns early on it,
    /// so the question is not asked rather than answered carefully.
    ///
    /// No guard was written here, deliberately, and that is still the right
    /// call: matching a `"password:"` prefix would have coupled this file to
    /// another package's string format and would have stopped matching,
    /// silently, the day it changed — a guard that can rot without saying so
    /// buys the illusion of protection. The fix belonged where the subject is
    /// minted, and that is where it went (RFC 2802 §2b).
    pub fn grant_for(&self, subject: &str) -> Option<Grant> {
        let inner = self.state.read();
        if let Some(member) = inner.org.member(subject) {
            return Some(member.grant.clone());
        }
        inner
            .org
            .members()
            .find(|member| member.email.eq_ignore_ascii_case(subject))
            .map(|member| member.grant.clone())
    }
}

/// ORG-02, which [`MembershipRoles::grant_for`] cannot express.
///
/// `Roles::grant_for` takes a subject and no project, and
/// `routes::mount::guarded` applies one permission to a whole subtree. Neither
/// has a project in scope, so neither can see a project-level override. A
/// member who is an editor across the organization and a viewer on one project
/// therefore passes a subtree guard that asks for `ContentPublish` — the
/// override is a *reduction*, and a gate that cannot see it grants more than
/// the membership table says.
///
/// So a handler that names a project asks this as well. It is not a second
/// opinion on the subtree guard; it is the only place the project half is
/// decided.
pub fn in_project(
    state: &OrgState,
    subject: &str,
    project: &str,
    permission: Permission,
) -> ProjectVerdict {
    let inner = state.read();
    let member = inner.org.member(subject).or_else(|| {
        inner
            .org
            .members()
            .find(|member| member.email.eq_ignore_ascii_case(subject))
    });
    match member {
        Some(member) if member.allows(project, permission) => ProjectVerdict::Allowed,
        Some(_) => ProjectVerdict::Refused,
        None => ProjectVerdict::NotAMember,
    }
}

/// What membership has to say about a subject and a project.
///
/// Three answers rather than two, and the third is the one that matters.
/// `NotAMember` is **not** a refusal: an operator supplied by `StaticRoles`
/// has no membership row by construction — that is the whole point of the
/// escape hatch on an instance whose organization has no members yet — and a
/// gate that read "not a member" as "no" would lock the only person who can
/// add the first member out of doing it.
///
/// So the project check is a veto, not a grant: it can take away what the
/// subtree guard allowed, and it never gives.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ProjectVerdict {
    Allowed,
    /// A member whose grant on this project does not carry the permission.
    Refused,
    /// Membership has nothing to say; whatever elevated them, it was not this
    /// organization's member list.
    NotAMember,
}

impl ProjectVerdict {
    /// Whether a handler must refuse. Only a member who is refused stops here.
    pub fn is_refusal(self) -> bool {
        self == ProjectVerdict::Refused
    }
}

/// Membership's positive answer: whether the member list says yes. A
/// non-member is `false` here, which is correct for the question asked and is
/// why a gate uses [`ProjectVerdict::is_refusal`] instead.
pub fn allows_in_project(
    state: &OrgState,
    subject: &str,
    project: &str,
    permission: Permission,
) -> bool {
    in_project(state, subject, project, permission) == ProjectVerdict::Allowed
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::time::Duration;

    use crate::auth::clock::Clock;
    use crate::auth::roles::Role;
    use crate::org::model::{Member, Settings};
    use crate::org::plan::Plan;

    fn state() -> Arc<OrgState> {
        Arc::new(OrgState::with_clock(
            "acme",
            Settings::new("Acme"),
            Plan::unlimited(),
            Clock::manual(),
        ))
    }

    fn with_member(member: Member) -> Arc<OrgState> {
        let state = state();
        state.write().org.add_member(member).expect("a seat");
        state
    }

    #[test]
    fn an_instance_with_no_members_elevates_nobody() {
        // The whole point of defect 65's third correction: a role source that
        // returns a grant for somebody it has never heard of would be worse
        // than none at all.
        let roles = MembershipRoles::new(state());
        assert_eq!(roles.grant_for("u1"), None);
        assert_eq!(roles.grant_for(""), None);
        assert_eq!(roles.grant_for("ana@acme.com"), None);
    }

    #[test]
    fn a_member_carries_their_organization_grant() {
        let roles = MembershipRoles::new(with_member(Member::new(
            "u1",
            "ana@acme.com",
            Grant::role(Role::Admin),
        )));
        let grant = roles.grant_for("u1").expect("a member has a grant");
        assert_eq!(grant.role, Role::Admin);
        assert!(grant.allows(Permission::SettingsWrite));
        assert!(!grant.allows(Permission::OwnerAct));
    }

    #[test]
    fn a_subject_is_matched_on_the_id_or_the_address() {
        // An OIDC `sub` is not an address and a magic link knows only an
        // address. Matching one of them would work for half the sign-in paths.
        let roles = MembershipRoles::new(with_member(Member::new(
            "auth0|9f3",
            "Ana@Acme.com",
            Grant::role(Role::Editor),
        )));
        assert_eq!(
            roles.grant_for("auth0|9f3").map(|g| g.role),
            Some(Role::Editor)
        );
        assert_eq!(
            roles.grant_for("ana@acme.com").map(|g| g.role),
            Some(Role::Editor),
            "an address is matched without regard to case"
        );
        assert_eq!(roles.grant_for("someone@else.com"), None);
    }

    #[test]
    fn an_id_is_preferred_to_somebody_else_s_address() {
        let state = state();
        {
            let mut inner = state.write();
            inner
                .org
                .add_member(Member::new(
                    "shared",
                    "bo@acme.com",
                    Grant::role(Role::Owner),
                ))
                .expect("a seat");
            inner
                .org
                .add_member(Member::new(
                    "bo@acme.com",
                    "cy@acme.com",
                    Grant::role(Role::Viewer),
                ))
                .expect("a seat");
        }
        assert_eq!(
            MembershipRoles::new(state)
                .grant_for("bo@acme.com")
                .map(|g| g.role),
            Some(Role::Viewer),
            "the member whose id it is, not the member whose address it is"
        );
    }

    #[test]
    fn removing_a_member_takes_their_grant_away_on_the_next_request() {
        // ORG-01's "remove". Read twice, because the claim is about state
        // that survives the first read (rule 17).
        let state = with_member(Member::new("u1", "ana@acme.com", Grant::role(Role::Admin)));
        let roles = MembershipRoles::new(state.clone());
        assert_eq!(roles.grant_for("u1").map(|g| g.role), Some(Role::Admin));

        state.write().org.remove_member("u1").expect("the member");
        assert_eq!(
            roles.grant_for("u1"),
            None,
            "a removal that leaves a live session administering until its cookie expires is \
             not a removal"
        );
    }

    #[test]
    fn a_role_change_is_visible_without_signing_in_again() {
        // ORG-01's "change role", read twice for the same reason.
        let state = with_member(Member::new("u1", "ana@acme.com", Grant::role(Role::Viewer)));
        let roles = MembershipRoles::new(state.clone());
        assert!(
            !roles
                .grant_for("u1")
                .expect("a grant")
                .allows(Permission::SettingsWrite)
        );

        state.write().org.set_role("u1", Grant::role(Role::Admin));
        assert!(
            roles
                .grant_for("u1")
                .expect("a grant")
                .allows(Permission::SettingsWrite),
            "the source is read per request, so a membership change lands on the next one"
        );
    }

    #[test]
    fn a_project_override_that_reduces_a_role_is_invisible_to_the_organization_grant() {
        // ORG-02, and the reason `allows_in_project` exists. This is the
        // escalation the subtree guard cannot see: the organization grant is
        // what `Roles` returns, and it is larger than the project grant.
        let state = with_member(
            Member::new("u1", "ana@acme.com", Grant::role(Role::Editor))
                .with_override("secret", Grant::role(Role::Viewer)),
        );
        let roles = MembershipRoles::new(state.clone());

        assert!(
            roles
                .grant_for("u1")
                .expect("a grant")
                .allows(Permission::ContentPublish),
            "the organization grant, which is what a subtree guard asks"
        );
        assert!(
            !allows_in_project(&state, "u1", "secret", Permission::ContentPublish),
            "and the project grant, which is what ORG-02 says applies"
        );
        assert!(allows_in_project(
            &state,
            "u1",
            "docs",
            Permission::ContentPublish
        ));
    }

    #[test]
    fn a_project_override_is_consulted_by_address_too() {
        let state = with_member(
            Member::new("auth0|9f3", "ana@acme.com", Grant::role(Role::Editor))
                .with_override("secret", Grant::role(Role::Viewer)),
        );
        assert!(!allows_in_project(
            &state,
            "ana@acme.com",
            "secret",
            Permission::ContentPublish
        ));
        assert!(allows_in_project(
            &state,
            "ana@acme.com",
            "docs",
            Permission::ContentPublish
        ));
    }

    #[test]
    fn a_non_member_is_not_allowed_and_is_also_not_refused() {
        // The distinction the gate turns on. An operator from `StaticRoles`
        // has no membership row — that is what the escape hatch is for — and
        // reading "not a member" as "no" would lock the only person who can
        // add the first member out of adding them.
        let state = with_member(Member::new("u1", "ana@acme.com", Grant::role(Role::Owner)));
        let verdict = in_project(&state, "operator", "docs", Permission::SettingsWrite);
        assert_eq!(verdict, ProjectVerdict::NotAMember);
        assert!(!verdict.is_refusal(), "a gate must let them through");
        assert!(
            !allows_in_project(&state, "operator", "docs", Permission::SettingsWrite),
            "and membership still does not affirm them"
        );
    }

    #[test]
    fn only_a_member_whose_project_grant_falls_short_is_refused() {
        let state = with_member(
            Member::new("u1", "ana@acme.com", Grant::role(Role::Admin))
                .with_override("secret", Grant::role(Role::Viewer)),
        );
        assert_eq!(
            in_project(&state, "u1", "docs", Permission::SettingsWrite),
            ProjectVerdict::Allowed
        );
        let refused = in_project(&state, "u1", "secret", Permission::SettingsWrite);
        assert_eq!(refused, ProjectVerdict::Refused);
        assert!(refused.is_refusal());
    }

    #[test]
    fn an_invitation_confers_nothing_until_it_is_accepted() {
        // A pending invitation holds a seat; it does not hold a role. A source
        // that read invitations would hand the grant to whoever guessed the
        // address before the invited person accepted.
        use crate::org::model::InviteKind;

        let state = state();
        state
            .write()
            .org
            .invite(
                InviteKind::Email {
                    address: "bo@acme.com".to_owned(),
                },
                Grant::role(Role::Owner),
                Duration::from_secs(3600),
            )
            .expect("a seat");
        let roles = MembershipRoles::new(state.clone());
        assert_eq!(roles.grant_for("bo@acme.com"), None);

        let id = state
            .read()
            .org
            .invites()
            .next()
            .expect("the invitation")
            .id
            .clone();
        state
            .write()
            .org
            .accept_invite(&id, "u2", "bo@acme.com")
            .expect("accepted");
        assert_eq!(
            roles.grant_for("bo@acme.com").map(|g| g.role),
            Some(Role::Owner),
            "and it does confer one once accepted"
        );
    }

    #[test]
    fn the_debug_output_says_which_organization_and_how_many_members() {
        // The trait requires `Debug`, and a role source that prints as an
        // opaque struct tells an operator nothing about why nobody is elevated.
        let roles = MembershipRoles::new(with_member(Member::new(
            "u1",
            "ana@acme.com",
            Grant::role(Role::Admin),
        )));
        let text = format!("{roles:?}");
        assert!(text.contains("acme"), "{text}");
        assert!(text.contains('1'), "{text}");
    }
}
