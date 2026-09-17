//! Retention and deletion (ANA-06).
//!
//! Raw events are kept for a configurable number of days and hourly rollups
//! for a number of months; totals are kept forever. There is no totals table
//! in `migrations/analytics/0001_events.sql`, so a pass that deleted old
//! rollup rows would be the thing that lost the all-time numbers. It does not:
//! rollup rows past the window are deleted only when a [`TotalsSink`] has
//! taken the folded totals first, and otherwise they stay and the report says
//! so (RFC 1703).
//!
//! Every pass returns a [`SweepReport`], which is what the job row stores as
//! its result. That is what "deletion jobs are auditable" means here: not a log
//! line, a record with the policy it ran under and the boundary it deleted to.

use liyasa_core::net::BoxFut;
use liyasa_core::store::StoreError;
use liyasa_store::db::sql_error;
use serde::{Deserialize, Serialize};
use sqlx::Row;
use sqlx::sqlite::SqlitePool;

use crate::query::DAY_MS;

/// Thirteen months as days. A month is not a unit SQLite has and no date crate
/// is in the dependency table (§6.2.1), so the window is counted in days of
/// 30.4375 — the mean Gregorian month — and rounded.
const DAYS_PER_MONTH: f64 = 30.4375;

/// `analytics.retention` in `schemas/liyasa.schema.json`.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Policy {
    pub raw_days: i64,
    pub aggregate_months: i64,
}

impl Default for Policy {
    fn default() -> Self {
        Self {
            raw_days: 90,
            aggregate_months: 13,
        }
    }
}

impl Policy {
    /// The instant before which raw rows are deleted.
    pub fn raw_boundary(&self, now: i64) -> i64 {
        now - self.raw_days.max(0) * DAY_MS
    }

    /// The instant before which rollup rows may be deleted, once their totals
    /// are somewhere else.
    pub fn aggregate_boundary(&self, now: i64) -> i64 {
        let days = (self.aggregate_months.max(0) as f64 * DAYS_PER_MONTH).round() as i64;
        now - days * DAY_MS
    }
}

/// How far back each table can answer a question (RFC 1702). A dashboard page
/// asks this before it offers a range, so a version split over the last year
/// says "raw events reach back 90 days" rather than drawing an empty chart.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub struct Horizon {
    pub raw_from: i64,
    pub rollup_from: i64,
}

/// Reads the oldest row each table still holds.
pub async fn horizon(pool: &SqlitePool, policy: Policy, now: i64) -> Result<Horizon, StoreError> {
    let row = sqlx::query("SELECT MIN(ts) AS raw FROM event")
        .fetch_one(pool)
        .await
        .map_err(sql_error)?;
    let raw: Option<i64> = row.try_get("raw").map_err(sql_error)?;
    let row = sqlx::query("SELECT MIN(hour) AS rollup FROM agg_hour")
        .fetch_one(pool)
        .await
        .map_err(sql_error)?;
    let rollup: Option<i64> = row.try_get("rollup").map_err(sql_error)?;
    Ok(Horizon {
        raw_from: raw.unwrap_or_else(|| policy.raw_boundary(now)),
        rollup_from: rollup.unwrap_or_else(|| policy.aggregate_boundary(now)),
    })
}

/// One all-time number, folded out of rollup rows that are about to be deleted.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Total {
    pub site: String,
    pub env: String,
    pub route: String,
    #[serde(rename = "type")]
    pub kind: String,
    pub caller_kind: String,
    pub format: String,
    pub count: i64,
}

/// Where a total goes when its rollup rows are deleted (RFC 1703).
///
/// There is no totals table yet. Implementing this over one is the whole
/// change when there is.
pub trait TotalsSink: Send + Sync {
    fn absorb<'a>(&'a self, totals: &'a [Total]) -> BoxFut<'a, Result<(), StoreError>>;
}

/// What a pass did, stored as the job's result (ANA-06: auditable).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SweepReport {
    /// When the pass ran.
    pub at: i64,
    pub policy: Policy,
    pub raw_boundary: i64,
    pub raw_deleted: i64,
    pub aggregate_boundary: i64,
    pub rollup_deleted: i64,
    pub totals_absorbed: i64,
    /// Rollup rows past the window that were kept because nothing would take
    /// their totals. Non-zero means the aggregate window is not being honoured
    /// — deliberately, because the alternative is losing the number.
    pub rollup_kept_for_want_of_a_sink: i64,
    pub drops_deleted: i64,
}

impl SweepReport {
    pub fn deleted_anything(&self) -> bool {
        self.raw_deleted > 0 || self.rollup_deleted > 0 || self.drops_deleted > 0
    }
}

/// Runs one retention pass.
pub async fn sweep(
    pool: &SqlitePool,
    policy: Policy,
    now: i64,
    totals: Option<&dyn TotalsSink>,
) -> Result<SweepReport, StoreError> {
    let raw_boundary = policy.raw_boundary(now);
    let aggregate_boundary = policy.aggregate_boundary(now);

    let raw_deleted = sqlx::query("DELETE FROM event WHERE ts < ?")
        .bind(raw_boundary)
        .execute(pool)
        .await
        .map_err(sql_error)?
        .rows_affected() as i64;

    let mut rollup_deleted = 0;
    let mut absorbed = 0;
    let mut kept = 0;
    let expired = expired_totals(pool, aggregate_boundary).await?;
    if !expired.is_empty() {
        match totals {
            Some(sink) => {
                sink.absorb(&expired).await?;
                absorbed = expired.len() as i64;
                rollup_deleted = sqlx::query("DELETE FROM agg_hour WHERE hour < ?")
                    .bind(aggregate_boundary)
                    .execute(pool)
                    .await
                    .map_err(sql_error)?
                    .rows_affected() as i64;
            }
            None => {
                let row = sqlx::query("SELECT COUNT(*) AS n FROM agg_hour WHERE hour < ?")
                    .bind(aggregate_boundary)
                    .fetch_one(pool)
                    .await
                    .map_err(sql_error)?;
                kept = row.try_get("n").map_err(sql_error)?;
            }
        }
    }

    // The drop counters are keyed by day and expire with the rollup.
    let drops_deleted = sqlx::query("DELETE FROM ingest_drops WHERE day < ?")
        .bind(aggregate_boundary / DAY_MS)
        .execute(pool)
        .await
        .map_err(sql_error)?
        .rows_affected() as i64;

    Ok(SweepReport {
        at: now,
        policy,
        raw_boundary,
        raw_deleted,
        aggregate_boundary,
        rollup_deleted,
        totals_absorbed: absorbed,
        rollup_kept_for_want_of_a_sink: kept,
        drops_deleted,
    })
}

/// The totals a sweep would have to carry before it could delete anything.
pub async fn expired_totals(pool: &SqlitePool, boundary: i64) -> Result<Vec<Total>, StoreError> {
    let rows = sqlx::query(
        "SELECT site, env, route, type, caller_kind, format, SUM(count) AS n \
         FROM agg_hour WHERE hour < ? \
         GROUP BY site, env, route, type, caller_kind, format",
    )
    .bind(boundary)
    .fetch_all(pool)
    .await
    .map_err(sql_error)?;
    rows.iter()
        .map(|row| {
            Ok(Total {
                site: row.try_get("site").map_err(sql_error)?,
                env: row.try_get("env").map_err(sql_error)?,
                route: row.try_get("route").map_err(sql_error)?,
                kind: row.try_get("type").map_err(sql_error)?,
                caller_kind: row.try_get("caller_kind").map_err(sql_error)?,
                format: row.try_get("format").map_err(sql_error)?,
                count: row.try_get("n").map_err(sql_error)?,
            })
        })
        .collect()
}
