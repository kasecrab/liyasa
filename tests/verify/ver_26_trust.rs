//! VER-26: a value from a `url` source is `external`, is escaped where it
//! enters the context, and creates nothing when it is interpolated; a
//! plain-HTTP source is `E0806`.
//!
//! The value is not escaped by this test. It is fetched through a `url` source,
//! carried out of the refresher with the trust of that source, and handed to
//! the same `escape_untrusted` call `liyasa-markdown` documents as "the call a
//! build makes once, where the value enters the context" — which is the step a
//! build owes and does not yet make (`liyasa-verify/src/sources/refresh.rs`
//! notes the seam). What is asserted is the outcome for a reader: the page has
//! one paragraph of text and no component, no fence, no directive.

use std::sync::Mutex;
use std::time::SystemTime;

use liyasa_components::Registry;
use liyasa_core::ai::TrustLevel;
use liyasa_core::conformance::block_on;
use liyasa_core::document::{Block, BlockKind, Node};
use liyasa_core::ids::FactId;
use liyasa_core::markdown::{Expanded, ExpansionRecord, ParseOptions, SpanMap};
use liyasa_core::net::{BoxFut, HttpClient, HttpPolicy, HttpRequest, HttpResponse, NetError};
use liyasa_verify::sources::kinds::{BuildTrust, DeclaredSource};
use liyasa_verify::sources::refresh::{Facts, RefreshReport, Refresher};
use liyasa_verify::sources::snapshot::SnapshotLog;
use liyasa_verify::sources::spec::SourceSpec;
use serde_json::json;

/// What a vendor's API may put in a JSON string: a fence opener, a directive
/// opener, and the parser's own marker syntax.
///
/// The marker's nonce is hex with letters in it rather than the corpus's
/// sixteen zeros, because sixteen zeros are a Luhn-valid card number and the
/// scrubber redacts them on the way into the snapshot (§30.2.4's rules are
/// deliberately eager). That is the scrubber working; it would just make this
/// test assert about `[redacted]` instead of about escaping.
const HOSTILE: &str = "```js :::note <!--ly:9f8e7d6c5b4a3210:o:0-->";

struct Api {
    body: &'static str,
    seen: Mutex<usize>,
}

impl Api {
    fn new(body: &'static str) -> Self {
        Self {
            body,
            seen: Mutex::new(0),
        }
    }

    fn requests(&self) -> usize {
        *self.seen.lock().expect("not poisoned")
    }
}

impl HttpClient for Api {
    fn fetch<'a>(
        &'a self,
        req: HttpRequest,
        _policy: &'a HttpPolicy,
    ) -> BoxFut<'a, Result<HttpResponse, NetError>> {
        *self.seen.lock().expect("not poisoned") += 1;
        Box::pin(std::future::ready(Ok(HttpResponse {
            status: 200,
            headers: Vec::new(),
            body: self.body.as_bytes().to_vec().into(),
            final_url: req.url,
        })))
    }
}

fn refresh(url: &str, api: &Api) -> RefreshReport {
    let (spec, problems) = SourceSpec::parse(
        "pricing",
        &json!({
            "kind": "url",
            "url": url,
            "facts": { "plan.pro.label": "/label" }
        }),
    );
    assert!(problems.is_empty(), "{problems:#?}");
    let log = SnapshotLog::new();
    block_on(Refresher::new(&log, BuildTrust::Trusted, "main").refresh(
        &[DeclaredSource::new(spec)],
        api,
        None,
        SystemTime::UNIX_EPOCH,
    ))
}

/// The `facts.*` layer as a build must assemble it: every value that entered
/// below `operator` escaped on the way in (CM-20).
fn escaped_context(facts: &Facts) -> serde_json::Value {
    assert_eq!(
        facts.untrusted(),
        [&FactId::new("plan.pro.label")],
        "a `url` source's value is untrusted and must be named as such"
    );
    liyasa_markdown::escape_untrusted(&facts.as_context())
        .unwrap_or_else(|problems| panic!("{problems:#?}"))
}

fn components_in(block: &Block, out: &mut Vec<String>) {
    if let BlockKind::Component { name, .. } = &block.kind {
        out.push(name.clone());
    }
    for node in &block.children {
        if let Node::Block(child) = node {
            components_in(child, out);
        }
    }
}

fn code_blocks_in(block: &Block, out: &mut Vec<String>) {
    if let BlockKind::CodeBlock { lang, .. } = &block.kind {
        out.push(lang.clone().unwrap_or_default());
    }
    for node in &block.children {
        if let Node::Block(child) = node {
            code_blocks_in(child, out);
        }
    }
}

#[test]
fn a_url_fact_is_external_and_is_named_as_needing_escaping() {
    let api = Api::new(r#"{"label": "safe"}"#);
    let report = refresh("https://api.example.com/plans", &api);
    let fact = report
        .facts
        .get(&FactId::new("plan.pro.label"))
        .expect("the fact");
    assert_eq!(fact.trust, TrustLevel::External);
    assert_eq!(report.facts.untrusted(), [&FactId::new("plan.pro.label")]);
}

#[test]
fn a_hostile_value_is_escaped_and_creates_nothing() {
    let api = Api::new("{\"label\": \"```js :::note <!--ly:9f8e7d6c5b4a3210:o:0-->\"}");
    let report = refresh("https://api.example.com/plans", &api);
    assert!(report.diagnostics.is_empty(), "{:#?}", report.diagnostics);

    let context = escaped_context(&report.facts);
    let value = context
        .pointer("/plan/pro/label")
        .and_then(serde_json::Value::as_str)
        .expect("the escaped value");
    assert_ne!(value, HOSTILE, "the value entered untouched");
    assert!(value.contains("\\`\\`\\`"), "the fence is escaped: {value}");
    assert!(value.contains("\\<\\!--"), "the marker is escaped: {value}");
    // `:::note` is not escaped here and does not need to be: it is not at the
    // start of the line and cannot be, because a value carrying a line break is
    // refused outright with `E0320` rather than escaped. What lands at the
    // start of a line is the first character of the value, and that is what the
    // block-opener rule covers — see the test below.

    // And what a reader gets: one paragraph, no component, no fence.
    let page = format!("Plan: {value}\n");
    let expanded = Expanded {
        text: page,
        map: SpanMap::default(),
        record: ExpansionRecord::default(),
    };
    let document =
        liyasa_markdown::parse(&expanded, &Registry::builtins(), &ParseOptions::default());
    assert!(
        !document.diagnostics.has_errors(),
        "{:#?}",
        document.diagnostics
    );

    let mut components = Vec::new();
    components_in(&document.root, &mut components);
    assert!(components.is_empty(), "a value created {components:?}");

    let mut fenced = Vec::new();
    code_blocks_in(&document.root, &mut fenced);
    assert!(fenced.is_empty(), "a value opened a code block: {fenced:?}");
}

#[test]
fn a_value_that_starts_with_a_directive_cannot_open_one() {
    let api = Api::new("{\"label\": \":::card{title=\\\"Free\\\"}\"}");
    let report = refresh("https://api.example.com/plans", &api);
    let context = escaped_context(&report.facts);
    let value = context
        .pointer("/plan/pro/label")
        .and_then(serde_json::Value::as_str)
        .expect("the escaped value");
    assert!(value.starts_with("\\:::"), "{value}");

    let expanded = Expanded {
        text: format!("{value}\n"),
        map: SpanMap::default(),
        record: ExpansionRecord::default(),
    };
    let document =
        liyasa_markdown::parse(&expanded, &Registry::builtins(), &ParseOptions::default());
    let mut components = Vec::new();
    components_in(&document.root, &mut components);
    assert!(components.is_empty(), "a value created {components:?}");
}

#[test]
fn a_plain_http_source_is_e0806_and_is_never_fetched() {
    let api = Api::new(r#"{"label": "safe"}"#);
    let report = refresh("http://api.example.com/plans", &api);
    let codes: Vec<&str> = report.diagnostics.iter().map(|d| d.code.as_str()).collect();
    assert_eq!(codes, ["E0806"]);
    assert_eq!(api.requests(), 0, "a refused transport makes no request");
    assert!(report.facts.is_empty());
}
