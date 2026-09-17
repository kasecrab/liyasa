//! Traffic (ANA-10).
//!
//! Every number here is split by caller kind, because the question this
//! product exists to answer is how much of the reading is done by agents.
//! Markdown fetches and MCP calls are series in their own right for the same
//! reason.
//!
//! Which table answers is not a detail: `agg_hour` has no session key, no
//! variant and no referrer, so four of ANA-10's clauses can only come from the
//! raw `event` table, which is retained for 90 days rather than thirteen
//! months (RFC 1702). [`Series::source`] says which one answered so a chart can
//! label itself.

use liyasa_core::store::StoreError;
use liyasa_store::db::sql_error;
use serde::{Deserialize, Serialize};
use sqlx::Row;
use sqlx::sqlite::SqlitePool;

use crate::agents::CallerKind;
use crate::query::{Filters, Grain, Range};
use crate::sql::{bind_all, bind_kinds, placeholders, sql};

/// Types the server emits for itself (ANA-01). These are complete: every
/// request produces one.
pub const SERVER_TYPES: &[&str] = &[
    "page_view",
    "markdown_fetch",
    "search",
    "assistant_message",
    "feedback",
    "playground_request",
    "mcp_call",
    "deployment",
];

/// Types only a browser can observe. A reader whose content blocker or
/// corporate proxy eats the beacon is missing from every one of these, which
/// is what "sampled by client" on the chart means (ANA-10).
pub const CLIENT_TYPES: &[&str] = &[
    "scroll_depth",
    "time_on_page",
    "toc_click",
    "tab_select",
    "code_group_select",
    "copy_code",
    "outbound_click",
    "page_load",
    "search_click",
    "feedback_shown",
];

pub fn is_client_measured(kind: &str) -> bool {
    CLIENT_TYPES.contains(&kind)
}

/// Which table a series came from.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum Source {
    /// `agg_hour`, kept for thirteen months.
    Rollup,
    /// `event`, kept for the raw retention window.
    Raw,
}

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct ByCaller {
    pub human: i64,
    pub agent: i64,
    pub bot: i64,
    pub integration: i64,
}

impl ByCaller {
    pub fn total(&self) -> i64 {
        self.human + self.agent + self.bot + self.integration
    }

    pub fn add(&mut self, kind: &str, count: i64) {
        match kind {
            "agent" => self.agent += count,
            "bot" => self.bot += count,
            "integration" => self.integration += count,
            _ => self.human += count,
        }
    }

    pub fn of(&self, kind: CallerKind) -> i64 {
        match kind {
            CallerKind::Human => self.human,
            CallerKind::Agent => self.agent,
            CallerKind::Bot => self.bot,
            CallerKind::Integration => self.integration,
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub struct Point {
    /// The bucket's opening instant.
    pub bucket: i64,
    #[serde(flatten)]
    pub counts: ByCaller,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Series {
    pub grain: Grain,
    pub source: Source,
    /// True when the series counts events only a browser reports, and is
    /// therefore an undercount of readers (ANA-10).
    pub sampled_by_client: bool,
    pub points: Vec<Point>,
}

impl Series {
    pub fn total(&self) -> ByCaller {
        self.points.iter().fold(ByCaller::default(), |mut sum, p| {
            sum.human += p.counts.human;
            sum.agent += p.counts.agent;
            sum.bot += p.counts.bot;
            sum.integration += p.counts.integration;
            sum
        })
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct RouteCount {
    pub route: String,
    #[serde(flatten)]
    pub counts: ByCaller,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct NameCount {
    pub name: String,
    pub count: i64,
}

/// Server page views against client page-load beacons over the same window
/// (ANA-10). Not a sampling rate anyone chose: it is the measured share of
/// readers whose browser delivered a beacon.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub struct Delivery {
    pub server_page_views: i64,
    pub client_page_loads: i64,
}

impl Delivery {
    /// `None` when nothing was served: a ratio out of zero is not zero.
    pub fn ratio(&self) -> Option<f64> {
        (self.server_page_views > 0)
            .then(|| self.client_page_loads as f64 / self.server_page_views as f64)
    }
}

/// A time series of one or more event types, split by caller kind.
pub async fn series(
    pool: &SqlitePool,
    range: Range,
    grain: Grain,
    filters: &Filters,
    kinds: &[&str],
) -> Result<Series, StoreError> {
    let step = grain.millis();
    let sampled_by_client = kinds.iter().all(|k| is_client_measured(k));
    let use_rollup = filters.served_by_rollup();
    let source = if use_rollup {
        Source::Rollup
    } else {
        Source::Raw
    };
    let predicate = if use_rollup {
        filters.rollup_predicate()
    } else {
        filters.predicate()
    };
    let (table, time, caller) = if use_rollup {
        ("agg_hour", "hour", "caller_kind")
    } else {
        ("event", "ts", "json_extract(caller, '$.kind')")
    };
    let count = if use_rollup { "SUM(count)" } else { "COUNT(*)" };
    let statement = format!(
        "SELECT ({time} - ({time} % ?)) AS bucket, {caller} AS kind, {count} AS n \
         FROM {table} \
         WHERE {time} >= ? AND {time} < ? AND type IN ({}){} \
         GROUP BY bucket, kind",
        placeholders(kinds.len()),
        predicate.sql
    );
    let query = sql(statement).bind(step).bind(range.from).bind(range.to);
    let rows = bind_all(bind_kinds(query, kinds), &predicate.binds)
        .fetch_all(pool)
        .await
        .map_err(sql_error)?;

    // Every bucket in the range gets a point, so a gap in the data is a zero
    // on the chart rather than a line drawn straight across it.
    let mut points: Vec<Point> = range
        .buckets(grain)
        .into_iter()
        .map(|bucket| Point {
            bucket,
            counts: ByCaller::default(),
        })
        .collect();
    for row in &rows {
        let bucket: i64 = row.try_get("bucket").map_err(sql_error)?;
        let kind: Option<String> = row.try_get("kind").map_err(sql_error)?;
        let n: i64 = row.try_get("n").map_err(sql_error)?;
        if let Some(point) = points.iter_mut().find(|p| p.bucket == bucket) {
            point.counts.add(kind.as_deref().unwrap_or("human"), n);
        }
    }
    Ok(Series {
        grain,
        source,
        sampled_by_client,
        points,
    })
}

/// How many events of each type, split by caller.
pub async fn totals(
    pool: &SqlitePool,
    range: Range,
    filters: &Filters,
    kinds: &[&str],
) -> Result<ByCaller, StoreError> {
    let use_rollup = filters.served_by_rollup();
    let predicate = if use_rollup {
        filters.rollup_predicate()
    } else {
        filters.predicate()
    };
    let (table, time, caller, count) = if use_rollup {
        ("agg_hour", "hour", "caller_kind", "SUM(count)")
    } else {
        ("event", "ts", "json_extract(caller, '$.kind')", "COUNT(*)")
    };
    let statement = format!(
        "SELECT {caller} AS kind, {count} AS n FROM {table} \
         WHERE {time} >= ? AND {time} < ? AND type IN ({}){} GROUP BY kind",
        placeholders(kinds.len()),
        predicate.sql
    );
    let query = sql(statement).bind(range.from).bind(range.to);
    let rows = bind_all(bind_kinds(query, kinds), &predicate.binds)
        .fetch_all(pool)
        .await
        .map_err(sql_error)?;
    let mut out = ByCaller::default();
    for row in &rows {
        let kind: Option<String> = row.try_get("kind").map_err(sql_error)?;
        let n: i64 = row.try_get("n").map_err(sql_error)?;
        out.add(kind.as_deref().unwrap_or("human"), n);
    }
    Ok(out)
}

/// Distinct session keys, split by caller kind. Raw table only: the rollup has
/// no session key to count (RFC 1702).
pub async fn unique_sessions(
    pool: &SqlitePool,
    range: Range,
    filters: &Filters,
) -> Result<ByCaller, StoreError> {
    let predicate = filters.predicate();
    let statement = format!(
        "SELECT json_extract(caller, '$.kind') AS kind, COUNT(DISTINCT session_key) AS n \
         FROM event WHERE ts >= ? AND ts < ? AND session_key <> ''{} GROUP BY kind",
        predicate.sql
    );
    let rows = bind_all(
        sql(statement).bind(range.from).bind(range.to),
        &predicate.binds,
    )
    .fetch_all(pool)
    .await
    .map_err(sql_error)?;
    let mut out = ByCaller::default();
    for row in &rows {
        let kind: Option<String> = row.try_get("kind").map_err(sql_error)?;
        let n: i64 = row.try_get("n").map_err(sql_error)?;
        out.add(kind.as_deref().unwrap_or("human"), n);
    }
    Ok(out)
}

/// The most-read pages, split by caller so an operator can see a page agents
/// read and readers do not.
pub async fn top_routes(
    pool: &SqlitePool,
    range: Range,
    filters: &Filters,
    kinds: &[&str],
    limit: i64,
) -> Result<Vec<RouteCount>, StoreError> {
    let use_rollup = filters.served_by_rollup();
    let predicate = if use_rollup {
        filters.rollup_predicate()
    } else {
        filters.predicate()
    };
    let (table, time, caller, count) = if use_rollup {
        ("agg_hour", "hour", "caller_kind", "SUM(count)")
    } else {
        ("event", "ts", "json_extract(caller, '$.kind')", "COUNT(*)")
    };
    let statement = format!(
        "SELECT route, {caller} AS kind, {count} AS n FROM {table} \
         WHERE {time} >= ? AND {time} < ? AND type IN ({}){} \
         GROUP BY route, kind",
        placeholders(kinds.len()),
        predicate.sql
    );
    let query = sql(statement).bind(range.from).bind(range.to);
    let rows = bind_all(bind_kinds(query, kinds), &predicate.binds)
        .fetch_all(pool)
        .await
        .map_err(sql_error)?;
    let mut by_route: Vec<RouteCount> = Vec::new();
    for row in &rows {
        let route: String = row.try_get("route").map_err(sql_error)?;
        let kind: Option<String> = row.try_get("kind").map_err(sql_error)?;
        let n: i64 = row.try_get("n").map_err(sql_error)?;
        match by_route.iter_mut().find(|r| r.route == route) {
            Some(found) => found.counts.add(kind.as_deref().unwrap_or("human"), n),
            None => {
                let mut counts = ByCaller::default();
                counts.add(kind.as_deref().unwrap_or("human"), n);
                by_route.push(RouteCount { route, counts });
            }
        }
    }
    // Sorted here rather than in SQL because the ranking is on the total of
    // four caller kinds, which are separate rows.
    by_route.sort_by(|a, b| {
        b.counts
            .total()
            .cmp(&a.counts.total())
            .then_with(|| a.route.cmp(&b.route))
    });
    by_route.truncate(limit.max(0) as usize);
    Ok(by_route)
}

/// Referring hosts. Raw table only: the rollup has no referrer (RFC 1702).
pub async fn referrers(
    pool: &SqlitePool,
    range: Range,
    filters: &Filters,
    limit: i64,
) -> Result<Vec<NameCount>, StoreError> {
    let predicate = filters.predicate();
    let statement = format!(
        "SELECT referrer_host AS name, COUNT(*) AS n FROM event \
         WHERE ts >= ? AND ts < ? AND referrer_host IS NOT NULL AND referrer_host <> ''{} \
         GROUP BY name ORDER BY n DESC, name ASC LIMIT ?",
        predicate.sql
    );
    let rows = bind_all(
        sql(statement).bind(range.from).bind(range.to),
        &predicate.binds,
    )
    .bind(limit)
    .fetch_all(pool)
    .await
    .map_err(sql_error)?;
    rows.iter()
        .map(|row| {
            Ok(NameCount {
                name: row.try_get("name").map_err(sql_error)?,
                count: row.try_get("n").map_err(sql_error)?,
            })
        })
        .collect()
}

/// Where a session started and where it ended (ANA-10). Raw table only: the
/// rollup has no session key to order within.
///
/// Two page views in one session can share a millisecond — a redirect, a
/// prefetch that resolved, a client clock with 1 ms resolution — so the
/// earliest `ts` is not on its own a single row. The tie is broken on the
/// row id, which the `event` table assigns in insertion order, and one
/// session therefore contributes to exactly one entry page and one exit page.
pub async fn entry_and_exit(
    pool: &SqlitePool,
    range: Range,
    filters: &Filters,
    limit: i64,
) -> Result<(Vec<NameCount>, Vec<NameCount>), StoreError> {
    let predicate = filters.predicate();
    let mut out = Vec::new();
    for aggregate in ["MIN", "MAX"] {
        let statement = format!(
            "WITH bounds AS (\
                 SELECT session_key, {aggregate}(ts) AS at FROM event \
                 WHERE ts >= ? AND ts < ? AND type = 'page_view' AND session_key <> ''{} \
                 GROUP BY session_key\
             ), picked AS (\
                 SELECT {aggregate}(event.id) AS id \
                 FROM bounds JOIN event \
                   ON event.session_key = bounds.session_key AND event.ts = bounds.at \
                 WHERE event.type = 'page_view' \
                 GROUP BY bounds.session_key\
             ) \
             SELECT event.route AS name, COUNT(*) AS n \
             FROM picked JOIN event ON event.id = picked.id \
             GROUP BY name ORDER BY n DESC, name ASC LIMIT ?",
            predicate.sql
        );
        let rows = bind_all(
            sql(statement).bind(range.from).bind(range.to),
            &predicate.binds,
        )
        .bind(limit)
        .fetch_all(pool)
        .await
        .map_err(sql_error)?;
        let counts: Result<Vec<NameCount>, StoreError> = rows
            .iter()
            .map(|row| {
                Ok(NameCount {
                    name: row.try_get("name").map_err(sql_error)?,
                    count: row.try_get("n").map_err(sql_error)?,
                })
            })
            .collect();
        out.push(counts?);
    }
    let exit = out.pop().unwrap_or_default();
    let entry = out.pop().unwrap_or_default();
    Ok((entry, exit))
}

/// A split by one of the four `variant` dimensions. Raw table only (RFC 1702).
pub async fn by_variant(
    pool: &SqlitePool,
    range: Range,
    filters: &Filters,
    dimension: &str,
    kinds: &[&str],
    limit: i64,
) -> Result<Vec<NameCount>, StoreError> {
    // A whitelist, not an escape: the four are the whole set ANA-71 names.
    if !["version", "locale", "region", "product"].contains(&dimension) {
        return Err(StoreError::Sql(format!(
            "`{dimension}` is not a variant dimension"
        )));
    }
    let predicate = filters.predicate();
    let statement = format!(
        "SELECT json_extract(variant, '$.{dimension}') AS name, COUNT(*) AS n FROM event \
         WHERE ts >= ? AND ts < ? AND type IN ({}) AND name IS NOT NULL{} \
         GROUP BY name ORDER BY n DESC, name ASC LIMIT ?",
        placeholders(kinds.len()),
        predicate.sql
    );
    let query = sql(statement).bind(range.from).bind(range.to);
    let rows = bind_all(bind_kinds(query, kinds), &predicate.binds)
        .bind(limit)
        .fetch_all(pool)
        .await
        .map_err(sql_error)?;
    rows.iter()
        .map(|row| {
            Ok(NameCount {
                name: row.try_get("name").map_err(sql_error)?,
                count: row.try_get("n").map_err(sql_error)?,
            })
        })
        .collect()
}

/// The measured beacon delivery ratio (ANA-10).
pub async fn beacon_delivery(
    pool: &SqlitePool,
    range: Range,
    filters: &Filters,
) -> Result<Delivery, StoreError> {
    // Only human traffic: an agent never runs the reader runtime, so counting
    // agent page views in the denominator would make delivery look broken.
    let mut human = filters.clone();
    human.caller = Some(CallerKind::Human);
    let views = totals(pool, range, &human, &["page_view"]).await?;
    let loads = totals(pool, range, &human, &["page_load"]).await?;
    Ok(Delivery {
        server_page_views: views.total(),
        client_page_loads: loads.total(),
    })
}
