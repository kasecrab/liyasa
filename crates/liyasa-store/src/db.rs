//! Opening a database and bringing its schema forward (PRD §6.8, NFR-33).
//!
//! Migrations are embedded, numbered, forward-only, and applied inside one
//! `BEGIN IMMEDIATE` transaction so two replicas starting together serialize
//! on SQLite's write lock (the "advisory lock" of §6.8 on this backend).

use std::path::Path;
use std::time::Duration;

use liyasa_core::store::StoreError;
use sqlx::sqlite::{SqliteConnectOptions, SqliteJournalMode, SqlitePool, SqlitePoolOptions};
use sqlx::{Connection, Row};

/// One embedded migration: version, name, SQL.
pub type Migration = (i64, &'static str, &'static str);

/// The application schema, `migrations/NNNN_*.sql`. Append; never edit.
pub const APP: &[Migration] = &[(1, "init", include_str!("../../../migrations/0001_init.sql"))];

/// The analytics schema, `migrations/analytics/NNNN_*.sql`.
pub const ANALYTICS: &[Migration] = &[(
    1,
    "events",
    include_str!("../../../migrations/analytics/0001_events.sql"),
)];

#[derive(Debug, Clone)]
pub struct OpenOptions {
    pub max_connections: u32,
    pub busy_timeout: Duration,
    /// ANA-08: the analytics database turns automatic checkpoints off and
    /// runs its own checkpoint task between writer batches.
    pub wal_autocheckpoint: Option<u32>,
}

impl Default for OpenOptions {
    fn default() -> Self {
        Self {
            max_connections: 8,
            busy_timeout: Duration::from_secs(5),
            wal_autocheckpoint: None,
        }
    }
}

pub fn sql_error(e: sqlx::Error) -> StoreError {
    match e {
        sqlx::Error::RowNotFound => StoreError::NotFound,
        sqlx::Error::Io(e) => StoreError::Io(e.to_string()),
        other => StoreError::Sql(other.to_string()),
    }
}

/// Opens (creating if missing) `path` in WAL mode with the `busy_timeout`
/// that makes readers wait rather than fail, and applies `migrations`.
pub async fn open(
    path: &Path,
    options: &OpenOptions,
    migrations: &'static [Migration],
) -> Result<SqlitePool, StoreError> {
    let mut connect = SqliteConnectOptions::new()
        .filename(path)
        .create_if_missing(true)
        .journal_mode(SqliteJournalMode::Wal)
        .busy_timeout(options.busy_timeout)
        .foreign_keys(true);
    if let Some(pages) = options.wal_autocheckpoint {
        connect = connect.pragma("wal_autocheckpoint", pages.to_string());
    }
    let pool = SqlitePoolOptions::new()
        .max_connections(options.max_connections)
        .acquire_timeout(options.busy_timeout + Duration::from_secs(1))
        .connect_with(connect)
        .await
        .map_err(sql_error)?;
    migrate(&pool, migrations).await?;
    Ok(pool)
}

/// The versions applied so far, in order.
pub async fn applied(pool: &SqlitePool) -> Result<Vec<i64>, StoreError> {
    let rows = sqlx::query("SELECT version FROM _liyasa_migrations ORDER BY version")
        .fetch_all(pool)
        .await
        .map_err(sql_error)?;
    rows.iter()
        .map(|row| row.try_get::<i64, _>("version").map_err(sql_error))
        .collect()
}

pub async fn migrate(pool: &SqlitePool, migrations: &[Migration]) -> Result<(), StoreError> {
    let mut connection = pool.acquire().await.map_err(sql_error)?;
    sqlx::query(
        "CREATE TABLE IF NOT EXISTS _liyasa_migrations (\
            version INTEGER PRIMARY KEY, name TEXT NOT NULL, applied_at INTEGER NOT NULL)",
    )
    .execute(&mut *connection)
    .await
    .map_err(sql_error)?;
    let mut tx = connection
        .begin_with("BEGIN IMMEDIATE")
        .await
        .map_err(sql_error)?;
    let applied: Vec<i64> = sqlx::query("SELECT version FROM _liyasa_migrations")
        .fetch_all(&mut *tx)
        .await
        .map_err(sql_error)?
        .iter()
        .map(|row| row.try_get("version"))
        .collect::<Result<_, _>>()
        .map_err(sql_error)?;
    for (version, name, sql) in migrations {
        if applied.contains(version) {
            continue;
        }
        // One statement per `execute`: SQLite runs only the first statement of
        // a prepared string.
        for statement in split_statements(sql) {
            sqlx::query(sqlx::AssertSqlSafe(statement))
                .execute(&mut *tx)
                .await
                .map_err(|e| StoreError::Sql(format!("migration {version} ({name}): {e}")))?;
        }
        sqlx::query("INSERT INTO _liyasa_migrations (version, name, applied_at) VALUES (?, ?, ?)")
            .bind(version)
            .bind(name)
            .bind(crate::now_ms())
            .execute(&mut *tx)
            .await
            .map_err(sql_error)?;
    }
    tx.commit().await.map_err(sql_error)
}

/// Splits a migration file on `;` at the end of a line, dropping comment
/// lines. Statements in these files never contain a literal `;`.
fn split_statements(sql: &str) -> Vec<String> {
    let mut out = Vec::new();
    let mut current = String::new();
    for line in sql.lines() {
        let trimmed = line.trim();
        if trimmed.is_empty() || trimmed.starts_with("--") {
            continue;
        }
        current.push_str(line);
        current.push('\n');
        if trimmed.ends_with(';') {
            out.push(current.trim().to_owned());
            current.clear();
        }
    }
    if !current.trim().is_empty() {
        out.push(current.trim().to_owned());
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn statements_split_on_terminating_semicolons_only() {
        let sql = "-- comment\nCREATE TABLE a (\n  x INTEGER -- trailing\n);\n\nCREATE INDEX b ON a(x);\n";
        let parts = split_statements(sql);
        assert_eq!(parts.len(), 2);
        assert!(parts[0].starts_with("CREATE TABLE a"));
        assert!(parts[1].starts_with("CREATE INDEX b"));
    }

    #[test]
    fn migration_versions_are_strictly_increasing() {
        for set in [APP, ANALYTICS] {
            let versions: Vec<i64> = set.iter().map(|m| m.0).collect();
            let mut sorted = versions.clone();
            sorted.sort_unstable();
            sorted.dedup();
            assert_eq!(versions, sorted);
        }
    }
}
