//! Feedback and ratings (ANA-30).
//!
//! Feedback rows live in the application database rather than the analytics
//! one — they are editable records with a status workflow and internal notes,
//! not append-only events — so everything here takes the application pool.
//! `liyasa_store::repos::Feedback` owns the writes; this is the reading.
//!
//! Agent feedback (`kind = 'agent'`, RX-52) is kept separate throughout rather
//! than folded into the ratings, because an agent reporting that a page failed
//! its task is a different signal from a reader's thumb and mixing them makes
//! both unreadable.

use liyasa_core::store::StoreError;
use liyasa_store::db::sql_error;
use liyasa_store::records::{FeedbackKind, FeedbackRecord, FeedbackStatus};
use serde::{Deserialize, Serialize};
use sqlx::Row;
use sqlx::sqlite::SqlitePool;

use crate::query::{Grain, Range};
use crate::sql::{bind_all, sql};

/// Up and down over one bucket, for the per-page rating chart.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct RatingPoint {
    pub bucket: i64,
    pub up: i64,
    pub down: i64,
}

impl RatingPoint {
    pub fn total(&self) -> i64 {
        self.up + self.down
    }

    /// `None` when nobody rated: a page with no votes is not a page rated 0%.
    pub fn score(&self) -> Option<f64> {
        (self.total() > 0).then(|| self.up as f64 / self.total() as f64)
    }
}

/// A page's standing over a window.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct PageRating {
    pub route: String,
    pub up: i64,
    pub down: i64,
    /// Reports from agents, counted but never scored.
    pub agent_reports: i64,
    pub open: i64,
}

impl PageRating {
    pub fn total(&self) -> i64 {
        self.up + self.down
    }

    pub fn score(&self) -> Option<f64> {
        (self.total() > 0).then(|| self.up as f64 / self.total() as f64)
    }
}

/// What the feedback page filters by (ANA-30).
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(default, rename_all = "camelCase")]
pub struct FeedbackFilters {
    pub route_prefix: Option<String>,
    pub kind: Option<FeedbackKind>,
    pub status: Option<FeedbackStatus>,
    /// `category` on the row: what the reader picked from the list.
    pub category: Option<String>,
    /// Only rows with written text, which is the triage queue.
    pub with_text: bool,
}

fn kind_text(kind: FeedbackKind) -> &'static str {
    match kind {
        FeedbackKind::Page => "page",
        FeedbackKind::Code => "code",
        FeedbackKind::Agent => "agent",
    }
}

fn status_text(status: FeedbackStatus) -> &'static str {
    match status {
        FeedbackStatus::Open => "open",
        FeedbackStatus::Triaged => "triaged",
        FeedbackStatus::Resolved => "resolved",
    }
}

fn kind_of(text: &str) -> Result<FeedbackKind, StoreError> {
    Ok(match text {
        "page" => FeedbackKind::Page,
        "code" => FeedbackKind::Code,
        "agent" => FeedbackKind::Agent,
        other => return Err(StoreError::Sql(format!("unknown feedback kind `{other}`"))),
    })
}

fn status_of(text: &str) -> Result<FeedbackStatus, StoreError> {
    Ok(match text {
        "open" => FeedbackStatus::Open,
        "triaged" => FeedbackStatus::Triaged,
        "resolved" => FeedbackStatus::Resolved,
        other => {
            return Err(StoreError::Sql(format!(
                "unknown feedback status `{other}`"
            )));
        }
    })
}

fn escape_like(text: &str) -> String {
    let mut out = String::with_capacity(text.len());
    for c in text.chars() {
        if matches!(c, '%' | '_' | '\\') {
            out.push('\\');
        }
        out.push(c);
    }
    out
}

impl FeedbackFilters {
    fn predicate(&self) -> (String, Vec<String>) {
        let mut sql = String::new();
        let mut binds = Vec::new();
        if let Some(prefix) = &self.route_prefix {
            sql.push_str(" AND route LIKE ? ESCAPE '\\'");
            binds.push(format!("{}%", escape_like(prefix)));
        }
        if let Some(kind) = self.kind {
            sql.push_str(" AND kind = ?");
            binds.push(kind_text(kind).to_owned());
        }
        if let Some(status) = self.status {
            sql.push_str(" AND status = ?");
            binds.push(status_text(status).to_owned());
        }
        if let Some(category) = &self.category {
            sql.push_str(" AND category = ?");
            binds.push(category.clone());
        }
        if self.with_text {
            sql.push_str(" AND text IS NOT NULL AND text <> ''");
        }
        (sql, binds)
    }
}

fn record_from_row(row: &sqlx::sqlite::SqliteRow) -> Result<FeedbackRecord, StoreError> {
    let kind: String = row.try_get("kind").map_err(sql_error)?;
    let status: String = row.try_get("status").map_err(sql_error)?;
    let project: Option<String> = row.try_get("project_id").map_err(sql_error)?;
    Ok(FeedbackRecord {
        id: row.try_get("id").map_err(sql_error)?,
        project: project.and_then(|p| liyasa_core::ids::ProjectId::parse(&p)),
        route: row.try_get("route").map_err(sql_error)?,
        kind: kind_of(&kind)?,
        rating: row.try_get("rating").map_err(sql_error)?,
        category: row.try_get("category").map_err(sql_error)?,
        text: row.try_get("text").map_err(sql_error)?,
        block_id: row.try_get("block_id").map_err(sql_error)?,
        task: row.try_get("task").map_err(sql_error)?,
        status: status_of(&status)?,
        notes: row.try_get("notes").map_err(sql_error)?,
        created_at: row.try_get("created_at").map_err(sql_error)?,
        updated_at: row.try_get("updated_at").map_err(sql_error)?,
    })
}

/// The written feedback list (ANA-30), newest first.
pub async fn list(
    pool: &SqlitePool,
    range: Range,
    filters: &FeedbackFilters,
    limit: i64,
) -> Result<Vec<FeedbackRecord>, StoreError> {
    let (predicate, binds) = filters.predicate();
    let statement = format!(
        "SELECT * FROM feedback WHERE created_at >= ? AND created_at < ?{predicate} \
         ORDER BY created_at DESC, id DESC LIMIT ?"
    );
    let rows = bind_all(sql(statement).bind(range.from).bind(range.to), &binds)
        .bind(limit)
        .fetch_all(pool)
        .await
        .map_err(sql_error)?;
    rows.iter().map(record_from_row).collect()
}

/// Ratings over time for one page, or for the whole site when `route` is
/// `None` (ANA-30).
pub async fn ratings_over_time(
    pool: &SqlitePool,
    range: Range,
    grain: Grain,
    route: Option<&str>,
) -> Result<Vec<RatingPoint>, StoreError> {
    let step = grain.millis();
    let mut predicate = String::new();
    let mut binds: Vec<String> = Vec::new();
    if let Some(route) = route {
        predicate.push_str(" AND route = ?");
        binds.push(route.to_owned());
    }
    let statement = format!(
        "SELECT (created_at - (created_at % ?)) AS bucket, \
                SUM(CASE WHEN rating > 0 THEN 1 ELSE 0 END) AS up, \
                SUM(CASE WHEN rating < 0 THEN 1 ELSE 0 END) AS down \
         FROM feedback \
         WHERE created_at >= ? AND created_at < ? AND rating IS NOT NULL{predicate} \
         GROUP BY bucket"
    );
    let rows = bind_all(
        sql(statement).bind(step).bind(range.from).bind(range.to),
        &binds,
    )
    .fetch_all(pool)
    .await
    .map_err(sql_error)?;

    let mut points: Vec<RatingPoint> = range
        .buckets(grain)
        .into_iter()
        .map(|bucket| RatingPoint {
            bucket,
            ..RatingPoint::default()
        })
        .collect();
    for row in &rows {
        let bucket: i64 = row.try_get("bucket").map_err(sql_error)?;
        let up: Option<i64> = row.try_get("up").map_err(sql_error)?;
        let down: Option<i64> = row.try_get("down").map_err(sql_error)?;
        if let Some(point) = points.iter_mut().find(|p| p.bucket == bucket) {
            point.up = up.unwrap_or(0);
            point.down = down.unwrap_or(0);
        }
    }
    Ok(points)
}

/// Per-page standing, worst-rated first among pages with enough votes to mean
/// anything.
pub async fn by_page(
    pool: &SqlitePool,
    range: Range,
    limit: i64,
) -> Result<Vec<PageRating>, StoreError> {
    let statement = "SELECT route, \
                SUM(CASE WHEN kind <> 'agent' AND rating > 0 THEN 1 ELSE 0 END) AS up, \
                SUM(CASE WHEN kind <> 'agent' AND rating < 0 THEN 1 ELSE 0 END) AS down, \
                SUM(CASE WHEN kind = 'agent' THEN 1 ELSE 0 END) AS agent_reports, \
                SUM(CASE WHEN status = 'open' THEN 1 ELSE 0 END) AS open \
         FROM feedback WHERE created_at >= ? AND created_at < ? \
         GROUP BY route ORDER BY (down + agent_reports) DESC, route ASC LIMIT ?"
        .to_owned();
    let rows = sql(statement)
        .bind(range.from)
        .bind(range.to)
        .bind(limit)
        .fetch_all(pool)
        .await
        .map_err(sql_error)?;
    rows.iter()
        .map(|row| {
            Ok(PageRating {
                route: row.try_get("route").map_err(sql_error)?,
                up: row
                    .try_get::<Option<i64>, _>("up")
                    .map_err(sql_error)?
                    .unwrap_or(0),
                down: row
                    .try_get::<Option<i64>, _>("down")
                    .map_err(sql_error)?
                    .unwrap_or(0),
                agent_reports: row
                    .try_get::<Option<i64>, _>("agent_reports")
                    .map_err(sql_error)?
                    .unwrap_or(0),
                open: row
                    .try_get::<Option<i64>, _>("open")
                    .map_err(sql_error)?
                    .unwrap_or(0),
            })
        })
        .collect()
}

/// How many rows sit in each status, which is the triage queue's header.
pub async fn status_counts(
    pool: &SqlitePool,
    range: Range,
    filters: &FeedbackFilters,
) -> Result<Vec<(FeedbackStatus, i64)>, StoreError> {
    let (predicate, binds) = filters.predicate();
    let statement = format!(
        "SELECT status, COUNT(*) AS n FROM feedback \
         WHERE created_at >= ? AND created_at < ?{predicate} GROUP BY status"
    );
    let rows = bind_all(sql(statement).bind(range.from).bind(range.to), &binds)
        .fetch_all(pool)
        .await
        .map_err(sql_error)?;
    let mut out = Vec::new();
    for row in &rows {
        let status: String = row.try_get("status").map_err(sql_error)?;
        out.push((status_of(&status)?, row.try_get("n").map_err(sql_error)?));
    }
    out.sort_by_key(|(status, _)| status_text(*status));
    Ok(out)
}
