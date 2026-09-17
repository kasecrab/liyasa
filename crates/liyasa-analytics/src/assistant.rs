//! What the assistant was asked and how it did (ANA-10, §19).
//!
//! One event type carries it: `assistant_message`, whose shape is
//! [`crate::props::AssistantMessage`]. The message text is never stored — a
//! thread identifier and a rating are — so the only thing this can report
//! about a question is the topic the assistant itself chose to record and
//! whether it could answer.

use liyasa_core::store::StoreError;
use liyasa_store::db::sql_error;
use serde::{Deserialize, Serialize};
use sqlx::Row;
use sqlx::sqlite::SqlitePool;

use crate::query::{Filters, Range};
use crate::sql::{bind_all, sql};

#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Summary {
    pub messages: i64,
    /// Messages a reader rated either way.
    pub rated: i64,
    pub positive: i64,
    /// Topics the assistant reported it could not answer, most recent first.
    pub unanswered: Vec<String>,
}

impl Summary {
    /// `None` when nobody rated anything: an unrated assistant is not a
    /// badly-rated one.
    pub fn satisfaction(&self) -> Option<f64> {
        (self.rated > 0).then(|| self.positive as f64 / self.rated as f64)
    }
}

pub async fn summary(
    pool: &SqlitePool,
    range: Range,
    filters: &Filters,
    limit: i64,
) -> Result<Summary, StoreError> {
    let predicate = filters.predicate();
    let statement = format!(
        "SELECT COUNT(*) AS messages, \
                SUM(CASE WHEN json_extract(props, '$.rating') IS NOT NULL THEN 1 ELSE 0 END) AS rated, \
                SUM(CASE WHEN json_extract(props, '$.rating') > 0 THEN 1 ELSE 0 END) AS positive \
         FROM event WHERE ts >= ? AND ts < ? AND type = 'assistant_message'{}",
        predicate.sql
    );
    let row = bind_all(
        sql(statement).bind(range.from).bind(range.to),
        &predicate.binds,
    )
    .fetch_one(pool)
    .await
    .map_err(sql_error)?;

    let statement = format!(
        "SELECT DISTINCT json_extract(props, '$.topic') AS topic, MAX(ts) AS at FROM event \
         WHERE ts >= ? AND ts < ? AND type = 'assistant_message' \
           AND json_extract(props, '$.unanswered') = 1 AND topic IS NOT NULL{} \
         GROUP BY topic ORDER BY at DESC LIMIT ?",
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

    Ok(Summary {
        messages: row.try_get("messages").map_err(sql_error)?,
        rated: row
            .try_get::<Option<i64>, _>("rated")
            .map_err(sql_error)?
            .unwrap_or(0),
        positive: row
            .try_get::<Option<i64>, _>("positive")
            .map_err(sql_error)?
            .unwrap_or(0),
        unanswered: rows
            .iter()
            .map(|row| row.try_get("topic").map_err(sql_error))
            .collect::<Result<_, _>>()?,
    })
}
