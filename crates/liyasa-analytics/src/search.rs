//! Search analytics (ANA-20).
//!
//! Two event types carry all of it: `search`, which the server emits with the
//! scrubbed query, the result count and the routes it showed, and
//! `search_click`, which the overlay beacons when a result is opened. The
//! shapes are in [`crate::props`].
//!
//! Click-through is a ratio of two counts that are collected differently — the
//! search is server-side and complete, the click is a beacon and is not — so
//! every rate here is an undercount by the beacon delivery ratio
//! [`crate::traffic::beacon_delivery`] measures. The dashboard shows the two
//! together for that reason.

use liyasa_core::store::StoreError;
use liyasa_store::db::sql_error;
use serde::{Deserialize, Serialize};
use sqlx::Row;
use sqlx::sqlite::SqlitePool;

use crate::query::{Filters, Range};
use crate::sql::{bind_all, sql};

/// One query string over a window.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct QueryStat {
    pub q: String,
    pub searches: i64,
    /// Searches that returned nothing.
    pub empty: i64,
    pub clicks: i64,
    /// The route clicked most often from this query, and how often.
    pub top_result: Option<String>,
    pub top_result_clicks: i64,
}

impl QueryStat {
    /// `None` when nobody searched it, rather than a rate of zero.
    pub fn click_through(&self) -> Option<f64> {
        (self.searches > 0).then(|| self.clicks as f64 / self.searches as f64)
    }

    pub fn empty_rate(&self) -> Option<f64> {
        (self.searches > 0).then(|| self.empty as f64 / self.searches as f64)
    }
}

/// How a page did in search results (ANA-20).
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct PageSearch {
    pub route: String,
    /// Times the page appeared in a result list.
    pub impressions: i64,
    pub clicks: i64,
}

impl PageSearch {
    pub fn click_through(&self) -> Option<f64> {
        (self.impressions > 0).then(|| self.clicks as f64 / self.impressions as f64)
    }
}

/// A query against the same window immediately before it (ANA-20).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Trend {
    pub q: String,
    pub searches: i64,
    pub previous: i64,
}

impl Trend {
    pub fn delta(&self) -> i64 {
        self.searches - self.previous
    }

    /// `None` when the query is new: a first appearance has no rate of
    /// change, and reporting one as `+100%` buries genuinely rising queries
    /// under every one-off typo.
    pub fn change(&self) -> Option<f64> {
        (self.previous > 0).then(|| (self.searches - self.previous) as f64 / self.previous as f64)
    }

    pub fn is_new(&self) -> bool {
        self.previous == 0
    }
}

/// Every query in the window, most searched first.
pub async fn queries(
    pool: &SqlitePool,
    range: Range,
    filters: &Filters,
    limit: i64,
) -> Result<Vec<QueryStat>, StoreError> {
    let predicate = filters.predicate();
    let statement = format!(
        "SELECT json_extract(props, '$.q') AS q, \
                COUNT(*) AS searches, \
                SUM(CASE WHEN json_extract(props, '$.results') = 0 THEN 1 ELSE 0 END) AS empty \
         FROM event \
         WHERE ts >= ? AND ts < ? AND type = 'search' AND q IS NOT NULL AND q <> ''{} \
         GROUP BY q ORDER BY searches DESC, q ASC LIMIT ?",
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

    let mut stats: Vec<QueryStat> = rows
        .iter()
        .map(|row| {
            Ok(QueryStat {
                q: row.try_get("q").map_err(sql_error)?,
                searches: row.try_get("searches").map_err(sql_error)?,
                empty: row
                    .try_get::<Option<i64>, _>("empty")
                    .map_err(sql_error)?
                    .unwrap_or(0),
                ..QueryStat::default()
            })
        })
        .collect::<Result<_, StoreError>>()?;

    let clicks = clicks_by_query(pool, range, filters).await?;
    for stat in &mut stats {
        if let Some(found) = clicks.iter().find(|c| c.q == stat.q) {
            stat.clicks = found.clicks;
            stat.top_result = found.top_result.clone();
            stat.top_result_clicks = found.top_result_clicks;
        }
    }
    Ok(stats)
}

/// Clicks per query, and the most clicked result for each.
async fn clicks_by_query(
    pool: &SqlitePool,
    range: Range,
    filters: &Filters,
) -> Result<Vec<QueryStat>, StoreError> {
    let predicate = filters.predicate();
    let statement = format!(
        "SELECT json_extract(props, '$.q') AS q, \
                json_extract(props, '$.target') AS target, \
                COUNT(*) AS n \
         FROM event \
         WHERE ts >= ? AND ts < ? AND type = 'search_click' AND q IS NOT NULL{} \
         GROUP BY q, target ORDER BY n DESC",
        predicate.sql
    );
    let rows = bind_all(
        sql(statement).bind(range.from).bind(range.to),
        &predicate.binds,
    )
    .fetch_all(pool)
    .await
    .map_err(sql_error)?;

    let mut out: Vec<QueryStat> = Vec::new();
    for row in &rows {
        let q: String = row.try_get("q").map_err(sql_error)?;
        let target: Option<String> = row.try_get("target").map_err(sql_error)?;
        let n: i64 = row.try_get("n").map_err(sql_error)?;
        match out.iter_mut().find(|s| s.q == q) {
            Some(found) => {
                found.clicks += n;
                // Rows arrive most-clicked first, so the first target seen for
                // a query is the top one.
                if found.top_result.is_none() {
                    found.top_result = target;
                    found.top_result_clicks = n;
                }
            }
            None => out.push(QueryStat {
                q,
                clicks: n,
                top_result_clicks: n,
                top_result: target,
                ..QueryStat::default()
            }),
        }
    }
    Ok(out)
}

/// Queries that returned nothing (ANA-20). These are the gap between what
/// readers ask for and what the site has.
pub async fn no_result_queries(
    pool: &SqlitePool,
    range: Range,
    filters: &Filters,
    limit: i64,
) -> Result<Vec<QueryStat>, StoreError> {
    let mut found: Vec<QueryStat> = queries(pool, range, filters, i64::MAX)
        .await?
        .into_iter()
        .filter(|s| s.empty > 0)
        .collect();
    found.sort_by(|a, b| b.empty.cmp(&a.empty).then_with(|| a.q.cmp(&b.q)));
    found.truncate(limit.max(0) as usize);
    Ok(found)
}

/// Queries that were searched often and clicked rarely (ANA-20): the results
/// exist and none of them is the answer.
///
/// `min_searches` keeps a single unclicked search out of the list; a rate over
/// one observation is noise.
pub async fn low_click_queries(
    pool: &SqlitePool,
    range: Range,
    filters: &Filters,
    min_searches: i64,
    max_rate: f64,
    limit: i64,
) -> Result<Vec<QueryStat>, StoreError> {
    let mut found: Vec<QueryStat> = queries(pool, range, filters, i64::MAX)
        .await?
        .into_iter()
        .filter(|s| {
            s.searches >= min_searches
                // A query with no results is a different problem with a
                // different fix, and it is already on its own list.
                && s.empty < s.searches
                && s.click_through().is_some_and(|rate| rate <= max_rate)
        })
        .collect();
    found.sort_by(|a, b| b.searches.cmp(&a.searches).then_with(|| a.q.cmp(&b.q)));
    found.truncate(limit.max(0) as usize);
    Ok(found)
}

/// Impressions and clicks per page (ANA-20).
pub async fn per_page(
    pool: &SqlitePool,
    range: Range,
    filters: &Filters,
    limit: i64,
) -> Result<Vec<PageSearch>, StoreError> {
    let predicate = filters.predicate();
    // `json_each` walks the `shown` array, so a page that appeared third in a
    // result list is counted once for that search.
    let impressions = format!(
        // Two names have to be spelled carefully here. `event.type` is
        // qualified because `json_each` exposes a `type` column of its own.
        // The result column is `page` rather than `route` because `event` HAS
        // a `route`, and `GROUP BY route` would then group by the route the
        // search was run from — one group for the whole window — rather than
        // by the page that was shown.
        "SELECT shown.value AS page, COUNT(*) AS n \
         FROM event, json_each(event.props, '$.shown') AS shown \
         WHERE event.ts >= ? AND event.ts < ? AND event.type = 'search'{} \
         GROUP BY shown.value",
        predicate.sql
    );
    let rows = bind_all(
        sql(impressions).bind(range.from).bind(range.to),
        &predicate.binds,
    )
    .fetch_all(pool)
    .await
    .map_err(sql_error)?;
    let mut pages: Vec<PageSearch> = rows
        .iter()
        .map(|row| {
            Ok(PageSearch {
                route: row.try_get("page").map_err(sql_error)?,
                impressions: row.try_get("n").map_err(sql_error)?,
                clicks: 0,
            })
        })
        .collect::<Result<_, StoreError>>()?;

    let clicks = format!(
        // `page`, not `route`, for the same reason: grouping by `route` would
        // group by the page the search was run from.
        "SELECT json_extract(props, '$.target') AS page, COUNT(*) AS n \
         FROM event \
         WHERE ts >= ? AND ts < ? AND type = 'search_click' AND page IS NOT NULL{} \
         GROUP BY json_extract(props, '$.target')",
        predicate.sql
    );
    let rows = bind_all(
        sql(clicks).bind(range.from).bind(range.to),
        &predicate.binds,
    )
    .fetch_all(pool)
    .await
    .map_err(sql_error)?;
    for row in &rows {
        let route: String = row.try_get("page").map_err(sql_error)?;
        let n: i64 = row.try_get("n").map_err(sql_error)?;
        match pages.iter_mut().find(|p| p.route == route) {
            Some(found) => found.clicks += n,
            // A click on a page that was never recorded as shown: the search
            // that produced it is outside the window, or predates `shown`.
            None => pages.push(PageSearch {
                route,
                impressions: 0,
                clicks: n,
            }),
        }
    }
    pages.sort_by(|a, b| {
        b.impressions
            .cmp(&a.impressions)
            .then_with(|| b.clicks.cmp(&a.clicks))
            .then_with(|| a.route.cmp(&b.route))
    });
    pages.truncate(limit.max(0) as usize);
    Ok(pages)
}

/// Queries rising fastest against the previous window of the same length
/// (ANA-20).
pub async fn trending(
    pool: &SqlitePool,
    range: Range,
    filters: &Filters,
    limit: i64,
) -> Result<Vec<Trend>, StoreError> {
    let current = queries(pool, range, filters, i64::MAX).await?;
    let previous = queries(pool, range.previous(), filters, i64::MAX).await?;
    let mut trends: Vec<Trend> = current
        .into_iter()
        .map(|stat| Trend {
            previous: previous
                .iter()
                .find(|p| p.q == stat.q)
                .map_or(0, |p| p.searches),
            q: stat.q,
            searches: stat.searches,
        })
        .collect();
    // By absolute growth, not by rate: a query that went from one to three is
    // not the story a query that went from forty to ninety is.
    trends.sort_by(|a, b| b.delta().cmp(&a.delta()).then_with(|| a.q.cmp(&b.q)));
    trends.retain(|t| t.delta() > 0);
    trends.truncate(limit.max(0) as usize);
    Ok(trends)
}
