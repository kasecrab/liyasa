//! Storage for drift records (VER-23, VER-77).
//!
//! This holds the SQL and nothing else: a key, whether the record is open, and
//! the document. It deliberately does not name `DriftRecord` — that type lives
//! in `liyasa-verify`, which does not depend on this crate, and keeping the
//! dependency out means the only crate that opens a database stays the only
//! crate that opens a database. The caller serialises and implements
//! `RecordStore` over these four calls.

use sqlx::Row;
use sqlx::sqlite::SqlitePool;

use crate::db::sql_error;
use liyasa_core::store::StoreError;

/// One stored record: its key, whether it is open, and its JSON document.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct StoredRecord {
    pub key: String,
    pub open: bool,
    pub document: String,
}

#[derive(Debug, Clone)]
pub struct DriftRecords {
    pool: SqlitePool,
}

impl DriftRecords {
    pub fn new(pool: SqlitePool) -> Self {
        Self { pool }
    }

    /// Every record, open or resolved, in key order.
    pub async fn all(&self) -> Result<Vec<StoredRecord>, StoreError> {
        let rows = sqlx::query("SELECT key, open, document FROM drift_record ORDER BY key")
            .fetch_all(&self.pool)
            .await
            .map_err(sql_error)?;
        rows.iter().map(row_to_record).collect()
    }

    /// The open ones. Filtered in SQL against the index rather than by
    /// deserialising every record the site has ever had.
    pub async fn open(&self) -> Result<Vec<StoredRecord>, StoreError> {
        let rows =
            sqlx::query("SELECT key, open, document FROM drift_record WHERE open = 1 ORDER BY key")
                .fetch_all(&self.pool)
                .await
                .map_err(sql_error)?;
        rows.iter().map(row_to_record).collect()
    }

    pub async fn find(&self, key: &str) -> Result<Option<StoredRecord>, StoreError> {
        let row = sqlx::query("SELECT key, open, document FROM drift_record WHERE key = ?")
            .bind(key)
            .fetch_optional(&self.pool)
            .await
            .map_err(sql_error)?;
        row.as_ref().map(row_to_record).transpose()
    }

    /// One reconciliation's writes, in one transaction.
    ///
    /// All-or-nothing on purpose: a run is one logical operation, and a
    /// failure halfway through used to leave some of its records written and
    /// some not. An empty slice still opens and commits, so a caller sees the
    /// same shape every run rather than a transaction that sometimes happens.
    pub async fn save_all(&self, records: &[StoredRecord]) -> Result<(), StoreError> {
        let now = crate::now_ms();
        let mut tx = self.pool.begin().await.map_err(sql_error)?;
        for record in records {
            sqlx::query(
                "INSERT INTO drift_record (key, open, document, updated_at) \
                 VALUES (?, ?, ?, ?) \
                 ON CONFLICT(key) DO UPDATE SET \
                 open = excluded.open, document = excluded.document, \
                 updated_at = excluded.updated_at",
            )
            .bind(&record.key)
            .bind(i64::from(record.open))
            .bind(&record.document)
            .bind(now)
            .execute(&mut *tx)
            .await
            .map_err(sql_error)?;
        }
        tx.commit().await.map_err(sql_error)
    }
}

fn row_to_record(row: &sqlx::sqlite::SqliteRow) -> Result<StoredRecord, StoreError> {
    let open: i64 = row.try_get("open").map_err(sql_error)?;
    Ok(StoredRecord {
        key: row.try_get("key").map_err(sql_error)?,
        open: open != 0,
        document: row.try_get("document").map_err(sql_error)?,
    })
}
