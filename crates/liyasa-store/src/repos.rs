//! Projects, builds, deployments, feedback, domains, and webhooks over the
//! application pool.

use liyasa_core::ids::{BuildId, Fingerprint, ProjectId};
use liyasa_core::store::{BuildStatus, Page, StoreError};
use sqlx::Row;
use sqlx::sqlite::{SqlitePool, SqliteRow};

use crate::db::sql_error;
use crate::now_ms;
use crate::records::{
    BuildRecord, DeliveryStatus, DeploymentRecord, FeedbackKind, FeedbackRecord, FeedbackStatus,
    ProjectRecord, WebhookDelivery, WebhookSubscription,
};

fn project_id(text: &str) -> Result<ProjectId, StoreError> {
    ProjectId::parse(text).ok_or_else(|| StoreError::Sql(format!("bad project id `{text}`")))
}

fn build_id(text: &str) -> Result<BuildId, StoreError> {
    Fingerprint::parse(text)
        .map(BuildId)
        .ok_or_else(|| StoreError::Sql(format!("bad build id `{text}`")))
}

fn status_text(status: BuildStatus) -> &'static str {
    match status {
        BuildStatus::Queued => "queued",
        BuildStatus::Running => "running",
        BuildStatus::Succeeded => "succeeded",
        BuildStatus::Failed => "failed",
        BuildStatus::Cancelled => "cancelled",
    }
}

fn status_from(text: &str) -> Result<BuildStatus, StoreError> {
    Ok(match text {
        "queued" => BuildStatus::Queued,
        "running" => BuildStatus::Running,
        "succeeded" => BuildStatus::Succeeded,
        "failed" => BuildStatus::Failed,
        "cancelled" => BuildStatus::Cancelled,
        other => return Err(StoreError::Sql(format!("unknown build status `{other}`"))),
    })
}

fn limit(page: &Page) -> i64 {
    if page.limit == 0 {
        50
    } else {
        page.limit.min(500) as i64
    }
}

// ---- projects ----

#[derive(Debug, Clone)]
pub struct Projects {
    pool: SqlitePool,
}

fn project_from_row(row: &SqliteRow) -> Result<ProjectRecord, StoreError> {
    let id: String = row.try_get("id").map_err(sql_error)?;
    let org: Option<String> = row.try_get("org_id").map_err(sql_error)?;
    Ok(ProjectRecord {
        id: project_id(&id)?,
        org: org.and_then(|o| liyasa_core::ids::OrgId::parse(&o)),
        slug: row.try_get("slug").map_err(sql_error)?,
        name: row.try_get("name").map_err(sql_error)?,
        created_at: row.try_get("created_at").map_err(sql_error)?,
        updated_at: row.try_get("updated_at").map_err(sql_error)?,
        version: row.try_get("version").map_err(sql_error)?,
    })
}

impl Projects {
    pub fn new(pool: SqlitePool) -> Self {
        Self { pool }
    }

    pub async fn create(&self, slug: &str, name: &str) -> Result<ProjectRecord, StoreError> {
        let id = ProjectId(crate::new_ulid());
        let now = now_ms();
        sqlx::query(
            "INSERT INTO project (id, org_id, slug, name, created_at, updated_at, version) \
             VALUES (?, NULL, ?, ?, ?, ?, 1)",
        )
        .bind(id.to_string())
        .bind(slug)
        .bind(name)
        .bind(now)
        .bind(now)
        .execute(&self.pool)
        .await
        .map_err(|e| match e {
            sqlx::Error::Database(ref d) if d.is_unique_violation() => StoreError::Conflict,
            other => sql_error(other),
        })?;
        Ok(ProjectRecord {
            id,
            org: None,
            slug: slug.to_owned(),
            name: name.to_owned(),
            created_at: now,
            updated_at: now,
            version: 1,
        })
    }

    pub async fn get(&self, id: &ProjectId) -> Result<Option<ProjectRecord>, StoreError> {
        let row = sqlx::query("SELECT * FROM project WHERE id = ?")
            .bind(id.to_string())
            .fetch_optional(&self.pool)
            .await
            .map_err(sql_error)?;
        row.as_ref().map(project_from_row).transpose()
    }

    pub async fn by_slug(&self, slug: &str) -> Result<Option<ProjectRecord>, StoreError> {
        let row = sqlx::query("SELECT * FROM project WHERE slug = ?")
            .bind(slug)
            .fetch_optional(&self.pool)
            .await
            .map_err(sql_error)?;
        row.as_ref().map(project_from_row).transpose()
    }

    /// Optimistic: the row's version must match `record.version`.
    pub async fn put(&self, record: &ProjectRecord) -> Result<ProjectRecord, StoreError> {
        let now = now_ms();
        let updated = sqlx::query(
            "UPDATE project SET slug = ?, name = ?, org_id = ?, updated_at = ?, version = version + 1 \
             WHERE id = ? AND version = ?",
        )
        .bind(&record.slug)
        .bind(&record.name)
        .bind(record.org.map(|o| o.to_string()))
        .bind(now)
        .bind(record.id.to_string())
        .bind(record.version)
        .execute(&self.pool)
        .await
        .map_err(sql_error)?;
        if updated.rows_affected() == 0 {
            return Err(match self.get(&record.id).await? {
                Some(_) => StoreError::Conflict,
                None => StoreError::NotFound,
            });
        }
        Ok(ProjectRecord {
            updated_at: now,
            version: record.version + 1,
            ..record.clone()
        })
    }

    pub async fn delete(&self, id: &ProjectId) -> Result<(), StoreError> {
        sqlx::query("DELETE FROM project WHERE id = ?")
            .bind(id.to_string())
            .execute(&self.pool)
            .await
            .map_err(sql_error)?;
        Ok(())
    }

    pub async fn list(
        &self,
        slug: Option<&str>,
        page: &Page,
    ) -> Result<Vec<ProjectRecord>, StoreError> {
        let rows = sqlx::query(
            "SELECT * FROM project WHERE (? IS NULL OR slug = ?) AND (? IS NULL OR id > ?) ORDER BY id LIMIT ?",
        )
        .bind(slug)
        .bind(slug)
        .bind(&page.cursor)
        .bind(&page.cursor)
        .bind(limit(page))
        .fetch_all(&self.pool)
        .await
        .map_err(sql_error)?;
        rows.iter().map(project_from_row).collect()
    }
}

// ---- builds and deployments ----

#[derive(Debug, Clone)]
pub struct Builds {
    pool: SqlitePool,
}

fn build_from_row(row: &SqliteRow) -> Result<BuildRecord, StoreError> {
    let id: String = row.try_get("id").map_err(sql_error)?;
    let project: String = row.try_get("project_id").map_err(sql_error)?;
    let status: String = row.try_get("status").map_err(sql_error)?;
    Ok(BuildRecord {
        id: build_id(&id)?,
        project: project_id(&project)?,
        env: row.try_get("env").map_err(sql_error)?,
        status: status_from(&status)?,
        dist: row.try_get("dist").map_err(sql_error)?,
        created_at: row.try_get("created_at").map_err(sql_error)?,
        updated_at: row.try_get("updated_at").map_err(sql_error)?,
        version: row.try_get("version").map_err(sql_error)?,
    })
}

impl Builds {
    pub fn new(pool: SqlitePool) -> Self {
        Self { pool }
    }

    /// Insert or update by build ID (a build is content-addressed, so the
    /// same ID is the same build).
    pub async fn put(&self, record: &BuildRecord) -> Result<(), StoreError> {
        let now = now_ms();
        sqlx::query(
            "INSERT INTO build (id, project_id, env, status, dist, created_at, updated_at, version) \
             VALUES (?, ?, ?, ?, ?, ?, ?, 1) ON CONFLICT (id) DO UPDATE SET status = excluded.status, \
             dist = excluded.dist, env = excluded.env, updated_at = excluded.updated_at, version = build.version + 1",
        )
        .bind(record.id.to_string())
        .bind(record.project.to_string())
        .bind(&record.env)
        .bind(status_text(record.status))
        .bind(&record.dist)
        .bind(if record.created_at == 0 { now } else { record.created_at })
        .bind(now)
        .execute(&self.pool)
        .await
        .map_err(sql_error)?;
        Ok(())
    }

    pub async fn get(&self, id: &BuildId) -> Result<Option<BuildRecord>, StoreError> {
        let row = sqlx::query("SELECT * FROM build WHERE id = ?")
            .bind(id.to_string())
            .fetch_optional(&self.pool)
            .await
            .map_err(sql_error)?;
        row.as_ref().map(build_from_row).transpose()
    }

    pub async fn delete(&self, id: &BuildId) -> Result<(), StoreError> {
        sqlx::query("DELETE FROM build WHERE id = ?")
            .bind(id.to_string())
            .execute(&self.pool)
            .await
            .map_err(sql_error)?;
        Ok(())
    }

    pub async fn latest_for(
        &self,
        project: &ProjectId,
        env: &str,
    ) -> Result<Option<BuildRecord>, StoreError> {
        let row = sqlx::query(
            "SELECT * FROM build WHERE project_id = ? AND env = ? AND status = 'succeeded' \
             ORDER BY created_at DESC LIMIT 1",
        )
        .bind(project.to_string())
        .bind(env)
        .fetch_optional(&self.pool)
        .await
        .map_err(sql_error)?;
        row.as_ref().map(build_from_row).transpose()
    }

    pub async fn list(
        &self,
        project: Option<&ProjectId>,
        env: Option<&str>,
        status: Option<BuildStatus>,
        page: &Page,
    ) -> Result<Vec<BuildRecord>, StoreError> {
        let rows = sqlx::query(
            "SELECT * FROM build WHERE (? IS NULL OR project_id = ?) AND (? IS NULL OR env = ?) \
             AND (? IS NULL OR status = ?) AND (? IS NULL OR created_at < ?) ORDER BY created_at DESC LIMIT ?",
        )
        .bind(project.map(|p| p.to_string()))
        .bind(project.map(|p| p.to_string()))
        .bind(env)
        .bind(env)
        .bind(status.map(status_text))
        .bind(status.map(status_text))
        .bind(page.cursor.as_deref().and_then(|c| c.parse::<i64>().ok()))
        .bind(page.cursor.as_deref().and_then(|c| c.parse::<i64>().ok()))
        .bind(limit(page))
        .fetch_all(&self.pool)
        .await
        .map_err(sql_error)?;
        rows.iter().map(build_from_row).collect()
    }
}

#[derive(Debug, Clone)]
pub struct Deployments {
    pool: SqlitePool,
}

fn deployment_from_row(row: &SqliteRow) -> Result<DeploymentRecord, StoreError> {
    let project: String = row.try_get("project_id").map_err(sql_error)?;
    let build: String = row.try_get("build_id").map_err(sql_error)?;
    Ok(DeploymentRecord {
        project: project_id(&project)?,
        env: row.try_get("env").map_err(sql_error)?,
        build: build_id(&build)?,
        created_at: row.try_get("created_at").map_err(sql_error)?,
        updated_at: row.try_get("updated_at").map_err(sql_error)?,
        version: row.try_get("version").map_err(sql_error)?,
    })
}

impl Deployments {
    pub fn new(pool: SqlitePool) -> Self {
        Self { pool }
    }

    /// Points `env` at `build`: the atomic pointer swap of NFR-30, recorded
    /// in the history so `rollback` can point back.
    pub async fn point(
        &self,
        project: &ProjectId,
        env: &str,
        build: &BuildId,
    ) -> Result<(), StoreError> {
        let now = now_ms();
        let mut tx = self.pool.begin().await.map_err(sql_error)?;
        sqlx::query(
            "INSERT INTO deployment (project_id, env, build_id, created_at, updated_at, version) \
             VALUES (?, ?, ?, ?, ?, 1) ON CONFLICT (project_id, env) DO UPDATE SET build_id = excluded.build_id, \
             updated_at = excluded.updated_at, version = deployment.version + 1",
        )
        .bind(project.to_string())
        .bind(env)
        .bind(build.to_string())
        .bind(now)
        .bind(now)
        .execute(&mut *tx)
        .await
        .map_err(sql_error)?;
        sqlx::query("INSERT INTO deployment_history (project_id, env, build_id, created_at) VALUES (?, ?, ?, ?)")
            .bind(project.to_string())
            .bind(env)
            .bind(build.to_string())
            .bind(now)
            .execute(&mut *tx)
            .await
            .map_err(sql_error)?;
        tx.commit().await.map_err(sql_error)
    }

    pub async fn current(
        &self,
        project: &ProjectId,
        env: &str,
    ) -> Result<Option<DeploymentRecord>, StoreError> {
        let row = sqlx::query("SELECT * FROM deployment WHERE project_id = ? AND env = ?")
            .bind(project.to_string())
            .bind(env)
            .fetch_optional(&self.pool)
            .await
            .map_err(sql_error)?;
        row.as_ref().map(deployment_from_row).transpose()
    }

    /// The pointer before the current one, for rollback.
    pub async fn previous(
        &self,
        project: &ProjectId,
        env: &str,
    ) -> Result<Option<BuildId>, StoreError> {
        let rows = sqlx::query(
            "SELECT build_id FROM deployment_history WHERE project_id = ? AND env = ? ORDER BY id DESC LIMIT 2",
        )
        .bind(project.to_string())
        .bind(env)
        .fetch_all(&self.pool)
        .await
        .map_err(sql_error)?;
        match rows.get(1) {
            Some(row) => {
                let id: String = row.try_get("build_id").map_err(sql_error)?;
                Ok(Some(build_id(&id)?))
            }
            None => Ok(None),
        }
    }

    pub async fn history(
        &self,
        project: &ProjectId,
        env: &str,
        page: &Page,
    ) -> Result<Vec<DeploymentRecord>, StoreError> {
        let rows = sqlx::query(
            "SELECT project_id, env, build_id, created_at, created_at AS updated_at, id AS version \
             FROM deployment_history WHERE project_id = ? AND env = ? AND (? IS NULL OR id < ?) ORDER BY id DESC LIMIT ?",
        )
        .bind(project.to_string())
        .bind(env)
        .bind(page.cursor.as_deref().and_then(|c| c.parse::<i64>().ok()))
        .bind(page.cursor.as_deref().and_then(|c| c.parse::<i64>().ok()))
        .bind(limit(page))
        .fetch_all(&self.pool)
        .await
        .map_err(sql_error)?;
        rows.iter().map(deployment_from_row).collect()
    }

    pub async fn list(
        &self,
        project: Option<&ProjectId>,
        env: Option<&str>,
    ) -> Result<Vec<DeploymentRecord>, StoreError> {
        let rows = sqlx::query(
            "SELECT * FROM deployment WHERE (? IS NULL OR project_id = ?) AND (? IS NULL OR env = ?) ORDER BY project_id, env",
        )
        .bind(project.map(|p| p.to_string()))
        .bind(project.map(|p| p.to_string()))
        .bind(env)
        .bind(env)
        .fetch_all(&self.pool)
        .await
        .map_err(sql_error)?;
        rows.iter().map(deployment_from_row).collect()
    }

    pub async fn delete(&self, project: &ProjectId, env: &str) -> Result<(), StoreError> {
        sqlx::query("DELETE FROM deployment WHERE project_id = ? AND env = ?")
            .bind(project.to_string())
            .bind(env)
            .execute(&self.pool)
            .await
            .map_err(sql_error)?;
        Ok(())
    }
}

// ---- domains (HOST-21, HOST-22) ----

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DomainRecord {
    pub host: String,
    pub project: ProjectId,
    pub base_path: String,
    pub env: String,
}

#[derive(Debug, Clone)]
pub struct Domains {
    pool: SqlitePool,
}

impl Domains {
    pub fn new(pool: SqlitePool) -> Self {
        Self { pool }
    }

    pub async fn put(&self, record: &DomainRecord) -> Result<(), StoreError> {
        let now = now_ms();
        sqlx::query(
            "INSERT INTO domain (host, project_id, base_path, env, verified, created_at, updated_at) \
             VALUES (?, ?, ?, ?, 1, ?, ?) ON CONFLICT (host) DO UPDATE SET project_id = excluded.project_id, \
             base_path = excluded.base_path, env = excluded.env, updated_at = excluded.updated_at",
        )
        .bind(record.host.to_ascii_lowercase())
        .bind(record.project.to_string())
        .bind(&record.base_path)
        .bind(&record.env)
        .bind(now)
        .bind(now)
        .execute(&self.pool)
        .await
        .map_err(sql_error)?;
        Ok(())
    }

    pub async fn get(&self, host: &str) -> Result<Option<DomainRecord>, StoreError> {
        let row = sqlx::query("SELECT host, project_id, base_path, env FROM domain WHERE host = ?")
            .bind(host.to_ascii_lowercase())
            .fetch_optional(&self.pool)
            .await
            .map_err(sql_error)?;
        row.map(|row| {
            let project: String = row.try_get("project_id").map_err(sql_error)?;
            Ok(DomainRecord {
                host: row.try_get("host").map_err(sql_error)?,
                project: project_id(&project)?,
                base_path: row.try_get("base_path").map_err(sql_error)?,
                env: row.try_get("env").map_err(sql_error)?,
            })
        })
        .transpose()
    }

    pub async fn list(&self) -> Result<Vec<DomainRecord>, StoreError> {
        let rows = sqlx::query("SELECT host, project_id, base_path, env FROM domain ORDER BY host")
            .fetch_all(&self.pool)
            .await
            .map_err(sql_error)?;
        rows.iter()
            .map(|row| {
                let project: String = row.try_get("project_id").map_err(sql_error)?;
                Ok(DomainRecord {
                    host: row.try_get("host").map_err(sql_error)?,
                    project: project_id(&project)?,
                    base_path: row.try_get("base_path").map_err(sql_error)?,
                    env: row.try_get("env").map_err(sql_error)?,
                })
            })
            .collect()
    }

    /// Moves a project from `old_host` to `new_host` and keeps the old host
    /// redirecting until `expires_at` (30 days by the caller's arithmetic).
    pub async fn rename(
        &self,
        old_host: &str,
        new_host: &str,
        expires_at: i64,
    ) -> Result<(), StoreError> {
        let mut tx = self.pool.begin().await.map_err(sql_error)?;
        let updated = sqlx::query("UPDATE domain SET host = ?, updated_at = ? WHERE host = ?")
            .bind(new_host.to_ascii_lowercase())
            .bind(now_ms())
            .bind(old_host.to_ascii_lowercase())
            .execute(&mut *tx)
            .await
            .map_err(sql_error)?;
        if updated.rows_affected() == 0 {
            return Err(StoreError::NotFound);
        }
        sqlx::query(
            "INSERT INTO domain_redirect (old_host, new_host, expires_at) VALUES (?, ?, ?) \
             ON CONFLICT (old_host) DO UPDATE SET new_host = excluded.new_host, expires_at = excluded.expires_at",
        )
        .bind(old_host.to_ascii_lowercase())
        .bind(new_host.to_ascii_lowercase())
        .bind(expires_at)
        .execute(&mut *tx)
        .await
        .map_err(sql_error)?;
        tx.commit().await.map_err(sql_error)
    }

    /// Where an old host now redirects, if the redirect has not expired.
    pub async fn redirect_for(&self, host: &str) -> Result<Option<String>, StoreError> {
        let row = sqlx::query(
            "SELECT new_host FROM domain_redirect WHERE old_host = ? AND expires_at > ?",
        )
        .bind(host.to_ascii_lowercase())
        .bind(now_ms())
        .fetch_optional(&self.pool)
        .await
        .map_err(sql_error)?;
        row.map(|r| r.try_get("new_host").map_err(sql_error))
            .transpose()
    }
}

// ---- feedback (RX-50..53) ----

#[derive(Debug, Clone)]
pub struct Feedback {
    pool: SqlitePool,
}

fn kind_text(kind: FeedbackKind) -> &'static str {
    match kind {
        FeedbackKind::Page => "page",
        FeedbackKind::Code => "code",
        FeedbackKind::Agent => "agent",
    }
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

pub fn feedback_status_text(status: FeedbackStatus) -> &'static str {
    match status {
        FeedbackStatus::Open => "open",
        FeedbackStatus::Triaged => "triaged",
        FeedbackStatus::Resolved => "resolved",
    }
}

fn feedback_from_row(row: &SqliteRow) -> Result<FeedbackRecord, StoreError> {
    let project: Option<String> = row.try_get("project_id").map_err(sql_error)?;
    let kind: String = row.try_get("kind").map_err(sql_error)?;
    let status: String = row.try_get("status").map_err(sql_error)?;
    Ok(FeedbackRecord {
        id: row.try_get("id").map_err(sql_error)?,
        project: project.and_then(|p| ProjectId::parse(&p)),
        route: row.try_get("route").map_err(sql_error)?,
        kind: match kind.as_str() {
            "page" => FeedbackKind::Page,
            "code" => FeedbackKind::Code,
            "agent" => FeedbackKind::Agent,
            other => return Err(StoreError::Sql(format!("unknown feedback kind `{other}`"))),
        },
        rating: row
            .try_get::<Option<i64>, _>("rating")
            .map_err(sql_error)?
            .map(|r| r as i32),
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

#[derive(Debug, Clone, Default)]
pub struct FeedbackQuery {
    pub route: Option<String>,
    pub kind: Option<FeedbackKind>,
    pub status: Option<FeedbackStatus>,
}

impl Feedback {
    pub fn new(pool: SqlitePool) -> Self {
        Self { pool }
    }

    pub async fn insert(&self, record: &FeedbackRecord) -> Result<(), StoreError> {
        sqlx::query(
            "INSERT INTO feedback (id, project_id, route, kind, rating, category, text, block_id, task, \
             status, notes, created_at, updated_at) VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?)",
        )
        .bind(&record.id)
        .bind(record.project.map(|p| p.to_string()))
        .bind(&record.route)
        .bind(kind_text(record.kind))
        .bind(record.rating.map(|r| r as i64))
        .bind(&record.category)
        .bind(&record.text)
        .bind(&record.block_id)
        .bind(&record.task)
        .bind(feedback_status_text(record.status))
        .bind(&record.notes)
        .bind(record.created_at)
        .bind(record.updated_at)
        .execute(&self.pool)
        .await
        .map_err(sql_error)?;
        Ok(())
    }

    pub async fn get(&self, id: &str) -> Result<Option<FeedbackRecord>, StoreError> {
        let row = sqlx::query("SELECT * FROM feedback WHERE id = ?")
            .bind(id)
            .fetch_optional(&self.pool)
            .await
            .map_err(sql_error)?;
        row.as_ref().map(feedback_from_row).transpose()
    }

    pub async fn set_status(
        &self,
        id: &str,
        status: FeedbackStatus,
        note: Option<&str>,
    ) -> Result<(), StoreError> {
        let updated = sqlx::query(
            "UPDATE feedback SET status = ?, notes = CASE WHEN ? IS NULL THEN notes ELSE notes || ? || char(10) END, \
             updated_at = ? WHERE id = ?",
        )
        .bind(feedback_status_text(status))
        .bind(note)
        .bind(note)
        .bind(now_ms())
        .bind(id)
        .execute(&self.pool)
        .await
        .map_err(sql_error)?;
        if updated.rows_affected() == 1 {
            Ok(())
        } else {
            Err(StoreError::NotFound)
        }
    }

    pub async fn list(
        &self,
        query: &FeedbackQuery,
        page: &Page,
    ) -> Result<Vec<FeedbackRecord>, StoreError> {
        let rows = sqlx::query(
            "SELECT * FROM feedback WHERE (? IS NULL OR route = ?) AND (? IS NULL OR kind = ?) \
             AND (? IS NULL OR status = ?) AND (? IS NULL OR id < ?) ORDER BY id DESC LIMIT ?",
        )
        .bind(&query.route)
        .bind(&query.route)
        .bind(query.kind.map(kind_text))
        .bind(query.kind.map(kind_text))
        .bind(query.status.map(feedback_status_text))
        .bind(query.status.map(feedback_status_text))
        .bind(&page.cursor)
        .bind(&page.cursor)
        .bind(limit(page))
        .fetch_all(&self.pool)
        .await
        .map_err(sql_error)?;
        rows.iter().map(feedback_from_row).collect()
    }

    /// Up and down counts for a route (ANA-30's ratio).
    pub async fn ratio(&self, route: &str) -> Result<(u64, u64), StoreError> {
        let row = sqlx::query(
            "SELECT SUM(CASE WHEN rating > 0 THEN 1 ELSE 0 END) AS up, \
             SUM(CASE WHEN rating < 0 THEN 1 ELSE 0 END) AS down FROM feedback WHERE route = ? AND kind = 'page'",
        )
        .bind(route)
        .fetch_one(&self.pool)
        .await
        .map_err(sql_error)?;
        let up: Option<i64> = row.try_get("up").map_err(sql_error)?;
        let down: Option<i64> = row.try_get("down").map_err(sql_error)?;
        Ok((up.unwrap_or(0) as u64, down.unwrap_or(0) as u64))
    }
}

// ---- webhooks (REST-10) ----

#[derive(Debug, Clone)]
pub struct Webhooks {
    pool: SqlitePool,
}

fn subscription_from_row(row: &SqliteRow) -> Result<WebhookSubscription, StoreError> {
    let project: Option<String> = row.try_get("project_id").map_err(sql_error)?;
    let events: String = row.try_get("events").map_err(sql_error)?;
    Ok(WebhookSubscription {
        id: row.try_get("id").map_err(sql_error)?,
        project: project.and_then(|p| ProjectId::parse(&p)),
        url: row.try_get("url").map_err(sql_error)?,
        secret: row.try_get("secret").map_err(sql_error)?,
        events: serde_json::from_str(&events).unwrap_or_default(),
        active: row.try_get::<i64, _>("active").map_err(sql_error)? != 0,
        failures: row.try_get::<i64, _>("failures").map_err(sql_error)? as u32,
        created_at: row.try_get("created_at").map_err(sql_error)?,
        updated_at: row.try_get("updated_at").map_err(sql_error)?,
    })
}

fn delivery_from_row(row: &SqliteRow) -> Result<WebhookDelivery, StoreError> {
    let status: String = row.try_get("status").map_err(sql_error)?;
    Ok(WebhookDelivery {
        id: row.try_get("id").map_err(sql_error)?,
        subscription: row.try_get("subscription_id").map_err(sql_error)?,
        event_id: row.try_get("event_id").map_err(sql_error)?,
        event_type: row.try_get("event_type").map_err(sql_error)?,
        payload: row.try_get("payload").map_err(sql_error)?,
        attempt: row.try_get::<i64, _>("attempt").map_err(sql_error)? as u32,
        next_at: row.try_get("next_at").map_err(sql_error)?,
        status: match status.as_str() {
            "pending" => DeliveryStatus::Pending,
            "delivered" => DeliveryStatus::Delivered,
            _ => DeliveryStatus::Failed,
        },
        last_status: row
            .try_get::<Option<i64>, _>("last_status")
            .map_err(sql_error)?
            .map(|s| s as u16),
        created_at: row.try_get("created_at").map_err(sql_error)?,
        updated_at: row.try_get("updated_at").map_err(sql_error)?,
    })
}

impl Webhooks {
    pub fn new(pool: SqlitePool) -> Self {
        Self { pool }
    }

    pub async fn subscribe(
        &self,
        project: Option<ProjectId>,
        url: &str,
        secret: &str,
        events: &[String],
    ) -> Result<WebhookSubscription, StoreError> {
        let now = now_ms();
        let id = format!("whs_{}", crate::new_ulid());
        let events_json =
            serde_json::to_string(events).map_err(|e| StoreError::Sql(e.to_string()))?;
        sqlx::query(
            "INSERT INTO webhook_subscription (id, project_id, url, secret, events, active, failures, created_at, updated_at) \
             VALUES (?, ?, ?, ?, ?, 1, 0, ?, ?)",
        )
        .bind(&id)
        .bind(project.map(|p| p.to_string()))
        .bind(url)
        .bind(secret)
        .bind(&events_json)
        .bind(now)
        .bind(now)
        .execute(&self.pool)
        .await
        .map_err(sql_error)?;
        Ok(WebhookSubscription {
            id,
            project,
            url: url.to_owned(),
            secret: secret.to_owned(),
            events: events.to_vec(),
            active: true,
            failures: 0,
            created_at: now,
            updated_at: now,
        })
    }

    pub async fn subscriptions(
        &self,
        project: Option<&ProjectId>,
    ) -> Result<Vec<WebhookSubscription>, StoreError> {
        let rows = sqlx::query(
            "SELECT * FROM webhook_subscription WHERE (? IS NULL OR project_id = ?) ORDER BY id",
        )
        .bind(project.map(|p| p.to_string()))
        .bind(project.map(|p| p.to_string()))
        .fetch_all(&self.pool)
        .await
        .map_err(sql_error)?;
        rows.iter().map(subscription_from_row).collect()
    }

    pub async fn get_subscription(
        &self,
        id: &str,
    ) -> Result<Option<WebhookSubscription>, StoreError> {
        let row = sqlx::query("SELECT * FROM webhook_subscription WHERE id = ?")
            .bind(id)
            .fetch_optional(&self.pool)
            .await
            .map_err(sql_error)?;
        row.as_ref().map(subscription_from_row).transpose()
    }

    pub async fn unsubscribe(&self, id: &str) -> Result<(), StoreError> {
        let mut tx = self.pool.begin().await.map_err(sql_error)?;
        sqlx::query("DELETE FROM webhook_delivery WHERE subscription_id = ?")
            .bind(id)
            .execute(&mut *tx)
            .await
            .map_err(sql_error)?;
        let deleted = sqlx::query("DELETE FROM webhook_subscription WHERE id = ?")
            .bind(id)
            .execute(&mut *tx)
            .await
            .map_err(sql_error)?;
        tx.commit().await.map_err(sql_error)?;
        if deleted.rows_affected() == 1 {
            Ok(())
        } else {
            Err(StoreError::NotFound)
        }
    }

    /// Queues one delivery per active subscription that wants `event_type`.
    pub async fn queue(
        &self,
        event_id: &str,
        event_type: &str,
        payload: &str,
    ) -> Result<Vec<WebhookDelivery>, StoreError> {
        let now = now_ms();
        let mut out = Vec::new();
        for subscription in self.subscriptions(None).await? {
            if !subscription.active
                || (!subscription.events.is_empty()
                    && !subscription
                        .events
                        .iter()
                        .any(|e| e == event_type || e == "*"))
            {
                continue;
            }
            let delivery = WebhookDelivery {
                id: format!("whd_{}", crate::new_ulid()),
                subscription: subscription.id.clone(),
                event_id: event_id.to_owned(),
                event_type: event_type.to_owned(),
                payload: payload.to_owned(),
                attempt: 0,
                next_at: now,
                status: DeliveryStatus::Pending,
                last_status: None,
                created_at: now,
                updated_at: now,
            };
            sqlx::query(
                "INSERT INTO webhook_delivery (id, subscription_id, event_id, event_type, payload, attempt, next_at, \
                 status, last_status, created_at, updated_at) VALUES (?, ?, ?, ?, ?, 0, ?, 'pending', NULL, ?, ?)",
            )
            .bind(&delivery.id)
            .bind(&delivery.subscription)
            .bind(&delivery.event_id)
            .bind(&delivery.event_type)
            .bind(&delivery.payload)
            .bind(now)
            .bind(now)
            .bind(now)
            .execute(&self.pool)
            .await
            .map_err(sql_error)?;
            out.push(delivery);
        }
        Ok(out)
    }

    pub async fn due(&self, limit_n: i64) -> Result<Vec<WebhookDelivery>, StoreError> {
        let rows = sqlx::query(
            "SELECT * FROM webhook_delivery WHERE status = 'pending' AND next_at <= ? ORDER BY next_at LIMIT ?",
        )
        .bind(now_ms())
        .bind(limit_n)
        .fetch_all(&self.pool)
        .await
        .map_err(sql_error)?;
        rows.iter().map(delivery_from_row).collect()
    }

    pub async fn deliveries(&self, subscription: &str) -> Result<Vec<WebhookDelivery>, StoreError> {
        let rows = sqlx::query(
            "SELECT * FROM webhook_delivery WHERE subscription_id = ? ORDER BY id DESC",
        )
        .bind(subscription)
        .fetch_all(&self.pool)
        .await
        .map_err(sql_error)?;
        rows.iter().map(delivery_from_row).collect()
    }

    /// Records an attempt: delivered, or retried at `next_at`, or failed for
    /// good; the subscription's consecutive-failure counter follows.
    pub async fn record_attempt(
        &self,
        delivery: &str,
        subscription: &str,
        status: Option<u16>,
        outcome: DeliveryStatus,
        next_at: i64,
    ) -> Result<(), StoreError> {
        let now = now_ms();
        let text = match outcome {
            DeliveryStatus::Pending => "pending",
            DeliveryStatus::Delivered => "delivered",
            DeliveryStatus::Failed => "failed",
        };
        let mut tx = self.pool.begin().await.map_err(sql_error)?;
        sqlx::query(
            "UPDATE webhook_delivery SET attempt = attempt + 1, next_at = ?, status = ?, last_status = ?, updated_at = ? WHERE id = ?",
        )
        .bind(next_at)
        .bind(text)
        .bind(status.map(|s| s as i64))
        .bind(now)
        .bind(delivery)
        .execute(&mut *tx)
        .await
        .map_err(sql_error)?;
        let failures_sql = if outcome == DeliveryStatus::Delivered {
            "UPDATE webhook_subscription SET failures = 0, updated_at = ? WHERE id = ?"
        } else {
            "UPDATE webhook_subscription SET failures = failures + 1, active = CASE WHEN failures + 1 >= 20 THEN 0 ELSE active END, updated_at = ? WHERE id = ?"
        };
        sqlx::query(failures_sql)
            .bind(now)
            .bind(subscription)
            .execute(&mut *tx)
            .await
            .map_err(sql_error)?;
        tx.commit().await.map_err(sql_error)
    }
}
