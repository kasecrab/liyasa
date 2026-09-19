//! Everything one instance knows about its organization, behind one lock.
//!
//! RFC 2800: this is in memory rather than in `liyasa-store`, because
//! `crates/liyasa-store/` and `migrations/` are another package's paths. The
//! consequences are written down there rather than discovered later — a second
//! replica sees none of this, and a restart loses the audit log.

use std::sync::{Arc, RwLock, RwLockReadGuard, RwLockWriteGuard, Weak};

use crate::auth::clock::Clock;
use crate::routes::AppState;

use super::audit::{Log, Record};
use super::credits::Ledger;
use super::meter::Meter;
use super::model::{Organization, Settings};
use super::notify::{Endpoints, Subscriber};
use super::plan::Plan;
use super::region::Region;

#[derive(Debug)]
pub struct Inner {
    pub org: Organization,
    pub meter: Meter,
    pub ledger: Ledger,
    pub log: Log,
    pub endpoints: Endpoints,
    pub subscribers: Vec<Subscriber>,
}

#[derive(Debug)]
pub struct OrgState {
    inner: RwLock<Inner>,
    clock: Clock,
    /// The server this subtree is part of, for the client address an audit
    /// entry records and for the trusted-proxy rules that decide whether a
    /// forwarded header may be believed at all.
    ///
    /// `Weak`, and it has to be. Once the role source is wired the references
    /// form a ring: `AppState` holds `AuthState`, which holds the
    /// `Arc<dyn Roles>`, which is a `MembershipRoles` holding this
    /// `OrgState`. A strong link back closes it, and a cycle of `Arc`s never
    /// drops — no panic and no failing assertion, just the whole server
    /// state, the bundle and the store handle leaked for the life of the
    /// process, and one leak per harness in the test suite. `AppState`
    /// already keeps `self_arc` weak for the same reason.
    app: Option<Weak<AppState>>,
}

impl OrgState {
    pub fn new(id: &str, settings: Settings, plan: Plan) -> Self {
        let clock = Clock::default();
        OrgState::with_clock(id, settings, plan, clock)
    }

    pub fn with_clock(id: &str, settings: Settings, plan: Plan, clock: Clock) -> Self {
        let org = Organization::new(id, settings, plan.clone()).with_clock(clock.clone());
        Self {
            inner: RwLock::new(Inner {
                org,
                meter: Meter::new(plan.clone()),
                ledger: Ledger::new(plan.monthly_credits),
                log: Log::default(),
                endpoints: Endpoints::default(),
                subscribers: Vec::new(),
            }),
            clock,
            app: None,
        }
    }

    pub fn with_app(mut self, app: &Arc<AppState>) -> Self {
        self.app = Some(Arc::downgrade(app));
        self
    }

    /// `None` once the server it belongs to has been dropped, which in
    /// practice means during teardown. A caller that gets `None` should do
    /// whatever it does without a server rather than treat it as a failure.
    pub fn app(&self) -> Option<Arc<AppState>> {
        self.app.as_ref().and_then(Weak::upgrade)
    }

    pub fn clock(&self) -> &Clock {
        &self.clock
    }

    /// A poisoned lock means a handler panicked while holding it. Recovering
    /// the guard keeps the server answering: the alternative is that one
    /// panicked request takes the whole organization API down until a restart.
    pub fn read(&self) -> RwLockReadGuard<'_, Inner> {
        self.inner.read().unwrap_or_else(|e| e.into_inner())
    }

    pub fn write(&self) -> RwLockWriteGuard<'_, Inner> {
        self.inner.write().unwrap_or_else(|e| e.into_inner())
    }

    /// ORG-20: the one way a state change is recorded, so a handler cannot
    /// change something and forget the entry — it takes the same lock.
    pub fn audit(&self, record: Record) -> u64 {
        let at = self.clock.now_ms();
        self.write().log.record(at, record)
    }

    /// What `mount` builds from the server's own configuration. ORG-32: the
    /// plan is `unlimited`, because an instance an operator runs is not
    /// metered by us. A cloud control plane sets a tier through the plan
    /// endpoint; there is no config key for it (RFC 2800).
    pub fn from_app(app: &Arc<AppState>) -> Self {
        let name = app
            .config
            .site_config
            .get("name")
            .and_then(serde_json::Value::as_str)
            .unwrap_or(app.config.site.as_str());
        let mut settings = Settings::new(name);
        settings.default_region = Region::default();
        OrgState::new(&app.config.site, settings, Plan::unlimited()).with_app(app)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::routes::ServerConfig;

    use crate::org::audit::{Actor, Area};
    use crate::org::plan::Tier;

    #[test]
    fn an_instance_built_from_the_server_is_unlimited_and_named_after_the_site() {
        // ORG-32: no configuration says "unlimited"; it is what an instance
        // that was never told otherwise runs.
        let app = Arc::new(AppState::new(ServerConfig {
            site: "acme".to_owned(),
            site_config: Arc::new(serde_json::json!({ "name": "Acme docs" })),
            ..ServerConfig::default()
        }));
        let state = OrgState::from_app(&app);
        let inner = state.read();
        assert_eq!(inner.org.id, "acme");
        assert_eq!(inner.org.settings.name, "Acme docs");
        assert_eq!(inner.org.plan.tier, Tier::Unlimited);
        assert_eq!(
            inner.ledger.pool, None,
            "an OSS instance has no credit pool"
        );
    }

    #[test]
    fn a_site_with_no_name_falls_back_to_its_slug() {
        let app = Arc::new(AppState::new(ServerConfig {
            site: "acme".to_owned(),
            ..ServerConfig::default()
        }));
        assert_eq!(OrgState::from_app(&app).read().org.settings.name, "acme");
    }

    #[test]
    fn the_organization_does_not_keep_the_server_alive() {
        // Once the role source is wired the references form a ring:
        // AppState -> AuthState -> Arc<dyn Roles> -> OrgState -> AppState.
        // A strong link here closes it and nothing in the ring ever drops.
        // Falsified before it was trusted: making `app` an `Arc` again fails
        // this with the weak reference still upgrading.
        let app = Arc::new(AppState::new(ServerConfig {
            site: "acme".to_owned(),
            ..ServerConfig::default()
        }));
        let watch = Arc::downgrade(&app);
        let state = OrgState::from_app(&app);
        assert!(state.app().is_some(), "and it can still reach it meanwhile");

        drop(app);
        assert!(
            watch.upgrade().is_none(),
            "the organization outlived the server it belongs to"
        );
        // The organization is still usable; it simply has no server to ask.
        assert!(state.app().is_none());
        assert_eq!(state.read().org.id, "acme");
    }

    #[test]
    fn an_audit_entry_is_stamped_with_the_state_s_clock() {
        let state = OrgState::with_clock(
            "acme",
            Settings::new("Acme"),
            Plan::unlimited(),
            Clock::manual(),
        );
        let at = state.clock().now_ms();
        let id = state.audit(Record::new(
            Actor::system(),
            Area::Settings,
            "settings.update",
            "name",
        ));
        assert_eq!(id, 1);
        assert_eq!(
            state.read().log.entries().next().expect("an entry").at_ms,
            at
        );
    }
}
