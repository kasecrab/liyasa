//! The analytics ingest path (ANA-08).
//!
//! Events go into a bounded in-memory buffer and one batch writer per
//! instance flushes them into `analytics.db` in transactions of up to
//! `batch` events or every second, whichever comes first. The buffer is per
//! instance and lossy by design (HOST-03): under pressure raw events spill to
//! hourly newline-delimited JSON segment files and only aggregates reach the
//! database; only when that sink is unavailable does dropping begin, lowest
//! class first, and the drops are counted.

use std::collections::VecDeque;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicU64, AtomicUsize, Ordering};
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};

use liyasa_core::store::{Event, EventSink, QueueFull, StoreError};
use sqlx::Row;
use sqlx::sqlite::SqlitePool;
use tokio::sync::Notify;

use crate::db::{self, OpenOptions, sql_error};
use crate::now_ms;
use crate::records::{EventClass, EventRecord};

/// `analytics.rawSink`.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum RawSink {
    /// Raw rows always go to the database.
    Database,
    /// Raw rows always go to segment files; the database holds aggregates.
    Files(PathBuf),
    /// Database until the buffer stays above 80% for 10 s, files until it
    /// recovers (the default).
    Auto(PathBuf),
}

#[derive(Debug, Clone)]
pub struct IngestOptions {
    /// Ring capacity; 100,000 by default.
    pub capacity: usize,
    /// Events per transaction; 5,000 by default.
    pub batch: usize,
    pub flush_every: Duration,
    pub raw_sink: RawSink,
    /// PASSIVE checkpoint cadence; 10 s.
    pub checkpoint_every: Duration,
    /// TRUNCATE once the WAL passes this; 256 MB.
    pub wal_truncate_bytes: u64,
    pub pressure_threshold: f64,
    pub pressure_window: Duration,
}

impl Default for IngestOptions {
    fn default() -> Self {
        Self {
            capacity: 100_000,
            batch: 5_000,
            flush_every: Duration::from_secs(1),
            raw_sink: RawSink::Database,
            checkpoint_every: Duration::from_secs(10),
            wal_truncate_bytes: 256 * 1024 * 1024,
            pressure_threshold: 0.8,
            pressure_window: Duration::from_secs(10),
        }
    }
}

#[derive(Default)]
struct Buffer {
    interaction: VecDeque<EventRecord>,
    search_or_view: VecDeque<EventRecord>,
    critical: VecDeque<EventRecord>,
}

impl Buffer {
    fn len(&self) -> usize {
        self.interaction.len() + self.search_or_view.len() + self.critical.len()
    }

    fn lane(&mut self, class: EventClass) -> &mut VecDeque<EventRecord> {
        match class {
            EventClass::Interaction => &mut self.interaction,
            EventClass::SearchOrView => &mut self.search_or_view,
            EventClass::Critical => &mut self.critical,
        }
    }

    /// Evicts one event of the lowest class present below `incoming`, or
    /// reports that nothing may be evicted.
    fn make_room_for(&mut self, incoming: EventClass) -> Option<EventClass> {
        [EventClass::Interaction, EventClass::SearchOrView]
            .into_iter()
            .filter(|class| *class < incoming)
            .find(|class| self.lane(*class).pop_front().is_some())
    }

    fn drain(&mut self, max: usize) -> Vec<EventRecord> {
        let mut out = Vec::with_capacity(max.min(self.len()));
        for lane in [
            &mut self.critical,
            &mut self.search_or_view,
            &mut self.interaction,
        ] {
            while out.len() < max {
                match lane.pop_front() {
                    Some(event) => out.push(event),
                    None => break,
                }
            }
        }
        out
    }
}

/// Counters the metrics endpoint exposes.
#[derive(Debug, Default)]
pub struct IngestMetrics {
    pub received: AtomicU64,
    pub written: AtomicU64,
    pub spilled: AtomicU64,
    pub dropped_interaction: AtomicU64,
    pub dropped_search_or_view: AtomicU64,
    pub dropped_critical: AtomicU64,
    pub depth: AtomicUsize,
    pub capacity: AtomicUsize,
    pub batches: AtomicU64,
    pub spilling: AtomicU64,
}

impl IngestMetrics {
    pub fn dropped(&self) -> u64 {
        self.dropped_interaction.load(Ordering::Relaxed)
            + self.dropped_search_or_view.load(Ordering::Relaxed)
            + self.dropped_critical.load(Ordering::Relaxed)
    }
}

/// The producer side: what request handlers hold.
#[derive(Clone)]
pub struct IngestQueue {
    buffer: Arc<Mutex<Buffer>>,
    notify: Arc<Notify>,
    capacity: usize,
    batch: usize,
    pub metrics: Arc<IngestMetrics>,
}

impl std::fmt::Debug for IngestQueue {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("IngestQueue")
            .field("capacity", &self.capacity)
            .finish_non_exhaustive()
    }
}

impl IngestQueue {
    pub fn new(capacity: usize, batch: usize) -> Self {
        let metrics = Arc::new(IngestMetrics::default());
        metrics.capacity.store(capacity, Ordering::Relaxed);
        Self {
            buffer: Arc::new(Mutex::new(Buffer::default())),
            notify: Arc::new(Notify::new()),
            capacity,
            batch,
            metrics,
        }
    }

    /// Never blocks and never touches the database: the request path is
    /// done once the event is in the buffer (ANA-01).
    pub fn push(&self, event: EventRecord) -> Result<(), QueueFull> {
        self.metrics.received.fetch_add(1, Ordering::Relaxed);
        let class = event.class();
        let mut buffer = self.buffer.lock().unwrap_or_else(|e| e.into_inner());
        if buffer.len() >= self.capacity {
            match buffer.make_room_for(class) {
                Some(evicted) => self.count_drop(evicted),
                None => {
                    self.count_drop(class);
                    return Err(QueueFull);
                }
            }
        }
        buffer.lane(class).push_back(event);
        let depth = buffer.len();
        drop(buffer);
        self.metrics.depth.store(depth, Ordering::Relaxed);
        if depth >= self.batch {
            self.notify.notify_one();
        }
        Ok(())
    }

    fn count_drop(&self, class: EventClass) {
        let counter = match class {
            EventClass::Interaction => &self.metrics.dropped_interaction,
            EventClass::SearchOrView => &self.metrics.dropped_search_or_view,
            EventClass::Critical => &self.metrics.dropped_critical,
        };
        counter.fetch_add(1, Ordering::Relaxed);
    }

    pub fn depth(&self) -> usize {
        self.buffer.lock().unwrap_or_else(|e| e.into_inner()).len()
    }

    /// Takes up to `max` events, highest class first.
    pub fn drain(&self, max: usize) -> Vec<EventRecord> {
        let mut buffer = self.buffer.lock().unwrap_or_else(|e| e.into_inner());
        let out = buffer.drain(max);
        self.metrics.depth.store(buffer.len(), Ordering::Relaxed);
        out
    }

    /// Wakes the writer early, for a drain on shutdown.
    pub fn kick(&self) {
        self.notify.notify_one();
    }
}

// TODO(rfc-1400): `Event` carries no fields; the marker is stored as an
// interaction event of type `event` until the fields land.
impl EventSink for IngestQueue {
    fn push(&self, _e: Event) -> Result<(), QueueFull> {
        IngestQueue::push(
            self,
            EventRecord {
                ts: now_ms(),
                kind: "event".to_owned(),
                ..EventRecord::default()
            },
        )
    }
}

/// The consumer side: one per instance, owning the analytics pool.
pub struct Writer {
    pool: SqlitePool,
    queue: IngestQueue,
    options: IngestOptions,
    db_path: PathBuf,
    over_since: Option<Instant>,
    spilling: bool,
}

impl Writer {
    /// Opens `analytics.db` with its own pool, automatic checkpoints off.
    pub async fn open(
        path: &Path,
        queue: IngestQueue,
        options: IngestOptions,
    ) -> Result<Self, StoreError> {
        let pool = db::open(
            path,
            &OpenOptions {
                max_connections: 4,
                wal_autocheckpoint: Some(0),
                ..OpenOptions::default()
            },
            db::ANALYTICS,
        )
        .await?;
        Ok(Self {
            pool,
            queue,
            options,
            db_path: path.to_owned(),
            over_since: None,
            spilling: false,
        })
    }

    pub fn pool(&self) -> &SqlitePool {
        &self.pool
    }

    /// Writes one batch if anything is buffered. Returns the number written.
    pub async fn flush(&mut self) -> Result<usize, StoreError> {
        self.update_pressure();
        let events = self.queue.drain(self.options.batch);
        if events.is_empty() {
            return Ok(0);
        }
        let spill_dir = match (&self.options.raw_sink, self.spilling) {
            (RawSink::Files(dir), _) => Some(dir.clone()),
            (RawSink::Auto(dir), true) => Some(dir.clone()),
            _ => None,
        };
        let mut raw_to_db = spill_dir.is_none();
        if let Some(dir) = &spill_dir
            && let Err(e) = write_segment(dir, &events)
        {
            tracing::warn!(target: "liyasa_store", error = %e, "segment sink unavailable; raw rows go to the database");
            raw_to_db = true;
        }
        if spill_dir.is_some() && !raw_to_db {
            self.queue
                .metrics
                .spilled
                .fetch_add(events.len() as u64, Ordering::Relaxed);
        }
        let mut tx = self.pool.begin().await.map_err(sql_error)?;
        for event in &events {
            if raw_to_db {
                insert_event(&mut tx, event).await?;
            }
            let hour = event.ts - event.ts.rem_euclid(3_600_000);
            let caller_kind = event
                .caller
                .get("kind")
                .and_then(|k| k.as_str())
                .unwrap_or("human")
                .to_owned();
            sqlx::query(
                "INSERT INTO agg_hour (hour, site, env, route, type, caller_kind, format, count) \
                 VALUES (?, ?, ?, ?, ?, ?, ?, 1) \
                 ON CONFLICT (hour, site, env, route, type, caller_kind, format) DO UPDATE SET count = count + 1",
            )
            .bind(hour)
            .bind(&event.site)
            .bind(&event.env)
            .bind(&event.route)
            .bind(&event.kind)
            .bind(caller_kind)
            .bind(&event.format)
            .execute(&mut *tx)
            .await
            .map_err(sql_error)?;
        }
        let day = now_ms() / 86_400_000;
        for (class, counter) in [
            ("interaction", &self.queue.metrics.dropped_interaction),
            ("search_or_view", &self.queue.metrics.dropped_search_or_view),
            ("critical", &self.queue.metrics.dropped_critical),
        ] {
            let dropped = counter.swap(0, Ordering::Relaxed);
            if dropped > 0 {
                sqlx::query(
                    "INSERT INTO ingest_drops (day, class, count) VALUES (?, ?, ?) \
                     ON CONFLICT (day, class) DO UPDATE SET count = count + excluded.count",
                )
                .bind(day)
                .bind(class)
                .bind(dropped as i64)
                .execute(&mut *tx)
                .await
                .map_err(sql_error)?;
                // Put the total back so the metric keeps counting up.
                counter.fetch_add(dropped, Ordering::Relaxed);
            }
        }
        tx.commit().await.map_err(sql_error)?;
        self.queue
            .metrics
            .written
            .fetch_add(events.len() as u64, Ordering::Relaxed);
        self.queue.metrics.batches.fetch_add(1, Ordering::Relaxed);
        Ok(events.len())
    }

    fn update_pressure(&mut self) {
        let depth = self.queue.depth();
        let over = depth as f64 >= self.options.pressure_threshold * self.options.capacity as f64;
        match (over, self.over_since) {
            (true, None) => self.over_since = Some(Instant::now()),
            (true, Some(since)) if since.elapsed() >= self.options.pressure_window => {
                if !self.spilling && matches!(self.options.raw_sink, RawSink::Auto(_)) {
                    tracing::warn!(target: "liyasa_store", depth, "ingest buffer under pressure; raw events spill to segment files");
                    self.spilling = true;
                    self.queue.metrics.spilling.store(1, Ordering::Relaxed);
                }
            }
            (false, _) => {
                self.over_since = None;
                if self.spilling {
                    self.spilling = false;
                    self.queue.metrics.spilling.store(0, Ordering::Relaxed);
                }
            }
            _ => {}
        }
    }

    /// `PRAGMA wal_checkpoint(PASSIVE)`, or `TRUNCATE` once the WAL is past
    /// the cap. Called between batches, never during one.
    pub async fn checkpoint(&self) -> Result<(), StoreError> {
        let wal = std::fs::metadata(format!("{}-wal", self.db_path.display()))
            .map(|m| m.len())
            .unwrap_or(0);
        let mode = if wal > self.options.wal_truncate_bytes {
            "TRUNCATE"
        } else {
            "PASSIVE"
        };
        sqlx::query(sqlx::AssertSqlSafe(format!(
            "PRAGMA wal_checkpoint({mode})"
        )))
        .execute(&self.pool)
        .await
        .map_err(sql_error)?;
        Ok(())
    }

    /// The writer loop: flush on a full batch or the interval, checkpoint on
    /// its own cadence, drain on shutdown.
    pub async fn run(mut self, mut shutdown: tokio::sync::watch::Receiver<bool>) {
        let mut last_checkpoint = Instant::now();
        loop {
            let notified = self.queue.notify.notified();
            tokio::select! {
                _ = notified => {}
                _ = tokio::time::sleep(self.options.flush_every) => {}
                _ = shutdown.changed() => {
                    while self.queue.depth() > 0 {
                        if let Err(e) = self.flush().await {
                            tracing::error!(target: "liyasa_store", error = %e, "final flush failed");
                            break;
                        }
                    }
                    let _ = self.checkpoint().await;
                    return;
                }
            }
            loop {
                match self.flush().await {
                    Ok(n) if n >= self.options.batch => continue,
                    Ok(_) => break,
                    Err(e) => {
                        tracing::error!(target: "liyasa_store", error = %e, "batch write failed");
                        break;
                    }
                }
            }
            if last_checkpoint.elapsed() >= self.options.checkpoint_every {
                if let Err(e) = self.checkpoint().await {
                    tracing::warn!(target: "liyasa_store", error = %e, "checkpoint failed");
                }
                last_checkpoint = Instant::now();
            }
        }
    }

    /// Hourly counts for a route, from the aggregate table only.
    pub async fn hourly(&self, site: &str, route: &str) -> Result<Vec<(i64, i64)>, StoreError> {
        let rows = sqlx::query(
            "SELECT hour, SUM(count) AS n FROM agg_hour WHERE site = ? AND route = ? GROUP BY hour ORDER BY hour",
        )
        .bind(site)
        .bind(route)
        .fetch_all(&self.pool)
        .await
        .map_err(sql_error)?;
        rows.iter()
            .map(|row| {
                Ok((
                    row.try_get::<i64, _>("hour").map_err(sql_error)?,
                    row.try_get::<i64, _>("n").map_err(sql_error)?,
                ))
            })
            .collect()
    }
}

async fn insert_event(
    tx: &mut sqlx::Transaction<'_, sqlx::Sqlite>,
    event: &EventRecord,
) -> Result<(), StoreError> {
    let json = |v: &serde_json::Value| serde_json::to_string(v).unwrap_or_else(|_| "{}".to_owned());
    sqlx::query(
        "INSERT INTO event (ts, site, env, route, type, variant, caller, format, session_key, \
         referrer_host, device, country, duration_ms, props) VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?)",
    )
    .bind(event.ts)
    .bind(&event.site)
    .bind(&event.env)
    .bind(&event.route)
    .bind(&event.kind)
    .bind(json(&event.variant))
    .bind(json(&event.caller))
    .bind(&event.format)
    .bind(&event.session_key)
    .bind(&event.referrer_host)
    .bind(json(&event.device))
    .bind(&event.country)
    .bind(event.duration_ms.map(|d| d as i64))
    .bind(json(&event.props))
    .execute(&mut **tx)
    .await
    .map_err(sql_error)?;
    Ok(())
}

/// Appends `events` to the current hour's segment file as one JSON object
/// per line, the same shape the S3 stream uses.
pub fn write_segment(dir: &Path, events: &[EventRecord]) -> Result<PathBuf, std::io::Error> {
    use std::io::Write as _;
    std::fs::create_dir_all(dir)?;
    let hour = now_ms() / 3_600_000;
    let path = dir.join(format!("events-{hour}.ndjson"));
    let mut file = std::fs::OpenOptions::new()
        .create(true)
        .append(true)
        .open(&path)?;
    let mut out = String::new();
    for event in events {
        if let Ok(line) = serde_json::to_string(event) {
            out.push_str(&line);
            out.push('\n');
        }
    }
    file.write_all(out.as_bytes())?;
    Ok(path)
}
