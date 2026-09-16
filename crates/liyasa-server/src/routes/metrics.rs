//! Prometheus text exposition (HOST-05, RFC 1401).
//!
//! A handful of counters and gauges written by hand: the alternative was a
//! client library for four metric families, and the text format is a dozen
//! lines of code.

use std::collections::BTreeMap;
use std::sync::Mutex;
use std::sync::atomic::{AtomicU64, Ordering};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Kind {
    Counter,
    Gauge,
    Histogram,
}

/// Request durations in seconds; the buckets a documentation server's
/// latency actually falls in (NFR-02's budget is 200 ms).
pub const BUCKETS: &[f64] = &[0.005, 0.01, 0.025, 0.05, 0.1, 0.2, 0.5, 1.0, 2.5, 10.0];

#[derive(Debug, Default)]
struct Histogram {
    counts: Vec<AtomicU64>,
    sum_micros: AtomicU64,
    total: AtomicU64,
}

impl Histogram {
    fn new() -> Self {
        let mut counts = Vec::with_capacity(BUCKETS.len());
        counts.resize_with(BUCKETS.len(), AtomicU64::default);
        Self {
            counts,
            sum_micros: AtomicU64::new(0),
            total: AtomicU64::new(0),
        }
    }

    fn observe(&self, seconds: f64) {
        for (bucket, count) in BUCKETS.iter().zip(&self.counts) {
            if seconds <= *bucket {
                count.fetch_add(1, Ordering::Relaxed);
            }
        }
        self.sum_micros
            .fetch_add((seconds * 1e6) as u64, Ordering::Relaxed);
        self.total.fetch_add(1, Ordering::Relaxed);
    }
}

type Labels = Vec<(String, String)>;

#[derive(Debug, Default)]
pub struct Metrics {
    help: Mutex<BTreeMap<String, (Kind, String)>>,
    counters: Mutex<BTreeMap<(String, Labels), AtomicU64>>,
    gauges: Mutex<BTreeMap<(String, Labels), AtomicU64>>,
    histograms: Mutex<BTreeMap<(String, Labels), Histogram>>,
}

impl Metrics {
    pub fn new() -> Self {
        let this = Self::default();
        this.describe(
            "liyasa_http_requests_total",
            Kind::Counter,
            "HTTP requests by route class, method, and status.",
        );
        this.describe(
            "liyasa_http_request_duration_seconds",
            Kind::Histogram,
            "HTTP request duration by route class.",
        );
        this.describe(
            "liyasa_rate_limited_total",
            Kind::Counter,
            "Requests refused with 429 by pool.",
        );
        this.describe(
            "liyasa_ingest_queue_depth",
            Kind::Gauge,
            "Events buffered in the analytics ingest queue.",
        );
        this.describe(
            "liyasa_ingest_events_total",
            Kind::Counter,
            "Analytics events by disposition: received, written, spilled, dropped.",
        );
        this.describe(
            "liyasa_jobs_queue_depth",
            Kind::Gauge,
            "Jobs queued or leased in the job store.",
        );
        this.describe(
            "liyasa_build_info",
            Kind::Gauge,
            "Always 1, labelled with the running version.",
        );
        this
    }

    pub fn describe(&self, name: &str, kind: Kind, help: &str) {
        self.help
            .lock()
            .unwrap_or_else(|e| e.into_inner())
            .insert(name.to_owned(), (kind, help.to_owned()));
    }

    fn key(name: &str, labels: &[(&str, &str)]) -> (String, Labels) {
        let mut labels: Labels = labels
            .iter()
            .map(|(k, v)| ((*k).to_owned(), (*v).to_owned()))
            .collect();
        labels.sort();
        (name.to_owned(), labels)
    }

    pub fn increment(&self, name: &str, labels: &[(&str, &str)], by: u64) {
        let mut counters = self.counters.lock().unwrap_or_else(|e| e.into_inner());
        counters
            .entry(Self::key(name, labels))
            .or_default()
            .fetch_add(by, Ordering::Relaxed);
    }

    pub fn set(&self, name: &str, labels: &[(&str, &str)], value: u64) {
        let mut gauges = self.gauges.lock().unwrap_or_else(|e| e.into_inner());
        gauges
            .entry(Self::key(name, labels))
            .or_default()
            .store(value, Ordering::Relaxed);
    }

    pub fn observe(&self, name: &str, labels: &[(&str, &str)], seconds: f64) {
        let mut histograms = self.histograms.lock().unwrap_or_else(|e| e.into_inner());
        histograms
            .entry(Self::key(name, labels))
            .or_insert_with(Histogram::new)
            .observe(seconds);
    }

    /// The exposition body. Sorted, so two scrapes of an unchanged server are
    /// byte-identical.
    pub fn render(&self) -> String {
        let help = self.help.lock().unwrap_or_else(|e| e.into_inner());
        let mut out = String::new();
        let mut written: Vec<String> = Vec::new();

        for source in [&self.counters, &self.gauges] {
            let values = source.lock().unwrap_or_else(|e| e.into_inner());
            for ((name, labels), value) in values.iter() {
                write_header(&mut out, &mut written, &help, name);
                out.push_str(&format!(
                    "{name}{} {}\n",
                    render_labels(labels, None),
                    value.load(Ordering::Relaxed)
                ));
            }
        }

        let histograms = self.histograms.lock().unwrap_or_else(|e| e.into_inner());
        for ((name, labels), histogram) in histograms.iter() {
            write_header(&mut out, &mut written, &help, name);
            for (bucket, count) in BUCKETS.iter().zip(&histogram.counts) {
                out.push_str(&format!(
                    "{name}_bucket{} {}\n",
                    render_labels(labels, Some(&format!("{bucket}"))),
                    count.load(Ordering::Relaxed)
                ));
            }
            let total = histogram.total.load(Ordering::Relaxed);
            out.push_str(&format!(
                "{name}_bucket{} {total}\n",
                render_labels(labels, Some("+Inf"))
            ));
            out.push_str(&format!(
                "{name}_sum{} {:.6}\n",
                render_labels(labels, None),
                histogram.sum_micros.load(Ordering::Relaxed) as f64 / 1e6
            ));
            out.push_str(&format!(
                "{name}_count{} {total}\n",
                render_labels(labels, None)
            ));
        }
        out
    }
}

fn write_header(
    out: &mut String,
    written: &mut Vec<String>,
    help: &BTreeMap<String, (Kind, String)>,
    name: &str,
) {
    if written.iter().any(|w| w == name) {
        return;
    }
    if let Some((kind, text)) = help.get(name) {
        out.push_str(&format!("# HELP {name} {text}\n"));
        out.push_str(&format!(
            "# TYPE {name} {}\n",
            match kind {
                Kind::Counter => "counter",
                Kind::Gauge => "gauge",
                Kind::Histogram => "histogram",
            }
        ));
    }
    written.push(name.to_owned());
}

fn render_labels(labels: &Labels, le: Option<&str>) -> String {
    if labels.is_empty() && le.is_none() {
        return String::new();
    }
    let mut parts: Vec<String> = labels
        .iter()
        .map(|(k, v)| format!("{k}=\"{}\"", escape(v)))
        .collect();
    if let Some(le) = le {
        parts.push(format!("le=\"{le}\""));
    }
    format!("{{{}}}", parts.join(","))
}

fn escape(value: &str) -> String {
    value
        .replace('\\', "\\\\")
        .replace('"', "\\\"")
        .replace('\n', "\\n")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn counters_gauges_and_histograms_render_in_the_text_format() {
        let metrics = Metrics::new();
        metrics.increment(
            "liyasa_http_requests_total",
            &[("status", "200"), ("class", "page")],
            2,
        );
        metrics.set("liyasa_ingest_queue_depth", &[], 7);
        metrics.observe(
            "liyasa_http_request_duration_seconds",
            &[("class", "page")],
            0.03,
        );

        let body = metrics.render();
        assert!(body.contains("# TYPE liyasa_http_requests_total counter\n"));
        assert!(
            body.contains("liyasa_http_requests_total{class=\"page\",status=\"200\"} 2\n"),
            "{body}"
        );
        assert!(body.contains("liyasa_ingest_queue_depth 7\n"), "{body}");
        assert!(
            body.contains(
                "liyasa_http_request_duration_seconds_bucket{class=\"page\",le=\"0.05\"} 1\n"
            ),
            "{body}"
        );
        assert!(
            body.contains(
                "liyasa_http_request_duration_seconds_bucket{class=\"page\",le=\"0.025\"} 0\n"
            ),
            "{body}"
        );
        assert!(
            body.contains("liyasa_http_request_duration_seconds_count{class=\"page\"} 1\n"),
            "{body}"
        );
        assert!(
            body.contains("liyasa_http_request_duration_seconds_sum{class=\"page\"} 0.030"),
            "{body}"
        );
        assert_eq!(body, metrics.render(), "a scrape is deterministic");
    }

    #[test]
    fn a_label_value_with_a_quote_is_escaped() {
        let metrics = Metrics::new();
        metrics.increment("liyasa_http_requests_total", &[("class", "a\"b")], 1);
        assert!(metrics.render().contains(r#"class="a\"b""#));
    }
}
