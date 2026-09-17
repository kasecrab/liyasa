//! The audit log (ORG-20): who did what, when, from where, and what the thing
//! looked like before and after.
//!
//! The log is bounded, because it is in memory (RFC 2800). A bounded log that
//! quietly drops its oldest entries is an audit log that lies, so the count of
//! what was dropped is carried beside the entries and appears in every export
//! and every search result. An operator reading an export can tell the
//! difference between "nothing happened before this" and "we stopped keeping
//! it".

use std::collections::VecDeque;
use std::net::IpAddr;

use serde::{Deserialize, Serialize};
use serde_json::Value;

/// The surfaces ORG-20 names, in its own order.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum Area {
    ContentPublish,
    Settings,
    Members,
    Domains,
    Keys,
    Deployments,
    Rollbacks,
    AutomationRuns,
    AgentProposals,
}

impl Area {
    pub const ALL: &'static [Area] = &[
        Area::ContentPublish,
        Area::Settings,
        Area::Members,
        Area::Domains,
        Area::Keys,
        Area::Deployments,
        Area::Rollbacks,
        Area::AutomationRuns,
        Area::AgentProposals,
    ];

    pub fn as_str(self) -> &'static str {
        match self {
            Area::ContentPublish => "contentPublish",
            Area::Settings => "settings",
            Area::Members => "members",
            Area::Domains => "domains",
            Area::Keys => "keys",
            Area::Deployments => "deployments",
            Area::Rollbacks => "rollbacks",
            Area::AutomationRuns => "automationRuns",
            Area::AgentProposals => "agentProposals",
        }
    }

    pub fn parse(text: &str) -> Option<Area> {
        Area::ALL
            .iter()
            .copied()
            .find(|a| a.as_str().eq_ignore_ascii_case(text))
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum ActorKind {
    User,
    Token,
    Automation,
    Agent,
    /// The server itself: a scheduled rollback, a quota reset. Never "unknown"
    /// — an entry with no actor at all would be the one worth reading.
    System,
}

impl ActorKind {
    pub fn as_str(self) -> &'static str {
        match self {
            ActorKind::User => "user",
            ActorKind::Token => "token",
            ActorKind::Automation => "automation",
            ActorKind::Agent => "agent",
            ActorKind::System => "system",
        }
    }
}

/// ORG-20's "who".
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Actor {
    pub id: String,
    pub kind: ActorKind,
    pub email: Option<String>,
}

impl Actor {
    pub fn user(id: impl Into<String>, email: impl Into<String>) -> Self {
        Self {
            id: id.into(),
            kind: ActorKind::User,
            email: Some(email.into()),
        }
    }

    pub fn system() -> Self {
        Self {
            id: "system".to_owned(),
            kind: ActorKind::System,
            email: None,
        }
    }
}

/// ORG-20's "from where".
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct Origin {
    pub ip: Option<IpAddr>,
    pub user_agent: Option<String>,
}

impl Origin {
    pub fn from(ip: IpAddr) -> Self {
        Self {
            ip: Some(ip),
            user_agent: None,
        }
    }

    pub fn with_user_agent(mut self, user_agent: impl Into<String>) -> Self {
        self.user_agent = Some(user_agent.into());
        self
    }

    fn describe(&self) -> String {
        match (&self.ip, &self.user_agent) {
            (Some(ip), Some(agent)) => format!("{ip} ({agent})"),
            (Some(ip), None) => ip.to_string(),
            (None, Some(agent)) => agent.clone(),
            (None, None) => String::new(),
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Entry {
    pub id: u64,
    pub at_ms: i64,
    pub actor: Actor,
    pub area: Area,
    /// What was done, as a verb: `publish`, `role.change`, `domain.add`.
    pub action: String,
    /// What it was done to.
    pub object: String,
    pub project: Option<String>,
    pub origin: Origin,
    /// Absent when the object did not exist before.
    pub before: Option<Value>,
    /// Absent when the object no longer exists.
    pub after: Option<Value>,
}

impl Entry {
    fn matches_text(&self, needle: &str) -> bool {
        let needle = needle.to_lowercase();
        self.action.to_lowercase().contains(&needle)
            || self.object.to_lowercase().contains(&needle)
            || self.actor.id.to_lowercase().contains(&needle)
            || self
                .actor
                .email
                .as_deref()
                .is_some_and(|e| e.to_lowercase().contains(&needle))
            || self
                .project
                .as_deref()
                .is_some_and(|p| p.to_lowercase().contains(&needle))
    }
}

/// What to record. The log stamps the id and the time.
#[derive(Debug, Clone)]
pub struct Record {
    pub actor: Actor,
    pub area: Area,
    pub action: String,
    pub object: String,
    pub project: Option<String>,
    pub origin: Origin,
    pub before: Option<Value>,
    pub after: Option<Value>,
}

impl Record {
    pub fn new(
        actor: Actor,
        area: Area,
        action: impl Into<String>,
        object: impl Into<String>,
    ) -> Self {
        Self {
            actor,
            area,
            action: action.into(),
            object: object.into(),
            project: None,
            origin: Origin::default(),
            before: None,
            after: None,
        }
    }

    pub fn in_project(mut self, project: impl Into<String>) -> Self {
        self.project = Some(project.into());
        self
    }

    pub fn from(mut self, origin: Origin) -> Self {
        self.origin = origin;
        self
    }

    pub fn changing(mut self, before: Value, after: Value) -> Self {
        self.before = Some(before);
        self.after = Some(after);
        self
    }

    pub fn creating(mut self, after: Value) -> Self {
        self.after = Some(after);
        self
    }

    pub fn removing(mut self, before: Value) -> Self {
        self.before = Some(before);
        self
    }
}

/// ORG-20's search. Every field is a narrowing; an empty query matches
/// everything.
#[derive(Debug, Clone, Default)]
pub struct Query {
    pub actor: Option<String>,
    pub area: Option<Area>,
    pub project: Option<String>,
    pub object: Option<String>,
    pub since_ms: Option<i64>,
    pub until_ms: Option<i64>,
    /// Free text across the actor, the action, the object and the project.
    pub text: Option<String>,
    pub limit: Option<usize>,
}

impl Query {
    fn admits(&self, entry: &Entry) -> bool {
        if let Some(actor) = &self.actor
            && &entry.actor.id != actor
            && entry.actor.email.as_deref() != Some(actor.as_str())
        {
            return false;
        }
        if let Some(area) = self.area
            && entry.area != area
        {
            return false;
        }
        if let Some(project) = &self.project
            && entry.project.as_deref() != Some(project.as_str())
        {
            return false;
        }
        if let Some(object) = &self.object
            && &entry.object != object
        {
            return false;
        }
        if let Some(since) = self.since_ms
            && entry.at_ms < since
        {
            return false;
        }
        // Exclusive, so a page that asks for the next window with the previous
        // one's last timestamp does not repeat an entry.
        if let Some(until) = self.until_ms
            && entry.at_ms >= until
        {
            return false;
        }
        if let Some(text) = &self.text
            && !entry.matches_text(text)
        {
            return false;
        }
        true
    }
}

/// How many entries one organization's log holds before the oldest is dropped.
pub const CAPACITY: usize = 10_000;

#[derive(Debug, Clone)]
pub struct Log {
    entries: VecDeque<Entry>,
    capacity: usize,
    next_id: u64,
    dropped: u64,
}

impl Default for Log {
    fn default() -> Self {
        Log::with_capacity(CAPACITY)
    }
}

impl Log {
    pub fn with_capacity(capacity: usize) -> Self {
        Self {
            entries: VecDeque::new(),
            capacity: capacity.max(1),
            next_id: 1,
            dropped: 0,
        }
    }

    /// Entries that were evicted to stay inside the capacity. Non-zero means
    /// this log is no longer a complete record, and every export says so.
    pub fn dropped(&self) -> u64 {
        self.dropped
    }

    pub fn len(&self) -> usize {
        self.entries.len()
    }

    pub fn is_empty(&self) -> bool {
        self.entries.is_empty()
    }

    pub fn record(&mut self, at_ms: i64, record: Record) -> u64 {
        let id = self.next_id;
        self.next_id += 1;
        self.entries.push_back(Entry {
            id,
            at_ms,
            actor: record.actor,
            area: record.area,
            action: record.action,
            object: record.object,
            project: record.project,
            origin: record.origin,
            before: record.before,
            after: record.after,
        });
        while self.entries.len() > self.capacity {
            self.entries.pop_front();
            self.dropped += 1;
        }
        id
    }

    /// Newest first, which is the order every reader of an audit log wants.
    pub fn search(&self, query: &Query) -> Vec<&Entry> {
        let mut found: Vec<&Entry> = self
            .entries
            .iter()
            .rev()
            .filter(|entry| query.admits(entry))
            .collect();
        if let Some(limit) = query.limit {
            found.truncate(limit);
        }
        found
    }

    pub fn entries(&self) -> impl Iterator<Item = &Entry> {
        self.entries.iter()
    }

    /// ORG-20's JSON export.
    pub fn to_json(&self, query: &Query) -> Value {
        serde_json::json!({
            "entries": self.search(query),
            "dropped": self.dropped,
            "complete": self.dropped == 0,
        })
    }

    /// ORG-20's CSV export, RFC 4180. `before` and `after` are compact JSON in
    /// one column each: an audit row whose diff was flattened away is not an
    /// audit row.
    pub fn to_csv(&self, query: &Query) -> String {
        let mut out =
            String::from("id,at,actor,actorKind,area,action,object,project,origin,before,after\n");
        for entry in self.search(query) {
            let row = [
                entry.id.to_string(),
                entry.at_ms.to_string(),
                entry
                    .actor
                    .email
                    .clone()
                    .unwrap_or_else(|| entry.actor.id.clone()),
                entry.actor.kind.as_str().to_owned(),
                entry.area.as_str().to_owned(),
                entry.action.clone(),
                entry.object.clone(),
                entry.project.clone().unwrap_or_default(),
                entry.origin.describe(),
                entry
                    .before
                    .as_ref()
                    .map(Value::to_string)
                    .unwrap_or_default(),
                entry
                    .after
                    .as_ref()
                    .map(Value::to_string)
                    .unwrap_or_default(),
            ];
            let cells: Vec<String> = row.iter().map(|cell| csv_cell(cell)).collect();
            out.push_str(&cells.join(","));
            out.push('\n');
        }
        if self.dropped > 0 {
            out.push_str(&format!(
                "# {} earlier entries were dropped to stay inside this log's capacity\n",
                self.dropped
            ));
        }
        out
    }
}

/// RFC 4180: a field containing a comma, a quote or a newline is quoted, and
/// quotes inside it are doubled.
fn csv_cell(value: &str) -> String {
    if value.contains([',', '"', '\n', '\r']) {
        format!("\"{}\"", value.replace('"', "\"\""))
    } else {
        value.to_owned()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::net::{IpAddr, Ipv4Addr};

    fn origin() -> Origin {
        Origin::from(IpAddr::V4(Ipv4Addr::new(203, 0, 113, 7))).with_user_agent("liyasa-cli/0.1")
    }

    fn log() -> Log {
        let mut log = Log::default();
        log.record(
            1_000,
            Record::new(
                Actor::user("u1", "ana@acme.com"),
                Area::Members,
                "role.change",
                "u2",
            )
            .from(origin())
            .changing(
                serde_json::json!({ "role": "viewer" }),
                serde_json::json!({ "role": "editor" }),
            ),
        );
        log.record(
            2_000,
            Record::new(
                Actor::user("u2", "bo@acme.com"),
                Area::ContentPublish,
                "publish",
                "/guides/install",
            )
            .in_project("docs")
            .from(origin())
            .creating(serde_json::json!({ "build": "b_42" })),
        );
        log.record(
            3_000,
            Record::new(Actor::system(), Area::Rollbacks, "rollback", "production")
                .in_project("docs")
                .changing(
                    serde_json::json!({ "build": "b_42" }),
                    serde_json::json!({ "build": "b_41" }),
                ),
        );
        log
    }

    #[test]
    fn an_entry_carries_who_what_when_where_and_both_sides_of_the_change() {
        // ORG-20 lists six things; an entry missing any of them is not one.
        let log = log();
        let entry = log.entries().next().expect("the first entry");
        assert_eq!(entry.actor.email.as_deref(), Some("ana@acme.com"));
        assert_eq!(entry.action, "role.change");
        assert_eq!(entry.at_ms, 1_000);
        assert_eq!(
            entry.origin.ip.map(|ip| ip.to_string()).as_deref(),
            Some("203.0.113.7")
        );
        assert_eq!(entry.before.as_ref().expect("before")["role"], "viewer");
        assert_eq!(entry.after.as_ref().expect("after")["role"], "editor");
    }

    #[test]
    fn a_creation_has_no_before_and_a_removal_has_no_after() {
        let mut log = Log::default();
        log.record(
            1,
            Record::new(Actor::system(), Area::Keys, "key.issue", "cred_1")
                .creating(serde_json::json!({ "label": "ci" })),
        );
        log.record(
            2,
            Record::new(Actor::system(), Area::Keys, "key.revoke", "cred_1")
                .removing(serde_json::json!({ "label": "ci" })),
        );
        let entries: Vec<&Entry> = log.entries().collect();
        assert!(entries[0].before.is_none() && entries[0].after.is_some());
        assert!(entries[1].before.is_some() && entries[1].after.is_none());
    }

    #[test]
    fn search_narrows_by_area_actor_project_and_time() {
        let log = log();
        assert_eq!(log.search(&Query::default()).len(), 3);
        assert_eq!(
            log.search(&Query {
                area: Some(Area::Members),
                ..Query::default()
            })
            .len(),
            1
        );
        assert_eq!(
            log.search(&Query {
                actor: Some("bo@acme.com".to_owned()),
                ..Query::default()
            })
            .len(),
            1,
            "an actor is searchable by email as well as by id"
        );
        assert_eq!(
            log.search(&Query {
                project: Some("docs".to_owned()),
                ..Query::default()
            })
            .len(),
            2
        );
        assert_eq!(
            log.search(&Query {
                since_ms: Some(2_000),
                until_ms: Some(3_000),
                ..Query::default()
            })
            .len(),
            1,
            "`until` is exclusive so paging does not repeat an entry"
        );
    }

    #[test]
    fn search_returns_the_newest_first_and_honours_a_limit() {
        let log = log();
        let found = log.search(&Query {
            limit: Some(2),
            ..Query::default()
        });
        assert_eq!(found.len(), 2);
        assert_eq!(found[0].at_ms, 3_000);
        assert_eq!(found[1].at_ms, 2_000);
    }

    #[test]
    fn free_text_searches_the_actor_the_action_the_object_and_the_project() {
        let log = log();
        for needle in ["ana", "ROLLBACK", "/guides/install", "docs"] {
            assert!(
                !log.search(&Query {
                    text: Some(needle.to_owned()),
                    ..Query::default()
                })
                .is_empty(),
                "`{needle}` found nothing"
            );
        }
        assert!(
            log.search(&Query {
                text: Some("nothing here".to_owned()),
                ..Query::default()
            })
            .is_empty()
        );
    }

    #[test]
    fn a_csv_export_quotes_a_field_that_would_otherwise_break_the_row() {
        // A JSON diff is full of commas and quotes, so this is the common
        // case rather than the edge one.
        let mut log = Log::default();
        log.record(
            1,
            Record::new(Actor::system(), Area::Settings, "settings.update", "name").changing(
                serde_json::json!({ "name": "Acme, Inc." }),
                serde_json::json!({ "name": "Acme \"Docs\"" }),
            ),
        );
        let csv = log.to_csv(&Query::default());
        let row = csv.lines().nth(1).expect("one row");
        assert!(
            row.contains("\"{\"\"name\"\":\"\"Acme, Inc.\"\"}\""),
            "{row}"
        );
        assert_eq!(
            row.matches('"').count() % 2,
            0,
            "an unbalanced quote breaks every row after it: {row}"
        );
        assert_eq!(csv.lines().count(), 2, "a header and one row");
    }

    #[test]
    fn a_json_export_carries_the_same_entries_the_search_found() {
        let log = log();
        let query = Query {
            area: Some(Area::ContentPublish),
            ..Query::default()
        };
        let json = log.to_json(&query);
        assert_eq!(json["entries"].as_array().expect("entries").len(), 1);
        assert_eq!(json["entries"][0]["action"], "publish");
        assert_eq!(json["complete"], true);
        assert_eq!(
            json["entries"].as_array().expect("entries").len(),
            log.search(&query).len()
        );
    }

    #[test]
    fn a_log_that_dropped_entries_says_so_in_both_exports() {
        // The defect this exists against: an export that looks complete and
        // is not is worse than no export at all.
        let mut log = Log::with_capacity(2);
        for n in 0..5 {
            log.record(
                n,
                Record::new(Actor::system(), Area::Settings, "settings.update", "name"),
            );
        }
        assert_eq!(log.len(), 2);
        assert_eq!(log.dropped(), 3);
        assert_eq!(log.to_json(&Query::default())["dropped"], 3);
        assert_eq!(log.to_json(&Query::default())["complete"], false);
        assert!(
            log.to_csv(&Query::default())
                .contains("3 earlier entries were dropped"),
            "the CSV export must say so too"
        );
    }

    #[test]
    fn every_area_org_20_names_round_trips_through_its_name() {
        for area in Area::ALL {
            assert_eq!(Area::parse(area.as_str()), Some(*area));
        }
        assert_eq!(Area::ALL.len(), 9, "ORG-20 names nine surfaces");
    }
}
