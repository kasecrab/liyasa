//! Sessions and the reader they identify (AUTH-09, AUTH-22).
//!
//! The identifier is opaque and server-side: the cookie carries a random
//! token and nothing else, so a reader cannot read or forge their own groups.
//! It is regenerated on every privilege change — login and any group change —
//! which is what makes session fixation ineffective, and `logout` removes the
//! entry rather than merely clearing the cookie.
//!
//! The table is per instance. A multi-replica deployment needs a shared
//! session store, which the store crate does not have a table for yet; until
//! it does, `liyasa serve` is single-replica for a non-public site.
// TODO(rfc-1501): a `session` table in `migrations/`, once WP-14 can take one.

use std::collections::{BTreeMap, BTreeSet, HashMap};
use std::sync::RwLock;
use std::time::Duration;

use serde::{Deserialize, Serialize};

use crate::auth::clock::{Clock, millis};
use crate::auth::config::SessionConfig;
use crate::auth::random::{self, NoEntropy};
use crate::auth::roles::Role;

/// Who the reader is, as every surface that filters by group sees them
/// (AUTH-10). `data` is the arbitrary user data AUTH-03 lets an operator's
/// JWT carry; it reaches templates as `reader.*` and nothing else reads it.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct Principal {
    pub subject: String,
    pub groups: BTreeSet<String>,
    pub region: Option<String>,
    pub locale: Option<String>,
    #[serde(default)]
    pub data: BTreeMap<String, serde_json::Value>,
    #[serde(default)]
    pub role: Role,
    /// Which flow authenticated them, for the introspection endpoint.
    #[serde(default)]
    pub via: String,
}

impl Principal {
    pub fn new(subject: impl Into<String>) -> Self {
        Self {
            subject: subject.into(),
            ..Self::default()
        }
    }

    pub fn with_groups<I, S>(mut self, groups: I) -> Self
    where
        I: IntoIterator<Item = S>,
        S: Into<String>,
    {
        self.groups = groups.into_iter().map(Into::into).collect();
        self
    }

    pub fn with_role(mut self, role: Role) -> Self {
        self.role = role;
        self
    }

    pub fn with_via(mut self, via: impl Into<String>) -> Self {
        self.via = via.into();
        self
    }

    /// What a privilege change is: the groups or the role moved. The subject
    /// changing is a different reader entirely and is handled by logging the
    /// old session out.
    fn privileges_differ(&self, other: &Principal) -> bool {
        self.groups != other.groups || self.role != other.role
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Session {
    pub id: String,
    pub principal: Principal,
    /// The CSRF token this session's state-changing requests must carry.
    pub csrf: String,
    pub created_ms: i64,
    pub last_seen_ms: i64,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Policy {
    pub max_age: Duration,
    pub idle_timeout: Duration,
}

impl Default for Policy {
    fn default() -> Self {
        Self {
            max_age: Duration::from_secs(30 * 86_400),
            idle_timeout: Duration::from_secs(7 * 86_400),
        }
    }
}

impl From<&SessionConfig> for Policy {
    fn from(config: &SessionConfig) -> Self {
        Self {
            max_age: config.max_age(),
            idle_timeout: config.idle_timeout(),
        }
    }
}

/// Why a session identifier did not resolve. The reader is told "sign in
/// again" either way; the distinction is for the audit log and the metrics.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Rejected {
    Unknown,
    Expired,
    Idle,
}

#[derive(Debug)]
pub struct Sessions {
    live: RwLock<HashMap<String, Session>>,
    policy: Policy,
    clock: Clock,
}

impl Default for Sessions {
    fn default() -> Self {
        Self::new(Policy::default(), Clock::System)
    }
}

impl Sessions {
    pub fn new(policy: Policy, clock: Clock) -> Self {
        Self {
            live: RwLock::new(HashMap::new()),
            policy,
            clock,
        }
    }

    pub fn clock(&self) -> &Clock {
        &self.clock
    }

    pub fn policy(&self) -> Policy {
        self.policy
    }

    /// Logging in. Always a fresh identifier, whatever the caller was carrying
    /// before: this is the privilege change fixation attacks rely on.
    pub fn begin(&self, principal: Principal) -> Result<Session, NoEntropy> {
        let now = self.clock.now_ms();
        let session = Session {
            id: random::token()?,
            csrf: random::token()?,
            principal,
            created_ms: now,
            last_seen_ms: now,
        };
        self.write().insert(session.id.clone(), session.clone());
        Ok(session)
    }

    /// Resolves a cookie value, enforcing both timeouts and touching the idle
    /// clock. An expired session is removed here rather than left for a sweep.
    pub fn resolve(&self, id: &str) -> Result<Session, Rejected> {
        let now = self.clock.now_ms();
        let mut live = self.write();
        let Some(session) = live.get_mut(id) else {
            return Err(Rejected::Unknown);
        };
        if now.saturating_sub(session.created_ms) >= millis(self.policy.max_age) {
            live.remove(id);
            return Err(Rejected::Expired);
        }
        if now.saturating_sub(session.last_seen_ms) >= millis(self.policy.idle_timeout) {
            live.remove(id);
            return Err(Rejected::Idle);
        }
        session.last_seen_ms = now;
        Ok(session.clone())
    }

    /// A principal arriving again — a JWT re-presented, a group list refreshed
    /// from the identity provider. The identifier is regenerated when and only
    /// when the privileges moved; everything else keeps the session so an
    /// ordinary page view does not mint a cookie.
    pub fn refresh(&self, id: &str, principal: Principal) -> Result<Session, Rejected> {
        let current = self.resolve(id)?;
        if !current.principal.privileges_differ(&principal) {
            if current.principal == principal {
                return Ok(current);
            }
            let now = self.clock.now_ms();
            let mut live = self.write();
            let Some(session) = live.get_mut(id) else {
                return Err(Rejected::Unknown);
            };
            session.principal = principal;
            session.last_seen_ms = now;
            return Ok(session.clone());
        }
        self.write().remove(id);
        self.begin(principal).map_err(|_| Rejected::Unknown)
    }

    /// `POST /_liyasa/auth/logout`: the entry goes, so the cookie is worthless
    /// even where the browser kept it.
    pub fn invalidate(&self, id: &str) -> bool {
        self.write().remove(id).is_some()
    }

    /// Every session of one subject, which is what a group change on the
    /// operator's side and an account deletion both need.
    pub fn invalidate_subject(&self, subject: &str) -> usize {
        let mut live = self.write();
        let doomed: Vec<String> = live
            .values()
            .filter(|s| s.principal.subject == subject)
            .map(|s| s.id.clone())
            .collect();
        for id in &doomed {
            live.remove(id);
        }
        doomed.len()
    }

    pub fn len(&self) -> usize {
        self.read().len()
    }

    pub fn is_empty(&self) -> bool {
        self.len() == 0
    }

    /// Drops what the timeouts have already made unusable. Nothing depends on
    /// it running: `resolve` refuses an expired session whether or not a sweep
    /// has been by.
    pub fn sweep(&self) -> usize {
        let now = self.clock.now_ms();
        let max_age = millis(self.policy.max_age);
        let idle = millis(self.policy.idle_timeout);
        let mut live = self.write();
        let before = live.len();
        live.retain(|_, session| {
            now.saturating_sub(session.created_ms) < max_age
                && now.saturating_sub(session.last_seen_ms) < idle
        });
        before - live.len()
    }

    fn read(&self) -> std::sync::RwLockReadGuard<'_, HashMap<String, Session>> {
        self.live.read().unwrap_or_else(|e| e.into_inner())
    }

    fn write(&self) -> std::sync::RwLockWriteGuard<'_, HashMap<String, Session>> {
        self.live.write().unwrap_or_else(|e| e.into_inner())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn sessions(max_age: u64, idle: u64) -> Sessions {
        Sessions::new(
            Policy {
                max_age: Duration::from_secs(max_age),
                idle_timeout: Duration::from_secs(idle),
            },
            Clock::manual(),
        )
    }

    #[test]
    fn a_session_resolves_until_its_absolute_lifetime_runs_out() {
        let sessions = sessions(3_600, 3_600);
        let session = sessions.begin(Principal::new("reader")).expect("a session");
        assert_eq!(
            sessions
                .resolve(&session.id)
                .expect("live")
                .principal
                .subject,
            "reader"
        );
        sessions.clock().advance(Duration::from_secs(3_600));
        assert_eq!(sessions.resolve(&session.id), Err(Rejected::Expired));
        assert!(sessions.is_empty(), "an expired session is not kept");
    }

    #[test]
    fn an_idle_session_expires_even_though_its_lifetime_has_not_run_out() {
        let sessions = sessions(30 * 86_400, 600);
        let session = sessions.begin(Principal::new("reader")).expect("a session");
        sessions.clock().advance(Duration::from_secs(599));
        assert!(
            sessions.resolve(&session.id).is_ok(),
            "still inside the idle window"
        );
        // The successful resolve above touched it, so another 599 seconds is
        // still inside the window: idle is measured from the last request.
        sessions.clock().advance(Duration::from_secs(599));
        assert!(sessions.resolve(&session.id).is_ok());
        sessions.clock().advance(Duration::from_secs(600));
        assert_eq!(sessions.resolve(&session.id), Err(Rejected::Idle));
    }

    #[test]
    fn logging_in_always_mints_a_new_identifier() {
        let sessions = sessions(3_600, 3_600);
        let first = sessions.begin(Principal::new("reader")).expect("a session");
        let second = sessions.begin(Principal::new("reader")).expect("a session");
        assert_ne!(first.id, second.id, "AUTH-09 forbids reusing an identifier");
        assert_ne!(first.csrf, second.csrf);
    }

    #[test]
    fn a_group_change_regenerates_the_identifier_and_the_old_one_dies() {
        let sessions = sessions(3_600, 3_600);
        let before = sessions
            .begin(Principal::new("reader").with_groups(["partner"]))
            .expect("a session");
        let after = sessions
            .refresh(
                &before.id,
                Principal::new("reader").with_groups(["partner", "admin"]),
            )
            .expect("a refresh");
        assert_ne!(before.id, after.id, "a privilege change regenerates");
        assert_eq!(
            sessions.resolve(&before.id),
            Err(Rejected::Unknown),
            "the pre-change identifier must not still work"
        );
        assert!(after.principal.groups.contains("admin"));
    }

    #[test]
    fn a_role_change_is_a_privilege_change_too() {
        let sessions = sessions(3_600, 3_600);
        let before = sessions
            .begin(Principal::new("reader").with_role(Role::Viewer))
            .expect("a session");
        let after = sessions
            .refresh(&before.id, Principal::new("reader").with_role(Role::Admin))
            .expect("a refresh");
        assert_ne!(before.id, after.id);
    }

    #[test]
    fn a_refresh_that_changes_no_privilege_keeps_the_session() {
        let sessions = sessions(3_600, 3_600);
        let before = sessions
            .begin(Principal::new("reader").with_groups(["partner"]))
            .expect("a session");
        let same = sessions
            .refresh(
                &before.id,
                Principal::new("reader").with_groups(["partner"]),
            )
            .expect("a refresh");
        assert_eq!(before.id, same.id, "an ordinary page view mints no cookie");

        // Non-privilege data may move without a new identifier.
        let mut moved = Principal::new("reader").with_groups(["partner"]);
        moved.region = Some("DE".to_owned());
        let updated = sessions.refresh(&before.id, moved).expect("a refresh");
        assert_eq!(before.id, updated.id);
        assert_eq!(updated.principal.region.as_deref(), Some("DE"));
    }

    #[test]
    fn logout_invalidates_server_side_rather_than_only_clearing_the_cookie() {
        let sessions = sessions(3_600, 3_600);
        let session = sessions.begin(Principal::new("reader")).expect("a session");
        assert!(sessions.invalidate(&session.id));
        assert_eq!(sessions.resolve(&session.id), Err(Rejected::Unknown));
        assert!(!sessions.invalidate(&session.id), "twice is not an error");
    }

    #[test]
    fn every_session_of_one_subject_can_be_invalidated_at_once() {
        let sessions = sessions(3_600, 3_600);
        let a = sessions.begin(Principal::new("reader")).expect("a session");
        let b = sessions.begin(Principal::new("reader")).expect("a session");
        let other = sessions.begin(Principal::new("other")).expect("a session");
        assert_eq!(sessions.invalidate_subject("reader"), 2);
        assert_eq!(sessions.resolve(&a.id), Err(Rejected::Unknown));
        assert_eq!(sessions.resolve(&b.id), Err(Rejected::Unknown));
        assert!(sessions.resolve(&other.id).is_ok());
    }

    #[test]
    fn an_unknown_identifier_resolves_to_nothing() {
        let sessions = sessions(3_600, 3_600);
        assert_eq!(sessions.resolve("not-a-session"), Err(Rejected::Unknown));
        assert_eq!(sessions.resolve(""), Err(Rejected::Unknown));
    }

    #[test]
    fn a_sweep_drops_what_the_timeouts_already_made_unusable() {
        let sessions = sessions(600, 600);
        let live = sessions.begin(Principal::new("a")).expect("a session");
        sessions.clock().advance(Duration::from_secs(601));
        let fresh = sessions.begin(Principal::new("b")).expect("a session");
        assert_eq!(sessions.sweep(), 1);
        assert_eq!(sessions.len(), 1);
        assert_eq!(sessions.resolve(&live.id), Err(Rejected::Unknown));
        assert!(sessions.resolve(&fresh.id).is_ok());
    }

    #[test]
    fn the_policy_comes_from_the_configured_durations() {
        let config = SessionConfig {
            max_age: "2h".to_owned(),
            idle_timeout: "15m".to_owned(),
            cookie_name: "s".to_owned(),
        };
        let policy = Policy::from(&config);
        assert_eq!(policy.max_age, Duration::from_secs(7_200));
        assert_eq!(policy.idle_timeout, Duration::from_secs(900));
    }
}
