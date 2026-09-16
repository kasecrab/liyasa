//! Listening, draining, and the background workers (HOST-02, NFR-31).
//!
//! On `SIGTERM` the server stops accepting, lets in-flight requests finish
//! inside `server.drainTimeout`, flushes the ingest queue, releases its job
//! leases so another replica claims them without waiting for expiry, and
//! exits.

use std::net::SocketAddr;
use std::sync::Arc;
use std::time::Duration;

use axum::Router;
use liyasa_core::net::{HostSet, HttpPolicy, Purpose};
use liyasa_store::ingest::{IngestOptions, Writer};
use tokio::net::TcpListener;
use tokio::sync::watch;

use super::{AppState, webhooks};

/// Everything a running server owns besides the router.
pub struct Runtime {
    pub state: Arc<AppState>,
    shutdown_tx: watch::Sender<bool>,
    shutdown_rx: watch::Receiver<bool>,
    tasks: Vec<tokio::task::JoinHandle<()>>,
}

impl Runtime {
    pub fn new(state: Arc<AppState>) -> Self {
        let (shutdown_tx, shutdown_rx) = watch::channel(false);
        Self {
            state,
            shutdown_tx,
            shutdown_rx,
            tasks: Vec::new(),
        }
    }

    pub fn shutdown_signal(&self) -> watch::Receiver<bool> {
        self.shutdown_rx.clone()
    }

    /// The analytics batch writer: one per instance (ANA-08).
    pub fn spawn_ingest(&mut self, writer: Writer) {
        let rx = self.shutdown_rx.clone();
        self.tasks.push(tokio::spawn(writer.run(rx)));
    }

    /// Opens `analytics.db` beside the application database and starts the
    /// writer.
    pub async fn spawn_ingest_at(
        &mut self,
        path: &std::path::Path,
        options: IngestOptions,
    ) -> Result<(), liyasa_core::store::StoreError> {
        let writer = Writer::open(path, self.state.ingest.clone(), options).await?;
        self.spawn_ingest(writer);
        Ok(())
    }

    /// Retries due webhook deliveries (REST-10). Offline instances have no
    /// outbound path, so the worker does not start (HOST-08).
    pub fn spawn_webhooks(&mut self, http: Arc<dyn liyasa_core::net::HttpClient>) {
        let Some(store) = self.state.store.clone() else {
            return;
        };
        if self.state.config.offline {
            tracing::info!(
                target: "liyasa_server",
                "offline: the webhook worker is not started"
            );
            return;
        }
        let mut rx = self.shutdown_rx.clone();
        self.tasks.push(tokio::spawn(async move {
            loop {
                tokio::select! {
                    _ = tokio::time::sleep(Duration::from_secs(10)) => {}
                    _ = rx.changed() => return,
                }
                if let Err(error) = webhooks::deliver_due(&store, http.as_ref(), 32).await {
                    tracing::warn!(target: "liyasa_server", %error, "the webhook worker failed a pass");
                }
            }
        }));
    }

    /// Rotates the analytics salt at midnight UTC (ANA-03).
    pub fn spawn_salt_rotation(&mut self) {
        let state = self.state.clone();
        let mut rx = self.shutdown_rx.clone();
        self.tasks.push(tokio::spawn(async move {
            loop {
                let wait = millis_until_midnight();
                tokio::select! {
                    _ = tokio::time::sleep(wait) => state.salt.rotate(),
                    _ = rx.changed() => return,
                }
            }
        }));
    }

    /// Exports buffered spans to the collector (HOST-05, RFC 1401).
    pub fn spawn_trace_export(
        &mut self,
        http: Arc<dyn liyasa_core::net::HttpClient>,
        endpoint: liyasa_core::net::Url,
    ) {
        if !self.state.tracer.is_enabled() || self.state.config.offline {
            return;
        }
        let state = self.state.clone();
        let mut rx = self.shutdown_rx.clone();
        self.tasks.push(tokio::spawn(async move {
            loop {
                tokio::select! {
                    _ = tokio::time::sleep(Duration::from_secs(5)) => {}
                    _ = rx.changed() => {
                        export(&state, http.as_ref(), &endpoint).await;
                        return;
                    }
                }
                export(&state, http.as_ref(), &endpoint).await;
            }
        }));
    }

    /// Sends whatever spans are buffered, once. The export loop calls this on
    /// a timer; a test calls it directly rather than waiting on one.
    pub async fn export_once(
        &self,
        http: &dyn liyasa_core::net::HttpClient,
        endpoint: &liyasa_core::net::Url,
    ) {
        export(&self.state, http, endpoint).await;
    }

    /// Serves until `shutdown` fires, then drains.
    pub async fn serve(mut self, listener: TcpListener, router: Router) -> std::io::Result<()> {
        let state = self.state.clone();
        let mut rx = self.shutdown_rx.clone();
        let service = router.into_make_service_with_connect_info::<SocketAddr>();
        let result = axum::serve(listener, service)
            .with_graceful_shutdown(async move {
                let _ = rx.changed().await;
                state.begin_drain();
            })
            .await;
        self.drain().await;
        result
    }

    /// Asks every task to stop and waits for them, then hands back the job
    /// leases this replica holds.
    pub async fn drain(&mut self) {
        self.state.begin_drain();
        let _ = self.shutdown_tx.send(true);
        let timeout = self.state.config.drain_timeout;
        for task in self.tasks.drain(..) {
            let _ = tokio::time::timeout(timeout, task).await;
        }
        if let Some(store) = &self.state.store {
            match store.jobs_typed().release(&worker_name()).await {
                Ok(0) => {}
                Ok(n) => {
                    tracing::info!(target: "liyasa_server", released = n, "job leases handed back")
                }
                Err(error) => {
                    tracing::warn!(target: "liyasa_server", %error, "job leases could not be released")
                }
            }
        }
    }

    /// Fires the shutdown signal without waiting, which is what a `SIGTERM`
    /// handler does.
    pub fn stop(&self) {
        let _ = self.shutdown_tx.send(true);
    }

    /// A handle that can stop the runtime from another task.
    pub fn stopper(&self) -> Stopper {
        Stopper(self.shutdown_tx.clone())
    }
}

/// Stops a [`Runtime`] from a signal handler or a test.
#[derive(Debug, Clone)]
pub struct Stopper(watch::Sender<bool>);

impl Stopper {
    pub fn stop(&self) {
        let _ = self.0.send(true);
    }
}

async fn export(
    state: &AppState,
    http: &dyn liyasa_core::net::HttpClient,
    endpoint: &liyasa_core::net::Url,
) {
    let spans = state.tracer.take();
    if spans.is_empty() {
        return;
    }
    let payload = state.tracer.payload(&spans);
    let Ok(body) = serde_json::to_vec(&payload) else {
        return;
    };
    let request = liyasa_core::net::HttpRequest {
        method: liyasa_core::net::Method::POST,
        url: endpoint.clone(),
        headers: vec![("content-type".to_owned(), "application/json".to_owned())],
        body: Some(body.into()),
    };
    let policy = HttpPolicy {
        allow_hosts: HostSet::default(),
        deny_hosts: HostSet::default(),
        // A collector is usually a sidecar on the same host or network, so
        // this is the one purpose that reaches private space by design.
        allow_private: true,
        max_redirects: 0,
        max_bytes: 64 * 1024,
        timeout: Duration::from_secs(10),
        purpose: Purpose::Webhook,
    };
    if let Err(error) = http.fetch(request, &policy).await {
        tracing::debug!(target: "liyasa_server", %error, "traces could not be exported");
    }
}

/// This replica's name in the job table. The hostname on Kubernetes, which is
/// the pod name and therefore unique per replica (HOST-03).
pub fn worker_name() -> String {
    std::env::var("LIYASA_WORKER")
        .or_else(|_| std::env::var("HOSTNAME"))
        .unwrap_or_else(|_| format!("worker-{}", std::process::id()))
}

fn millis_until_midnight() -> Duration {
    let now = liyasa_store::now_ms();
    let day = 86_400_000;
    let next = (now / day + 1) * day;
    Duration::from_millis((next - now).max(1) as u64)
}

/// `SIGTERM` and `SIGINT`; on a platform without signals, only `ctrl_c`.
pub async fn shutdown_signal() {
    #[cfg(unix)]
    {
        use tokio::signal::unix::{SignalKind, signal};
        let mut term = match signal(SignalKind::terminate()) {
            Ok(term) => term,
            Err(_) => {
                let _ = tokio::signal::ctrl_c().await;
                return;
            }
        };
        tokio::select! {
            _ = term.recv() => {}
            _ = tokio::signal::ctrl_c() => {}
        }
    }
    #[cfg(not(unix))]
    {
        let _ = tokio::signal::ctrl_c().await;
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn midnight_is_always_ahead_and_within_a_day() {
        let wait = millis_until_midnight();
        assert!(wait.as_millis() > 0);
        assert!(wait.as_millis() <= 86_400_000);
    }

    #[test]
    fn a_worker_names_itself_from_the_environment_or_its_process() {
        let name = worker_name();
        assert!(!name.is_empty());
    }
}
