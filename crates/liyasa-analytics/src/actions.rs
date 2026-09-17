//! The actions a dashboard card links to (ANA-20, ANA-30, ANA-40).
//!
//! An insight nobody can act on is a number on a screen. Each of these builds
//! an [`Enqueue`] for the shared job table, so "create a page for this query"
//! and "ask the agent to fix this page" are the same mechanism as every other
//! background job — leased, retried, and visible in the Automations page.
//!
//! The de-duplication key matters as much as the payload. A team looking at
//! the same no-result query on Monday and on Tuesday should not queue the
//! agent twice, and `Jobs::enqueue` keeps one live job per `(name, key)`.

use liyasa_core::ids::ProjectId;
use liyasa_store::jobs::Enqueue;
use serde_json::json;

/// Job names, so a worker and a card cannot disagree by a typo.
pub const CREATE_PAGE: &str = "agent.create_page";
pub const FIX_PAGE: &str = "agent.fix_page";
pub const REFRESH_INSIGHTS: &str = "analytics.insights";
pub const DIGEST: &str = "analytics.digest";
pub const RETENTION: &str = "analytics.retention";

/// ANA-20: "a `create page for this query` action sends a task to the agent".
pub fn create_page_for_query(query: &str, project: Option<ProjectId>) -> Enqueue {
    Enqueue {
        project,
        payload: json!({
            "query": query,
            "reason": "no_result_search",
        }),
        ..Enqueue::new(CREATE_PAGE, format!("query:{query}"))
    }
}

/// ANA-30: "ask the agent to fix" on a piece of feedback.
pub fn fix_page(route: &str, feedback_id: &str, project: Option<ProjectId>) -> Enqueue {
    Enqueue {
        project,
        payload: json!({
            "route": route,
            "feedback_id": feedback_id,
            "reason": "feedback",
        }),
        // Keyed on the route rather than the feedback: five reports of the
        // same broken page are one job.
        ..Enqueue::new(FIX_PAGE, format!("route:{route}"))
    }
}

/// ANA-40: recompute the insight cards. Keyed on the day so a page refresh
/// does not queue a second pass.
pub fn refresh_insights(day: i64, project: Option<ProjectId>) -> Enqueue {
    Enqueue {
        project,
        payload: json!({ "day": day }),
        ..Enqueue::new(REFRESH_INSIGHTS, format!("day:{day}"))
    }
}

/// ANA-42: send the weekly digest.
pub fn send_digest(week_starting: i64, project: Option<ProjectId>) -> Enqueue {
    Enqueue {
        project,
        payload: json!({ "week_starting": week_starting }),
        ..Enqueue::new(DIGEST, format!("week:{week_starting}"))
    }
}

/// ANA-06: run the deletion pass for a day.
pub fn run_retention(day: i64) -> Enqueue {
    Enqueue {
        payload: json!({ "day": day }),
        ..Enqueue::new(RETENTION, format!("day:{day}"))
    }
}
