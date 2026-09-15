//! `RenderPool`: the only CPU pool in the request path (PRD §6.6.3 item 5,
//! §34.9).
//!
//! The server constructs one and borrows it everywhere. Static routes never
//! touch it. A job that waits longer than `server.dynamic.queueTimeout`, finds
//! the queue full, runs past its CPU budget, or writes past its output budget
//! comes back `RenderError::Budget`, which is what lets the server fall back to
//! the anonymous render instead of holding a reader's connection open.
//!
//! `plan/rfcs/0602-render-pool-implementation-home.md` records why the
//! implementation lives here and how the renderer is injected.

use std::collections::VecDeque;
use std::future::Future;
use std::pin::Pin;
use std::sync::{Arc, Condvar, Mutex};
use std::task::{Context, Poll, Waker};
use std::time::{Duration, Instant};

use liyasa_core::build::{PoolMetrics, RenderJob, Rendered};
use liyasa_core::components::RenderError;
use liyasa_core::net::BoxFut;

/// What turns a job into a rendering. The server closes over the source map,
/// the component registry, and the theme it already owns.
// TODO(rfc-0602): the frozen `new` takes none of them, so they arrive here.
pub type RenderFn = Arc<dyn Fn(RenderJob) -> Result<Rendered, RenderError> + Send + Sync>;

/// How many waits the p95 is computed over. A window rather than a histogram:
/// the number is for a dashboard, and a ring of 256 durations is 4 KB.
const WAIT_WINDOW: usize = 256;

#[derive(Default)]
struct Metrics {
    depth: usize,
    rejected: u64,
    waits: VecDeque<Duration>,
}

impl Metrics {
    fn record_wait(&mut self, waited: Duration) {
        if self.waits.len() == WAIT_WINDOW {
            self.waits.pop_front();
        }
        self.waits.push_back(waited);
    }

    fn wait_p95(&self) -> Duration {
        if self.waits.is_empty() {
            return Duration::ZERO;
        }
        let mut sorted: Vec<Duration> = self.waits.iter().copied().collect();
        sorted.sort_unstable();
        let at = (sorted.len() * 95).div_ceil(100).saturating_sub(1);
        sorted.get(at).copied().unwrap_or_default()
    }
}

pub struct RenderPool {
    pool: rayon::ThreadPool,
    renderer: Option<RenderFn>,
    queue: usize,
    queue_timeout: Duration,
    metrics: Arc<Mutex<Metrics>>,
}

impl std::fmt::Debug for RenderPool {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("RenderPool")
            .field("threads", &self.pool.current_num_threads())
            .field("queue", &self.queue)
            .field("queue_timeout", &self.queue_timeout)
            .field("renderer", &self.renderer.is_some())
            .finish()
    }
}

impl RenderPool {
    /// The frozen constructor. A pool built this way has no renderer, so every
    /// job it is given comes back as an error rather than a panic.
    pub fn new(threads: usize, queue: usize, queue_timeout: Duration) -> Self {
        Self::build(threads, queue, queue_timeout, None)
    }

    /// The constructor the server uses.
    pub fn with_renderer(
        threads: usize,
        queue: usize,
        queue_timeout: Duration,
        renderer: RenderFn,
    ) -> Self {
        Self::build(threads, queue, queue_timeout, Some(renderer))
    }

    fn build(
        threads: usize,
        queue: usize,
        queue_timeout: Duration,
        renderer: Option<RenderFn>,
    ) -> Self {
        let threads = threads.max(1);
        let pool = rayon::ThreadPoolBuilder::new()
            .num_threads(threads)
            .thread_name(|at| format!("liyasa-render-{at}"))
            .build()
            .unwrap_or_else(|_| {
                // A pool that will not start is not a reason to lose the
                // process: one thread still renders, just without parallelism.
                rayon::ThreadPoolBuilder::new()
                    .num_threads(1)
                    .build_global()
                    .ok();
                rayon::ThreadPoolBuilder::new()
                    .num_threads(1)
                    .build()
                    .unwrap_or_else(|error| unreachable!("a one-thread pool: {error}"))
            });
        Self {
            pool,
            renderer,
            queue: queue.max(1),
            queue_timeout,
            metrics: Arc::new(Mutex::new(Metrics::default())),
        }
    }

    pub fn threads(&self) -> usize {
        self.pool.current_num_threads()
    }

    /// Queues one render. `Err(RenderError::Budget)` on a full queue, a wait
    /// past the timeout, a CPU overrun, or an output overrun.
    pub fn submit<'a>(&'a self, job: RenderJob) -> BoxFut<'a, Result<Rendered, RenderError>> {
        let Some(renderer) = self.renderer.clone() else {
            return Box::pin(std::future::ready(Err(RenderError::Component(
                "renderer".to_owned(),
            ))));
        };

        {
            let Ok(mut metrics) = self.metrics.lock() else {
                return Box::pin(std::future::ready(Err(RenderError::Budget)));
            };
            if metrics.depth >= self.queue {
                metrics.rejected += 1;
                return Box::pin(std::future::ready(Err(RenderError::Budget)));
            }
            metrics.depth += 1;
        }

        let slot = Slot::new();
        let filled = slot.clone();
        let metrics = Arc::clone(&self.metrics);
        let queue_timeout = self.queue_timeout;
        let queued_at = Instant::now();

        self.pool.spawn(move || {
            let waited = queued_at.elapsed();
            let budget = job.budget;
            let result = if waited > queue_timeout {
                Err(RenderError::Budget)
            } else {
                run(&renderer, job, budget)
            };
            if let Ok(mut metrics) = metrics.lock() {
                metrics.depth = metrics.depth.saturating_sub(1);
                metrics.record_wait(waited);
            }
            filled.fill(result);
        });

        Box::pin(slot)
    }

    pub fn metrics(&self) -> PoolMetrics {
        match self.metrics.lock() {
            Ok(metrics) => PoolMetrics {
                depth: metrics.depth,
                wait_p95: metrics.wait_p95(),
                rejected: metrics.rejected,
            },
            Err(_) => PoolMetrics::default(),
        }
    }
}

/// Runs one job and holds it to its budget.
///
/// The CPU budget is measured as wall time on this thread: the job owns the
/// thread for its whole run, and std has no per-thread CPU clock. A render
/// cannot be preempted, so an overrun is caught on the way out — which is
/// enough, because the point is to stop serving the result and demote the page,
/// not to interrupt the work.
fn run(
    renderer: &RenderFn,
    job: RenderJob,
    budget: liyasa_core::build::RenderBudget,
) -> Result<Rendered, RenderError> {
    let started = Instant::now();
    let mut rendered = renderer(job)?;
    let elapsed = started.elapsed();
    if rendered.cpu.is_zero() {
        rendered.cpu = elapsed;
    }
    if rendered.cpu > budget.cpu || rendered.body.len() as u64 > budget.max_bytes {
        return Err(RenderError::Budget);
    }
    Ok(rendered)
}

/// A one-shot slot a rayon thread fills and a future reads. Nothing in the
/// workspace pulls a channel crate in for this.
struct Slot<T>(Arc<SlotState<T>>);

struct SlotState<T> {
    value: Mutex<Option<T>>,
    waker: Mutex<Option<Waker>>,
    ready: Condvar,
}

impl<T> Clone for Slot<T> {
    fn clone(&self) -> Self {
        Self(Arc::clone(&self.0))
    }
}

impl<T> Slot<T> {
    fn new() -> Self {
        Self(Arc::new(SlotState {
            value: Mutex::new(None),
            waker: Mutex::new(None),
            ready: Condvar::new(),
        }))
    }

    fn fill(&self, value: T) {
        if let Ok(mut slot) = self.0.value.lock() {
            *slot = Some(value);
        }
        self.0.ready.notify_all();
        let waker = self.0.waker.lock().ok().and_then(|mut waker| waker.take());
        if let Some(waker) = waker {
            waker.wake();
        }
    }
}

impl<T> Future for Slot<T> {
    type Output = T;

    fn poll(self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<T> {
        let Ok(mut value) = self.0.value.lock() else {
            // A poisoned slot never resolves; waking forever would spin, so the
            // future parks and the caller's timeout decides.
            return Poll::Pending;
        };
        match value.take() {
            Some(value) => Poll::Ready(value),
            None => {
                if let Ok(mut waker) = self.0.waker.lock() {
                    *waker = Some(cx.waker().clone());
                }
                Poll::Pending
            }
        }
    }
}

/// Drives one future to completion on the calling thread.
///
/// `liyasa build` has no async runtime and does not want one; the pool's
/// futures exist for the server, and a static build still needs to wait for
/// them.
pub fn block_on<F: Future>(future: F) -> F::Output {
    let mut future = Box::pin(future);
    let parker = Arc::new(Parker::default());
    let waker = Waker::from(Arc::clone(&parker));
    let mut context = Context::from_waker(&waker);
    loop {
        if let Poll::Ready(value) = future.as_mut().poll(&mut context) {
            return value;
        }
        parker.park();
    }
}

#[derive(Default)]
struct Parker {
    woken: Mutex<bool>,
    signal: Condvar,
}

impl Parker {
    fn park(&self) {
        let Ok(mut woken) = self.woken.lock() else {
            return;
        };
        while !*woken {
            match self.signal.wait_timeout(woken, Duration::from_millis(10)) {
                Ok((guard, _)) => woken = guard,
                Err(_) => return,
            }
        }
        *woken = false;
    }
}

impl std::task::Wake for Parker {
    fn wake(self: Arc<Self>) {
        self.wake_by_ref();
    }

    fn wake_by_ref(self: &Arc<Self>) {
        if let Ok(mut woken) = self.woken.lock() {
            *woken = true;
        }
        self.signal.notify_all();
    }
}

#[cfg(test)]
mod tests {
    use std::sync::Arc;

    use liyasa_core::build::{OutputFormat, RenderBudget, RenderMode, Variant};
    use liyasa_core::document::SourceDocument;
    use liyasa_core::ids::PageId;
    use liyasa_core::span::SourceId;

    use super::*;

    fn job(budget: RenderBudget) -> RenderJob {
        RenderJob {
            page: PageId(ulid::Ulid::nil()),
            variant: Variant::default(),
            format: OutputFormat::Html,
            mode: RenderMode::Anonymous {
                source: Arc::new(SourceDocument {
                    source: SourceId(0),
                    frontmatter: None,
                    segments: Vec::new(),
                }),
            },
            budget,
        }
    }

    fn budget() -> RenderBudget {
        RenderBudget {
            cpu: Duration::from_secs(5),
            max_bytes: 1 << 20,
        }
    }

    fn renderer(body: &'static str) -> RenderFn {
        Arc::new(move |_job| {
            Ok(Rendered {
                body: body.to_owned(),
                cpu: Duration::ZERO,
            })
        })
    }

    #[test]
    fn a_submitted_job_comes_back_rendered() {
        let pool = RenderPool::with_renderer(2, 8, Duration::from_secs(1), renderer("<p>hi</p>"));
        let rendered = block_on(pool.submit(job(budget()))).expect("a rendering");
        assert_eq!(rendered.body, "<p>hi</p>");
        assert_eq!(pool.metrics().depth, 0);
    }

    #[test]
    fn a_pool_with_no_renderer_reports_rather_than_panics() {
        let pool = RenderPool::new(1, 4, Duration::from_millis(10));
        let error = block_on(pool.submit(job(budget()))).expect_err("no renderer");
        assert!(matches!(error, RenderError::Component(_)));
    }

    #[test]
    fn output_past_the_budget_is_refused() {
        let pool = RenderPool::with_renderer(1, 4, Duration::from_secs(1), renderer("0123456789"));
        let error = block_on(pool.submit(job(RenderBudget {
            cpu: Duration::from_secs(5),
            max_bytes: 4,
        })))
        .expect_err("over the output budget");
        assert_eq!(error, RenderError::Budget);
    }

    #[test]
    fn cpu_past_the_budget_is_refused() {
        let slow: RenderFn = Arc::new(|_job| {
            std::thread::sleep(Duration::from_millis(20));
            Ok(Rendered {
                body: "slow".to_owned(),
                cpu: Duration::ZERO,
            })
        });
        let pool = RenderPool::with_renderer(1, 4, Duration::from_secs(1), slow);
        let error = block_on(pool.submit(job(RenderBudget {
            cpu: Duration::from_millis(1),
            max_bytes: 1 << 20,
        })))
        .expect_err("over the cpu budget");
        assert_eq!(error, RenderError::Budget);
    }

    #[test]
    fn a_full_queue_is_refused_and_counted() {
        let blocker = Arc::new((Mutex::new(false), Condvar::new()));
        let held = Arc::clone(&blocker);
        let renderer: RenderFn = Arc::new(move |_job| {
            let (lock, signal) = &*held;
            if let Ok(mut released) = lock.lock() {
                while !*released {
                    match signal.wait_timeout(released, Duration::from_secs(5)) {
                        Ok((guard, _)) => released = guard,
                        Err(_) => break,
                    }
                }
            }
            Ok(Rendered {
                body: "held".to_owned(),
                cpu: Duration::ZERO,
            })
        });

        let pool = RenderPool::with_renderer(1, 1, Duration::from_secs(5), renderer);
        let first = pool.submit(job(budget()));
        // The queue holds one job, and the first is in it until the renderer is
        // released, so the second has nowhere to go.
        let second = block_on(pool.submit(job(budget())));
        assert_eq!(second, Err(RenderError::Budget));
        assert_eq!(pool.metrics().rejected, 1);

        let (lock, signal) = &*blocker;
        if let Ok(mut released) = lock.lock() {
            *released = true;
        }
        signal.notify_all();
        assert!(block_on(first).is_ok());
    }

    #[test]
    fn a_wait_past_the_timeout_is_refused() {
        let pool =
            RenderPool::with_renderer(1, 64, Duration::from_nanos(1), renderer("<p>late</p>"));
        let error = block_on(pool.submit(job(budget()))).expect_err("waited too long");
        assert_eq!(error, RenderError::Budget);
    }

    #[test]
    fn metrics_report_a_wait_percentile() {
        let pool = RenderPool::with_renderer(2, 16, Duration::from_secs(1), renderer("<p>hi</p>"));
        for _ in 0..8 {
            block_on(pool.submit(job(budget()))).expect("a rendering");
        }
        let metrics = pool.metrics();
        assert_eq!(metrics.depth, 0);
        assert_eq!(metrics.rejected, 0);
        assert!(metrics.wait_p95 < Duration::from_secs(1));
    }
}
