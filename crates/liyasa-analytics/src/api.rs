//! The HTTP surface the dashboard calls (ANA-70, REST-06).
//!
//! `liyasa-server` owns the routes and this package owns the dashboard that
//! calls them, so the list of paths is the seam between two packages and one
//! of them cannot see the other's. It is written down here, mirrored in
//! `web/dashboard/src/api.ts`, and both are held to
//! `web/dashboard/test/endpoints.fixture.json`.
//!
//! [`ServedBy`] is the part worth reading. Sixteen of these have no handler
//! anywhere yet; the dashboard renders against them and a page whose endpoint
//! is missing says so rather than showing an empty chart, which is the failure
//! this fleet spent a day on.

use serde::{Deserialize, Serialize};

pub const BASE: &str = "/_liyasa/api/v1";

/// Which package's router answers, as of 2026-09-17.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum ServedBy {
    /// Merged into main.
    #[serde(rename = "wp-14")]
    Wp14,
    /// Written and tested on `wp/16-deployments-git`, not yet merged.
    #[serde(rename = "wp-16")]
    Wp16,
    /// `crate::serve::mount`, in this crate, behind the `server` feature — the
    /// handlers WP-17 owns. Reachable once `liyasa-server`'s composition point
    /// registers the subtree (RFC 1704).
    #[serde(rename = "wp-17")]
    Wp17,
    /// No handler exists. The dashboard page that needs it renders its
    /// controls and says the data is not being served.
    Unbuilt,
}

/// Who may call an endpoint.
///
/// Sixteen reads and one write are dashboard operations and sit behind one
/// permission. The published event schema does not: it is the document a
/// collector and a browser client validate their events against *before* they
/// are allowed to post any, so a caller that reaches it by definition has no
/// dashboard credential. A subtree that applies one permission to everything
/// it mounts cannot host both, which is why this is a field rather than a
/// sentence in a comment.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum Auth {
    /// No credential. Readable by anyone who can reach the host.
    Public,
    /// `Permission::DashboardRead` in `liyasa-server`'s role table.
    DashboardRead,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Endpoint {
    pub id: &'static str,
    pub method: &'static str,
    /// `{name}` marks a path parameter, spelled as axum spells it.
    pub path: &'static str,
    pub requirement: &'static str,
    pub served_by: ServedBy,
    pub auth: Auth,
}

pub const ENDPOINTS: &[Endpoint] = &[
    // ANA-02 publishes the event schema here. It is the one route that must
    // stay readable without a dashboard credential.
    Endpoint {
        id: "schema.event",
        method: "GET",
        path: "/_liyasa/schema/event.json",
        requirement: "ANA-02",
        served_by: ServedBy::Wp17,
        auth: Auth::Public,
    },
    Endpoint {
        id: "traffic.series",
        method: "GET",
        path: "/_liyasa/api/v1/analytics/series",
        requirement: "ANA-10",
        served_by: ServedBy::Wp17,
        auth: Auth::DashboardRead,
    },
    Endpoint {
        id: "traffic.totals",
        method: "GET",
        path: "/_liyasa/api/v1/analytics/totals",
        requirement: "ANA-10",
        served_by: ServedBy::Wp17,
        auth: Auth::DashboardRead,
    },
    Endpoint {
        id: "traffic.pages",
        method: "GET",
        path: "/_liyasa/api/v1/analytics/pages",
        requirement: "ANA-10",
        served_by: ServedBy::Wp17,
        auth: Auth::DashboardRead,
    },
    Endpoint {
        id: "traffic.referrers",
        method: "GET",
        path: "/_liyasa/api/v1/analytics/referrers",
        requirement: "ANA-10",
        served_by: ServedBy::Wp17,
        auth: Auth::DashboardRead,
    },
    Endpoint {
        id: "traffic.journeys",
        method: "GET",
        path: "/_liyasa/api/v1/analytics/journeys",
        requirement: "ANA-10",
        served_by: ServedBy::Wp17,
        auth: Auth::DashboardRead,
    },
    Endpoint {
        id: "traffic.variants",
        method: "GET",
        path: "/_liyasa/api/v1/analytics/variants",
        requirement: "ANA-10",
        served_by: ServedBy::Wp17,
        auth: Auth::DashboardRead,
    },
    Endpoint {
        id: "traffic.delivery",
        method: "GET",
        path: "/_liyasa/api/v1/analytics/delivery",
        requirement: "ANA-10",
        served_by: ServedBy::Wp17,
        auth: Auth::DashboardRead,
    },
    Endpoint {
        id: "traffic.horizon",
        method: "GET",
        path: "/_liyasa/api/v1/analytics/horizon",
        requirement: "ANA-06",
        served_by: ServedBy::Wp17,
        auth: Auth::DashboardRead,
    },
    Endpoint {
        id: "search.queries",
        method: "GET",
        path: "/_liyasa/api/v1/analytics/search/queries",
        requirement: "ANA-20",
        served_by: ServedBy::Wp17,
        auth: Auth::DashboardRead,
    },
    Endpoint {
        id: "search.pages",
        method: "GET",
        path: "/_liyasa/api/v1/analytics/search/pages",
        requirement: "ANA-20",
        served_by: ServedBy::Wp17,
        auth: Auth::DashboardRead,
    },
    Endpoint {
        id: "search.trending",
        method: "GET",
        path: "/_liyasa/api/v1/analytics/search/trending",
        requirement: "ANA-20",
        served_by: ServedBy::Wp17,
        auth: Auth::DashboardRead,
    },
    Endpoint {
        id: "assistant.summary",
        method: "GET",
        path: "/_liyasa/api/v1/analytics/assistant",
        requirement: "ANA-10",
        served_by: ServedBy::Wp17,
        auth: Auth::DashboardRead,
    },
    Endpoint {
        id: "insights.cards",
        method: "GET",
        path: "/_liyasa/api/v1/analytics/insights",
        requirement: "ANA-40",
        served_by: ServedBy::Wp17,
        auth: Auth::DashboardRead,
    },
    Endpoint {
        id: "insights.act",
        method: "POST",
        path: "/_liyasa/api/v1/analytics/insights/act",
        requirement: "ANA-40",
        served_by: ServedBy::Wp17,
        auth: Auth::DashboardRead,
    },
    Endpoint {
        id: "settings.integrations",
        method: "GET",
        path: "/_liyasa/api/v1/analytics/integrations",
        requirement: "ANA-60",
        served_by: ServedBy::Wp17,
        auth: Auth::DashboardRead,
    },
    Endpoint {
        id: "feedback.ratings",
        method: "GET",
        path: "/_liyasa/api/v1/analytics/feedback/ratings",
        requirement: "ANA-30",
        served_by: ServedBy::Wp17,
        auth: Auth::DashboardRead,
    },
    Endpoint {
        id: "feedback.pages",
        method: "GET",
        path: "/_liyasa/api/v1/analytics/feedback/pages",
        requirement: "ANA-30",
        served_by: ServedBy::Wp17,
        auth: Auth::DashboardRead,
    },
    Endpoint {
        id: "content.tree",
        method: "GET",
        path: "/_liyasa/api/v1/content",
        requirement: "REST-02",
        served_by: ServedBy::Wp14,
        auth: Auth::DashboardRead,
    },
    Endpoint {
        id: "feedback.list",
        method: "GET",
        path: "/_liyasa/feedback",
        requirement: "ANA-30",
        served_by: ServedBy::Wp14,
        auth: Auth::DashboardRead,
    },
    Endpoint {
        id: "feedback.summary",
        method: "GET",
        path: "/_liyasa/feedback/summary",
        requirement: "ANA-30",
        served_by: ServedBy::Wp14,
        auth: Auth::DashboardRead,
    },
    Endpoint {
        id: "feedback.status",
        method: "PATCH",
        path: "/_liyasa/feedback/{id}",
        requirement: "ANA-30",
        served_by: ServedBy::Wp14,
        auth: Auth::DashboardRead,
    },
    Endpoint {
        id: "jobs.list",
        method: "GET",
        path: "/_liyasa/api/v1/jobs",
        requirement: "HOST-07",
        served_by: ServedBy::Wp14,
        auth: Auth::DashboardRead,
    },
    Endpoint {
        id: "jobs.retry",
        method: "POST",
        path: "/_liyasa/api/v1/jobs/{id}/retry",
        requirement: "HOST-07",
        served_by: ServedBy::Wp14,
        auth: Auth::DashboardRead,
    },
    Endpoint {
        id: "jobs.cancel",
        method: "POST",
        path: "/_liyasa/api/v1/jobs/{id}/cancel",
        requirement: "HOST-07",
        served_by: ServedBy::Wp14,
        auth: Auth::DashboardRead,
    },
    Endpoint {
        id: "deployments.list",
        method: "GET",
        path: "/_liyasa/api/v1/deployments",
        requirement: "REST-01",
        served_by: ServedBy::Wp14,
        auth: Auth::DashboardRead,
    },
    Endpoint {
        id: "deployments.current",
        method: "GET",
        path: "/_liyasa/api/v1/deployments/{env}",
        requirement: "REST-01",
        served_by: ServedBy::Wp14,
        auth: Auth::DashboardRead,
    },
    Endpoint {
        id: "builds.trigger",
        method: "POST",
        path: "/_liyasa/api/v1/builds",
        requirement: "GIT-21",
        served_by: ServedBy::Wp16,
        auth: Auth::DashboardRead,
    },
    Endpoint {
        id: "builds.queue",
        method: "GET",
        path: "/_liyasa/api/v1/builds",
        requirement: "GIT-24",
        served_by: ServedBy::Wp16,
        auth: Auth::DashboardRead,
    },
    Endpoint {
        id: "builds.status",
        method: "GET",
        path: "/_liyasa/api/v1/builds/{id}",
        requirement: "GIT-21",
        served_by: ServedBy::Wp16,
        auth: Auth::DashboardRead,
    },
    Endpoint {
        id: "builds.activate",
        method: "POST",
        path: "/_liyasa/api/v1/builds/{id}/deploy",
        requirement: "GIT-21",
        served_by: ServedBy::Wp16,
        auth: Auth::DashboardRead,
    },
    Endpoint {
        id: "deployments.history",
        method: "GET",
        path: "/_liyasa/api/v1/deployments/{env}/history",
        requirement: "GIT-21",
        served_by: ServedBy::Wp16,
        auth: Auth::DashboardRead,
    },
    Endpoint {
        id: "deployments.retained",
        method: "GET",
        path: "/_liyasa/api/v1/deployments/{env}/retained",
        requirement: "GIT-40",
        served_by: ServedBy::Wp16,
        auth: Auth::DashboardRead,
    },
    Endpoint {
        id: "deployments.rollback",
        method: "POST",
        path: "/_liyasa/api/v1/deployments/{env}/rollback/{buildId}",
        requirement: "GIT-40",
        served_by: ServedBy::Wp16,
        auth: Auth::DashboardRead,
    },
    Endpoint {
        id: "deployments.latest",
        method: "POST",
        path: "/_liyasa/api/v1/deployments/{env}/latest",
        requirement: "GIT-41",
        served_by: ServedBy::Wp16,
        auth: Auth::DashboardRead,
    },
    Endpoint {
        id: "drift.open",
        method: "GET",
        path: "/_liyasa/api/v1/drift",
        requirement: "REST-05",
        served_by: ServedBy::Unbuilt,
        auth: Auth::DashboardRead,
    },
    Endpoint {
        id: "proposals.list",
        method: "GET",
        path: "/_liyasa/api/v1/proposals",
        requirement: "REST-02",
        served_by: ServedBy::Unbuilt,
        auth: Auth::DashboardRead,
    },
];

pub fn endpoint(id: &str) -> Option<&'static Endpoint> {
    ENDPOINTS.iter().find(|e| e.id == id)
}

/// Endpoints nobody serves yet, so a release note can say which pages are
/// still shells.
pub fn unbuilt() -> impl Iterator<Item = &'static Endpoint> {
    ENDPOINTS
        .iter()
        .filter(|e| e.served_by == ServedBy::Unbuilt)
}

/// The endpoints one subtree may mount behind a single permission.
pub fn behind(auth: Auth) -> impl Iterator<Item = &'static Endpoint> {
    ENDPOINTS.iter().filter(move |e| e.auth == auth)
}
