//! The job table with leases (PRD §6.13, HOST-03, HOST-07).
//!
//! A job is a row claimed with a lease and renewed by heartbeat. A worker that
//! dies stops renewing, its lease expires, and the next `claim` re-leases the
//! row with the attempt counter advanced. Every statement is in the SQL
//! subset SQLite and Postgres share, and the claim is a select followed by a
//! version-guarded update so two workers racing for one row cannot both win.

use std::time::Duration;

use liyasa_core::ids::{JobId, ProjectId};
use liyasa_core::store::{JobQuery, JobState, Page, StoreError};
use sqlx::sqlite::SqlitePool;
use sqlx::{Row, sqlite::SqliteRow};

use crate::db::sql_error;
use crate::now_ms;
use crate::records::JobRecord;

/// The default retry ladder: 1 min, 5 min, 30 min, 2 h, 12 h (REST-10's
/// shape, used for every job with a backoff).
pub const BACKOFF: &[Duration] = &[
    Duration::from_secs(60),
    Duration::from_secs(5 * 60),
    Duration::from_secs(30 * 60),
    Duration::from_secs(2 * 3600),
    Duration::from_secs(12 * 3600),
];

pub fn backoff(attempt: u32) -> Duration {
    let index = (attempt.max(1) - 1) as usize;
    BACKOFF[index.min(BACKOFF.len() - 1)]
}

#[derive(Debug, Clone)]
pub struct Enqueue {
    pub name: String,
    pub key: String,
    pub priority: i32,
    pub project: Option<ProjectId>,
    pub payload: serde_json::Value,
    /// Not before; `None` is now.
    pub run_at: Option<i64>,
    pub max_attempts: u32,
    pub lease: Duration,
}

impl Enqueue {
    pub fn new(name: impl Into<String>, key: impl Into<String>) -> Self {
        Self {
            name: name.into(),
            key: key.into(),
            priority: 0,
            project: None,
            payload: serde_json::Value::Object(Default::default()),
            run_at: None,
            max_attempts: 5,
            lease: Duration::from_secs(60),
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Enqueued {
    Queued(JobId),
    /// A live job with the same name and key exists; nothing was added.
    Duplicate(JobId),
}

impl Enqueued {
    pub fn id(&self) -> JobId {
        match self {
            Self::Queued(id) | Self::Duplicate(id) => *id,
        }
    }
}

#[derive(Debug, Clone)]
pub struct Jobs {
    pool: SqlitePool,
}

fn state_text(state: JobState) -> &'static str {
    match state {
        JobState::Queued => "queued",
        JobState::Leased => "leased",
        JobState::Done => "done",
        JobState::Failed => "failed",
        JobState::Dead => "dead",
    }
}

fn state_from(text: &str) -> Result<JobState, StoreError> {
    Ok(match text {
        "queued" => JobState::Queued,
        "leased" => JobState::Leased,
        "done" => JobState::Done,
        "failed" => JobState::Failed,
        "dead" => JobState::Dead,
        other => return Err(StoreError::Sql(format!("unknown job state `{other}`"))),
    })
}

fn json_column(row: &SqliteRow, name: &str) -> Result<Option<serde_json::Value>, StoreError> {
    let text: Option<String> = row.try_get(name).map_err(sql_error)?;
    text.map(|t| serde_json::from_str(&t).map_err(|e| StoreError::Sql(e.to_string())))
        .transpose()
}

pub(crate) fn job_from_row(row: &SqliteRow) -> Result<JobRecord, StoreError> {
    let id: String = row.try_get("id").map_err(sql_error)?;
    let project: Option<String> = row.try_get("project_id").map_err(sql_error)?;
    let state: String = row.try_get("state").map_err(sql_error)?;
    Ok(JobRecord {
        id: JobId::parse(&id).ok_or_else(|| StoreError::Sql(format!("bad job id `{id}`")))?,
        name: row.try_get("name").map_err(sql_error)?,
        key: row.try_get("key").map_err(sql_error)?,
        priority: row.try_get::<i64, _>("priority").map_err(sql_error)? as i32,
        state: state_from(&state)?,
        project: project.and_then(|p| ProjectId::parse(&p)),
        payload: json_column(row, "payload")?.unwrap_or(serde_json::Value::Null),
        attempts: row.try_get::<i64, _>("attempts").map_err(sql_error)? as u32,
        max_attempts: row.try_get::<i64, _>("max_attempts").map_err(sql_error)? as u32,
        run_at: row.try_get("run_at").map_err(sql_error)?,
        lease_ms: row.try_get("lease_ms").map_err(sql_error)?,
        lease_until: row.try_get("lease_until").map_err(sql_error)?,
        worker: row.try_get("worker").map_err(sql_error)?,
        result: json_column(row, "result")?,
        error: row.try_get("error").map_err(sql_error)?,
        created_at: row.try_get("created_at").map_err(sql_error)?,
        updated_at: row.try_get("updated_at").map_err(sql_error)?,
        version: row.try_get("version").map_err(sql_error)?,
    })
}

macro_rules! columns {
    () => {
        "id, name, key, priority, state, project_id, payload, attempts, max_attempts, \
         run_at, lease_ms, lease_until, worker, result, error, created_at, updated_at, version"
    };
}

impl Jobs {
    pub fn new(pool: SqlitePool) -> Self {
        Self { pool }
    }

    pub async fn enqueue(&self, job: &Enqueue) -> Result<Enqueued, StoreError> {
        let id = JobId(crate::new_ulid());
        let now = now_ms();
        let payload =
            serde_json::to_string(&job.payload).map_err(|e| StoreError::Sql(e.to_string()))?;
        let inserted = sqlx::query(
            "INSERT INTO job (id, name, key, priority, state, project_id, payload, attempts, \
             max_attempts, run_at, lease_ms, created_at, updated_at, version) \
             VALUES (?, ?, ?, ?, 'queued', ?, ?, 0, ?, ?, ?, ?, ?, 1) \
             ON CONFLICT (name, key) WHERE state IN ('queued', 'leased') DO NOTHING",
        )
        .bind(id.to_string())
        .bind(&job.name)
        .bind(&job.key)
        .bind(job.priority as i64)
        .bind(job.project.map(|p| p.to_string()))
        .bind(payload)
        .bind(job.max_attempts as i64)
        .bind(job.run_at.unwrap_or(now))
        .bind(job.lease.as_millis() as i64)
        .bind(now)
        .bind(now)
        .execute(&self.pool)
        .await
        .map_err(sql_error)?;
        if inserted.rows_affected() == 1 {
            return Ok(Enqueued::Queued(id));
        }
        let existing = sqlx::query(
            "SELECT id FROM job WHERE name = ? AND key = ? AND state IN ('queued', 'leased') LIMIT 1",
        )
        .bind(&job.name)
        .bind(&job.key)
        .fetch_one(&self.pool)
        .await
        .map_err(sql_error)?;
        let existing: String = existing.try_get("id").map_err(sql_error)?;
        Ok(Enqueued::Duplicate(
            JobId::parse(&existing).ok_or(StoreError::NotFound)?,
        ))
    }

    /// Leases the next runnable job to `worker`: a queued job whose `run_at`
    /// has passed, or a leased job whose lease expired (its worker died),
    /// highest priority first, oldest first within a priority.
    pub async fn claim(
        &self,
        worker: &str,
        lease: Duration,
    ) -> Result<Option<JobRecord>, StoreError> {
        for _ in 0..8 {
            let now = now_ms();
            let candidate = sqlx::query(
                "SELECT id, version FROM job \
                 WHERE (state = 'queued' AND run_at <= ?) OR (state = 'leased' AND lease_until < ?) \
                 ORDER BY priority DESC, run_at ASC, id ASC LIMIT 1",
            )
            .bind(now)
            .bind(now)
            .fetch_optional(&self.pool)
            .await
            .map_err(sql_error)?;
            let Some(row) = candidate else {
                return Ok(None);
            };
            let id: String = row.try_get("id").map_err(sql_error)?;
            let version: i64 = row.try_get("version").map_err(sql_error)?;
            let lease_ms = lease.as_millis() as i64;
            let updated = sqlx::query(
                "UPDATE job SET state = 'leased', worker = ?, lease_ms = ?, lease_until = ?, \
                 attempts = attempts + 1, updated_at = ?, version = version + 1 \
                 WHERE id = ? AND version = ?",
            )
            .bind(worker)
            .bind(lease_ms)
            .bind(now + lease_ms)
            .bind(now)
            .bind(&id)
            .bind(version)
            .execute(&self.pool)
            .await
            .map_err(sql_error)?;
            if updated.rows_affected() == 1 {
                return self.get_text(&id).await;
            }
            // Another worker won the row; pick the next one.
        }
        Ok(None)
    }

    /// Extends the lease by its original length. A job whose lease already
    /// expired is not renewed: another worker may hold it now.
    pub async fn heartbeat(&self, id: &JobId) -> Result<(), StoreError> {
        let now = now_ms();
        let updated = sqlx::query(
            "UPDATE job SET lease_until = ? + lease_ms, updated_at = ? \
             WHERE id = ? AND state = 'leased' AND lease_until >= ?",
        )
        .bind(now)
        .bind(now)
        .bind(id.to_string())
        .bind(now)
        .execute(&self.pool)
        .await
        .map_err(sql_error)?;
        if updated.rows_affected() == 1 {
            Ok(())
        } else {
            Err(StoreError::Conflict)
        }
    }

    pub async fn complete(&self, id: &JobId, result: serde_json::Value) -> Result<(), StoreError> {
        let result = serde_json::to_string(&result).map_err(|e| StoreError::Sql(e.to_string()))?;
        self.finish(id, "done", Some(result), None).await
    }

    /// Records a failure: back to `queued` with backoff while attempts remain,
    /// `dead` once they run out.
    pub async fn fail(&self, id: &JobId, error: &str) -> Result<JobState, StoreError> {
        let Some(job) = self.get(id).await? else {
            return Err(StoreError::NotFound);
        };
        if job.attempts >= job.max_attempts {
            self.finish(id, "dead", None, Some(error)).await?;
            return Ok(JobState::Dead);
        }
        let now = now_ms();
        let run_at = now + backoff(job.attempts).as_millis() as i64;
        sqlx::query(
            "UPDATE job SET state = 'queued', run_at = ?, lease_until = NULL, worker = NULL, \
             error = ?, updated_at = ?, version = version + 1 WHERE id = ?",
        )
        .bind(run_at)
        .bind(error)
        .bind(now)
        .bind(id.to_string())
        .execute(&self.pool)
        .await
        .map_err(sql_error)?;
        Ok(JobState::Queued)
    }

    async fn finish(
        &self,
        id: &JobId,
        state: &str,
        result: Option<String>,
        error: Option<&str>,
    ) -> Result<(), StoreError> {
        let updated = sqlx::query(
            "UPDATE job SET state = ?, result = ?, error = ?, lease_until = NULL, worker = NULL, \
             updated_at = ?, version = version + 1 WHERE id = ?",
        )
        .bind(state)
        .bind(result)
        .bind(error)
        .bind(now_ms())
        .bind(id.to_string())
        .execute(&self.pool)
        .await
        .map_err(sql_error)?;
        if updated.rows_affected() == 1 {
            Ok(())
        } else {
            Err(StoreError::NotFound)
        }
    }

    /// `liyasa jobs cancel`: a queued or leased job stops; a worker holding it
    /// finds its heartbeat refused.
    pub async fn cancel(&self, id: &JobId) -> Result<(), StoreError> {
        let updated = sqlx::query(
            "UPDATE job SET state = 'failed', error = 'cancelled', lease_until = NULL, worker = NULL, \
             updated_at = ?, version = version + 1 WHERE id = ? AND state IN ('queued', 'leased')",
        )
        .bind(now_ms())
        .bind(id.to_string())
        .execute(&self.pool)
        .await
        .map_err(sql_error)?;
        if updated.rows_affected() == 1 {
            Ok(())
        } else {
            Err(StoreError::NotFound)
        }
    }

    /// `liyasa jobs retry`: a failed or dead job goes back to the queue with
    /// its attempts reset.
    pub async fn retry(&self, id: &JobId) -> Result<(), StoreError> {
        let now = now_ms();
        let updated = sqlx::query(
            "UPDATE job SET state = 'queued', attempts = 0, run_at = ?, error = NULL, result = NULL, \
             updated_at = ?, version = version + 1 WHERE id = ? AND state IN ('failed', 'dead')",
        )
        .bind(now)
        .bind(now)
        .bind(id.to_string())
        .execute(&self.pool)
        .await
        .map_err(sql_error)?;
        if updated.rows_affected() == 1 {
            Ok(())
        } else {
            Err(StoreError::NotFound)
        }
    }

    /// Hands one job back, without touching anything else this worker holds.
    ///
    /// `release` is by worker and would re-queue every job that worker has in
    /// flight, which is only safe while a worker holds one at a time. A caller
    /// that puts a single job down wants this.
    pub async fn release_one(&self, id: &JobId) -> Result<(), StoreError> {
        sqlx::query(
            "UPDATE job SET state = 'queued', lease_until = NULL, worker = NULL, \
             attempts = CASE WHEN attempts > 0 THEN attempts - 1 ELSE 0 END, \
             updated_at = ?, version = version + 1 WHERE id = ? AND state = 'leased'",
        )
        .bind(now_ms())
        .bind(id.to_string())
        .execute(&self.pool)
        .await
        .map_err(sql_error)?;
        Ok(())
    }

    /// NFR-31: a draining replica hands its leases back so another replica
    /// claims them without waiting for the leases to expire.
    pub async fn release(&self, worker: &str) -> Result<u64, StoreError> {
        let updated = sqlx::query(
            "UPDATE job SET state = 'queued', lease_until = NULL, worker = NULL, \
             attempts = CASE WHEN attempts > 0 THEN attempts - 1 ELSE 0 END, \
             updated_at = ?, version = version + 1 WHERE state = 'leased' AND worker = ?",
        )
        .bind(now_ms())
        .bind(worker)
        .execute(&self.pool)
        .await
        .map_err(sql_error)?;
        Ok(updated.rows_affected())
    }

    pub async fn get(&self, id: &JobId) -> Result<Option<JobRecord>, StoreError> {
        self.get_text(&id.to_string()).await
    }

    async fn get_text(&self, id: &str) -> Result<Option<JobRecord>, StoreError> {
        let row = sqlx::query(concat!("SELECT ", columns!(), " FROM job WHERE id = ?"))
            .bind(id)
            .fetch_optional(&self.pool)
            .await
            .map_err(sql_error)?;
        row.as_ref().map(job_from_row).transpose()
    }

    pub async fn delete(&self, id: &JobId) -> Result<(), StoreError> {
        sqlx::query("DELETE FROM job WHERE id = ?")
            .bind(id.to_string())
            .execute(&self.pool)
            .await
            .map_err(sql_error)?;
        Ok(())
    }

    /// Newest first; the cursor is the last id seen.
    pub async fn list(&self, query: &JobQuery, page: Page) -> Result<Vec<JobRecord>, StoreError> {
        let limit = if page.limit == 0 {
            50
        } else {
            page.limit.min(500)
        } as i64;
        let rows = sqlx::query(concat!(
            "SELECT ",
            columns!(),
            " FROM job WHERE (? IS NULL OR name = ?) AND (? IS NULL OR state = ?) \
             AND (? IS NULL OR project_id = ?) AND (? IS NULL OR id < ?) ORDER BY id DESC LIMIT ?"
        ))
        .bind(&query.name)
        .bind(&query.name)
        .bind(query.state.map(state_text))
        .bind(query.state.map(state_text))
        .bind(query.project.map(|p| p.to_string()))
        .bind(query.project.map(|p| p.to_string()))
        .bind(&page.cursor)
        .bind(&page.cursor)
        .bind(limit)
        .fetch_all(&self.pool)
        .await
        .map_err(sql_error)?;
        rows.iter().map(job_from_row).collect()
    }

    /// The distinct names of jobs waiting to run. A name here that no handler
    /// is registered for is work nobody can do, which is the shape of defect
    /// this project keeps producing (RFC 1404).
    pub async fn live_names(&self) -> Result<Vec<String>, StoreError> {
        let rows = sqlx::query(
            "SELECT DISTINCT name FROM job WHERE state IN ('queued', 'leased') ORDER BY name",
        )
        .fetch_all(&self.pool)
        .await
        .map_err(sql_error)?;
        rows.iter()
            .map(|row| row.try_get("name").map_err(sql_error))
            .collect()
    }

    /// The number of live jobs, for the metrics endpoint and the queue cap.
    pub async fn depth(&self, name: Option<&str>) -> Result<u64, StoreError> {
        let row = sqlx::query(
            "SELECT COUNT(*) AS n FROM job WHERE state IN ('queued', 'leased') AND (? IS NULL OR name = ?)",
        )
        .bind(name)
        .bind(name)
        .fetch_one(&self.pool)
        .await
        .map_err(sql_error)?;
        Ok(row.try_get::<i64, _>("n").map_err(sql_error)? as u64)
    }
}
