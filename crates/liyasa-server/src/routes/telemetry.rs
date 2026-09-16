//! Structured logs and traces (HOST-05, RFC 1401).
//!
//! Logs are `tracing` rendered as JSON. Traces are recorded here and exported
//! as OTLP over HTTP with the JSON encoding, posted through `liyasa-net` so
//! the collector endpoint is subject to the same outbound policy as every
//! other request. There is no OpenTelemetry SDK in the tree: the span shape
//! below is the wire format, and a package that later needs sampling or
//! resource detection replaces this module without changing it.

use std::sync::Mutex;
use std::time::{Duration, SystemTime, UNIX_EPOCH};

use serde_json::{Value, json};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct TraceId(pub [u8; 16]);

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct SpanId(pub [u8; 8]);

fn hex(bytes: &[u8]) -> String {
    bytes.iter().fold(String::new(), |mut out, b| {
        use std::fmt::Write as _;
        let _ = write!(out, "{b:02x}");
        out
    })
}

fn from_hex<const N: usize>(text: &str) -> Option<[u8; N]> {
    if text.len() != N * 2 {
        return None;
    }
    let mut out = [0u8; N];
    for (i, byte) in out.iter_mut().enumerate() {
        *byte = u8::from_str_radix(text.get(i * 2..i * 2 + 2)?, 16).ok()?;
    }
    Some(out)
}

impl TraceId {
    pub fn generate() -> Self {
        let mut bytes = [0u8; 16];
        if getrandom::fill(&mut bytes).is_err() {
            bytes = blake3::hash(&now_nanos().to_le_bytes()).as_bytes()[..16]
                .try_into()
                .unwrap_or([1; 16]);
        }
        Self(bytes)
    }

    pub fn to_hex(self) -> String {
        hex(&self.0)
    }
}

impl SpanId {
    pub fn generate() -> Self {
        let mut bytes = [0u8; 8];
        if getrandom::fill(&mut bytes).is_err() {
            bytes = blake3::hash(&now_nanos().to_le_bytes()).as_bytes()[..8]
                .try_into()
                .unwrap_or([1; 8]);
        }
        Self(bytes)
    }

    pub fn to_hex(self) -> String {
        hex(&self.0)
    }
}

/// The incoming `traceparent`, so a trace that started at the edge continues
/// here rather than beginning again (W3C Trace Context).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Parent {
    pub trace: TraceId,
    pub span: SpanId,
    pub sampled: bool,
}

pub fn parse_traceparent(value: &str) -> Option<Parent> {
    let mut parts = value.trim().split('-');
    let version = parts.next()?;
    if version != "00" {
        return None;
    }
    let trace = TraceId(from_hex::<16>(parts.next()?)?);
    let span = SpanId(from_hex::<8>(parts.next()?)?);
    let flags = u8::from_str_radix(parts.next()?, 16).ok()?;
    if trace.0 == [0; 16] || span.0 == [0; 8] {
        return None;
    }
    Some(Parent {
        trace,
        span,
        sampled: flags & 1 == 1,
    })
}

pub fn traceparent(trace: TraceId, span: SpanId, sampled: bool) -> String {
    format!(
        "00-{}-{}-{}",
        trace.to_hex(),
        span.to_hex(),
        if sampled { "01" } else { "00" }
    )
}

fn now_nanos() -> u128 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_nanos())
        .unwrap_or(0)
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SpanKind {
    Server = 2,
    Client = 3,
    Internal = 1,
}

#[derive(Debug, Clone)]
pub struct Span {
    pub trace: TraceId,
    pub id: SpanId,
    pub parent: Option<SpanId>,
    pub name: String,
    pub kind: SpanKind,
    pub start_nanos: u128,
    pub duration: Duration,
    pub attributes: Vec<(String, Value)>,
    /// OTLP status: 0 unset, 1 ok, 2 error.
    pub status: u8,
}

impl Span {
    pub fn to_otlp(&self) -> Value {
        let end = self.start_nanos + self.duration.as_nanos();
        json!({
            "traceId": self.trace.to_hex(),
            "spanId": self.id.to_hex(),
            "parentSpanId": self.parent.map(|p| p.to_hex()).unwrap_or_default(),
            "name": self.name,
            "kind": self.kind as u8,
            "startTimeUnixNano": self.start_nanos.to_string(),
            "endTimeUnixNano": end.to_string(),
            "attributes": self.attributes.iter().map(|(k, v)| json!({
                "key": k,
                "value": attribute_value(v),
            })).collect::<Vec<_>>(),
            "status": { "code": self.status },
        })
    }
}

fn attribute_value(value: &Value) -> Value {
    match value {
        Value::String(s) => json!({ "stringValue": s }),
        Value::Bool(b) => json!({ "boolValue": b }),
        Value::Number(n) if n.is_i64() || n.is_u64() => {
            json!({ "intValue": n.as_i64().unwrap_or_default().to_string() })
        }
        Value::Number(n) => json!({ "doubleValue": n.as_f64().unwrap_or_default() }),
        other => json!({ "stringValue": other.to_string() }),
    }
}

/// A span in progress. Dropping it without `end` records nothing, which is
/// what should happen to a request that panicked before its handler ran.
#[derive(Debug)]
pub struct Recording {
    span: Span,
    started: std::time::Instant,
}

impl Recording {
    pub fn attribute(&mut self, key: &str, value: Value) {
        self.span.attributes.push((key.to_owned(), value));
    }

    pub fn trace(&self) -> TraceId {
        self.span.trace
    }

    pub fn id(&self) -> SpanId {
        self.span.id
    }
}

/// Finished spans waiting for the exporter. Bounded and lossy on purpose: a
/// collector that is down must never grow the server's memory.
#[derive(Debug)]
pub struct Tracer {
    spans: Mutex<Vec<Span>>,
    capacity: usize,
    service: String,
    /// Off when no collector is configured, which is the default; spans are
    /// then not even recorded.
    enabled: bool,
}

impl Tracer {
    pub fn new(service: impl Into<String>, enabled: bool) -> Self {
        Self {
            spans: Mutex::new(Vec::new()),
            capacity: 2048,
            service: service.into(),
            enabled,
        }
    }

    pub fn disabled() -> Self {
        Self::new("liyasa", false)
    }

    pub fn is_enabled(&self) -> bool {
        self.enabled
    }

    pub fn start(&self, name: impl Into<String>, parent: Option<Parent>) -> Recording {
        Recording {
            span: Span {
                trace: parent.map(|p| p.trace).unwrap_or_else(TraceId::generate),
                id: SpanId::generate(),
                parent: parent.map(|p| p.span),
                name: name.into(),
                kind: SpanKind::Server,
                start_nanos: now_nanos(),
                duration: Duration::ZERO,
                attributes: Vec::new(),
                status: 0,
            },
            started: std::time::Instant::now(),
        }
    }

    pub fn end(&self, mut recording: Recording, status: u8) {
        if !self.enabled {
            return;
        }
        recording.span.duration = recording.started.elapsed();
        recording.span.status = status;
        let mut spans = self.spans.lock().unwrap_or_else(|e| e.into_inner());
        if spans.len() >= self.capacity {
            spans.remove(0);
        }
        spans.push(recording.span);
    }

    pub fn take(&self) -> Vec<Span> {
        std::mem::take(&mut *self.spans.lock().unwrap_or_else(|e| e.into_inner()))
    }

    pub fn depth(&self) -> usize {
        self.spans.lock().unwrap_or_else(|e| e.into_inner()).len()
    }

    /// One OTLP `ExportTraceServiceRequest`.
    pub fn payload(&self, spans: &[Span]) -> Value {
        json!({
            "resourceSpans": [{
                "resource": {
                    "attributes": [
                        { "key": "service.name", "value": { "stringValue": self.service } },
                        { "key": "service.version", "value": { "stringValue": env!("CARGO_PKG_VERSION") } },
                    ]
                },
                "scopeSpans": [{
                    "scope": { "name": "liyasa-server" },
                    "spans": spans.iter().map(Span::to_otlp).collect::<Vec<_>>(),
                }]
            }]
        })
    }
}

/// JSON lines on standard output, filtered by `RUST_LOG` (HOST-05).
/// Returns false when a subscriber was already installed, which is what
/// happens when two tests initialise in one process.
pub fn init_logging(json: bool) -> bool {
    use tracing_subscriber::layer::SubscriberExt as _;
    use tracing_subscriber::util::SubscriberInitExt as _;

    let filter = tracing_subscriber::EnvFilter::try_from_default_env()
        .unwrap_or_else(|_| tracing_subscriber::EnvFilter::new("info"));
    let registry = tracing_subscriber::registry().with(filter);
    if json {
        registry
            .with(tracing_subscriber::fmt::layer().json().flatten_event(true))
            .try_init()
            .is_ok()
    } else {
        registry
            .with(tracing_subscriber::fmt::layer())
            .try_init()
            .is_ok()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_traceparent_round_trips_and_a_malformed_one_is_refused() {
        let trace = TraceId([7; 16]);
        let span = SpanId([9; 8]);
        let header = traceparent(trace, span, true);
        let parsed = parse_traceparent(&header).expect("a parse");
        assert_eq!(parsed.trace, trace);
        assert_eq!(parsed.span, span);
        assert!(parsed.sampled);

        assert!(parse_traceparent("01-<future version>").is_none());
        assert!(parse_traceparent(&traceparent(TraceId([0; 16]), span, true)).is_none());
        assert!(parse_traceparent(&traceparent(trace, SpanId([0; 8]), true)).is_none());
        assert!(parse_traceparent("garbage").is_none());
    }

    #[test]
    fn a_span_exports_in_the_otlp_json_shape() {
        let tracer = Tracer::new("liyasa", true);
        let parent = parse_traceparent(&traceparent(TraceId([1; 16]), SpanId([2; 8]), true))
            .expect("a parent");
        let mut recording = tracer.start("GET /docs", Some(parent));
        recording.attribute("http.request.method", json!("GET"));
        recording.attribute("http.response.status_code", json!(200));
        tracer.end(recording, 1);

        let spans = tracer.take();
        assert_eq!(spans.len(), 1);
        assert_eq!(tracer.depth(), 0, "taking the batch empties the buffer");
        let payload = tracer.payload(&spans);
        let span = &payload["resourceSpans"][0]["scopeSpans"][0]["spans"][0];
        assert_eq!(span["traceId"], TraceId([1; 16]).to_hex());
        assert_eq!(span["parentSpanId"], SpanId([2; 8]).to_hex());
        assert_eq!(span["name"], "GET /docs");
        assert_eq!(span["kind"], 2);
        assert_eq!(span["status"]["code"], 1);
        assert_eq!(
            span["attributes"][1]["value"]["intValue"], "200",
            "OTLP writes integers as strings"
        );
        assert_eq!(
            payload["resourceSpans"][0]["resource"]["attributes"][0]["value"]["stringValue"],
            "liyasa"
        );
    }

    #[test]
    fn a_disabled_tracer_records_nothing() {
        let tracer = Tracer::disabled();
        let recording = tracer.start("GET /docs", None);
        tracer.end(recording, 1);
        assert!(tracer.take().is_empty());
    }

    #[test]
    fn the_buffer_is_bounded_so_a_dead_collector_cannot_grow_the_server() {
        let tracer = Tracer::new("liyasa", true);
        for _ in 0..(tracer.capacity + 10) {
            let recording = tracer.start("GET /docs", None);
            tracer.end(recording, 1);
        }
        assert_eq!(tracer.depth(), tracer.capacity);
    }
}
