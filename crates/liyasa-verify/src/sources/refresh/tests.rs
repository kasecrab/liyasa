use std::sync::{Arc, Mutex};
use std::time::Duration;

use liyasa_core::conformance::block_on;
use liyasa_core::conformance::fixtures::MemoryVfs;
use liyasa_core::net::{BoxFut, HttpPolicy, HttpRequest, HttpResponse, NetError};
use liyasa_core::verify::ChangeKind;
use serde_json::json;

use super::super::spec::SourceSpec;
use super::*;

struct Canned {
    body: Mutex<Vec<u8>>,
    seen: Mutex<usize>,
}

impl Canned {
    fn new(value: serde_json::Value) -> Self {
        Self {
            body: Mutex::new(value.to_string().into_bytes()),
            seen: Mutex::new(0),
        }
    }

    fn answer(&self, value: serde_json::Value) {
        *self.body.lock().expect("not poisoned") = value.to_string().into_bytes();
    }

    fn requests(&self) -> usize {
        *self.seen.lock().expect("not poisoned")
    }
}

impl HttpClient for Canned {
    fn fetch<'a>(
        &'a self,
        req: HttpRequest,
        _policy: &'a HttpPolicy,
    ) -> BoxFut<'a, Result<HttpResponse, NetError>> {
        *self.seen.lock().expect("not poisoned") += 1;
        let body = self.body.lock().expect("not poisoned").clone();
        Box::pin(std::future::ready(Ok(HttpResponse {
            status: 200,
            headers: Vec::new(),
            body: body.into(),
            final_url: req.url,
        })))
    }
}

fn at(seconds: u64) -> SystemTime {
    SystemTime::UNIX_EPOCH + Duration::from_secs(seconds)
}

fn source(id: &str, declaration: serde_json::Value) -> DeclaredSource {
    let (spec, problems) = SourceSpec::parse(id, &declaration);
    assert!(problems.is_empty(), "{problems:#?}");
    DeclaredSource::new(spec)
}

fn url_source(id: &str) -> DeclaredSource {
    source(
        id,
        json!({
            "kind": "url",
            "url": "https://api.example.com/plans",
            "facts": { "plan.pro.price": "/price" }
        }),
    )
}

#[test]
fn refreshing_twice_with_a_changed_value_reports_the_change() {
    let log = SnapshotLog::new();
    let http = Canned::new(json!({ "price": 20 }));
    let refresher = Refresher::new(&log, BuildTrust::Trusted, "main");
    let sources = [url_source("pricing").taken_at(at(0))];

    let first = block_on(refresher.refresh(&sources, &http, None, at(0)));
    assert_eq!(first.refreshed, ["pricing"]);
    assert!(first.changes.is_empty(), "nothing to compare against yet");
    assert!(first.diagnostics.is_empty(), "{:#?}", first.diagnostics);

    http.answer(json!({ "price": 25 }));
    let sources = [url_source("pricing").taken_at(at(3600))];
    let second = block_on(refresher.refresh(&sources, &http, None, at(3600)));
    assert_eq!(second.changes.len(), 1);
    assert_eq!(second.changes[0].kind, ChangeKind::Changed);
    assert_eq!(
        second.changes[0].new,
        Some(FactValue::Num(25.0)),
        "{:#?}",
        second.changes
    );
    assert_eq!(log.history("pricing").expect("a history").len(), 2);
}

#[test]
fn an_untrusted_build_reads_the_last_production_snapshot_instead_of_refreshing() {
    let log = SnapshotLog::new();
    let http = Canned::new(json!({ "price": 20 }));
    let sources = [url_source("pricing").taken_at(at(0))];
    block_on(Refresher::new(&log, BuildTrust::Trusted, "main").refresh(
        &sources,
        &http,
        None,
        at(0),
    ));
    let after_trusted = http.requests();

    http.answer(json!({ "price": 999 }));
    let fork = Refresher::new(&log, BuildTrust::Untrusted, "fork/pr-7");
    let sources = [url_source("pricing")
        .taken_at(at(60))
        .with_build_trust(BuildTrust::Untrusted)];
    let report = block_on(fork.refresh(&sources, &http, None, at(60)));

    assert_eq!(report.reused, ["pricing"]);
    assert!(report.refreshed.is_empty());
    assert_eq!(http.requests(), after_trusted, "nothing left the machine");
    assert_eq!(
        report
            .facts
            .get(&FactId::new("plan.pro.price"))
            .map(|f| &f.value),
        Some(&FactValue::Num(20.0)),
        "the production value, not the fork's"
    );
    assert!(report.diagnostics.is_empty(), "{:#?}", report.diagnostics);
}

#[test]
fn an_untrusted_build_with_no_production_snapshot_says_so() {
    let log = SnapshotLog::new();
    let http = Canned::new(json!({ "price": 20 }));
    let sources = [url_source("pricing").with_build_trust(BuildTrust::Untrusted)];
    let report = block_on(
        Refresher::new(&log, BuildTrust::Untrusted, "fork/pr-7").refresh(
            &sources,
            &http,
            None,
            at(0),
        ),
    );
    assert!(report.facts.is_empty());
    assert_eq!(report.diagnostics.len(), 1);
    assert_eq!(report.diagnostics[0].code.as_str(), "E0604");
    assert_eq!(http.requests(), 0);
}

#[test]
fn a_source_that_cannot_be_refreshed_is_e0604_and_does_not_stop_the_others() {
    let log = SnapshotLog::new();
    let http = Canned::new(json!({ "price": 20 }));
    let vfs = Arc::new(MemoryVfs::new().with("facts/limits.json", r#"{"seats": 5}"#));
    let sources = [
        source(
            "broken",
            json!({ "kind": "file", "path": "facts/missing.json" }),
        )
        .with_vfs(vfs.clone()),
        source(
            "limits",
            json!({ "kind": "file", "path": "facts/limits.json" }),
        )
        .with_vfs(vfs),
    ];
    let report = block_on(Refresher::new(&log, BuildTrust::Trusted, "main").refresh(
        &sources,
        &http,
        None,
        at(0),
    ));

    assert_eq!(report.refreshed, ["limits"]);
    assert_eq!(report.diagnostics.len(), 1);
    assert_eq!(report.diagnostics[0].code.as_str(), "E0604");
    assert!(report.facts.get(&FactId::new("seats")).is_some());
}

#[test]
fn an_expired_attestation_is_a_warning_and_a_lapsed_one_is_an_error() {
    let log = SnapshotLog::new();
    let http = Canned::new(json!({}));
    let expiry = 1_767_225_600; // 2026-01-01T00:00:00Z
    let declaration = json!({
        "kind": "manual", "owner": "ops", "expires": "2026-01-01",
        "values": { "sla.uptime": 99.95 }
    });

    let report = block_on(Refresher::new(&log, BuildTrust::Trusted, "main").refresh(
        &[source("sla", declaration.clone())],
        &http,
        None,
        at(expiry + 60),
    ));
    assert_eq!(report.diagnostics.len(), 1);
    assert!(
        !report.has_errors(),
        "inside the grace period it is a warning"
    );
    // The value is still published while it is only a warning.
    assert!(report.facts.get(&FactId::new("sla.uptime")).is_some());

    let late = block_on(Refresher::new(&log, BuildTrust::Trusted, "main").refresh(
        &[source("sla", declaration)],
        &http,
        None,
        at(expiry + 60 * 24 * 60 * 60),
    ));
    assert!(late.has_errors(), "{:#?}", late.diagnostics);
    assert_eq!(late.diagnostics[0].code.as_str(), "E0606");
}

#[test]
fn a_fact_carries_the_trust_of_the_source_it_came_from() {
    let log = SnapshotLog::new();
    let http = Canned::new(json!({ "price": 20 }));
    let vfs = Arc::new(MemoryVfs::new().with("facts/limits.json", r#"{"seats": 5}"#));
    let sources = [
        url_source("pricing"),
        source(
            "limits",
            json!({ "kind": "file", "path": "facts/limits.json" }),
        )
        .with_vfs(vfs),
    ];
    let report = block_on(Refresher::new(&log, BuildTrust::Trusted, "main").refresh(
        &sources,
        &http,
        None,
        at(0),
    ));

    let fetched = report
        .facts
        .get(&FactId::new("plan.pro.price"))
        .expect("the fetched fact");
    assert_eq!(fetched.trust, TrustLevel::External);
    assert_eq!(fetched.source, "pricing");

    let local = report
        .facts
        .get(&FactId::new("seats"))
        .expect("the local fact");
    assert_eq!(local.trust, TrustLevel::Operator);

    assert_eq!(
        report.facts.untrusted(),
        [&FactId::new("plan.pro.price")],
        "only what entered below `operator` needs escaping"
    );
}

#[test]
fn the_context_nests_a_dotted_fact_the_way_the_fact_filter_walks_it() {
    let facts = Facts(BTreeMap::from([
        (
            FactId::new("plan.pro.price"),
            Fact {
                value: FactValue::Num(20.0),
                source: "pricing".to_owned(),
                trust: TrustLevel::External,
            },
        ),
        (
            FactId::new("seats"),
            Fact {
                value: FactValue::Num(5.0),
                source: "limits".to_owned(),
                trust: TrustLevel::Operator,
            },
        ),
    ]));
    let context = facts.as_context();
    assert_eq!(
        context.pointer("/plan/pro/price"),
        Some(&json!({ "type": "num", "value": 20.0 }))
    );
    assert!(context.pointer("/seats").is_some());
}

#[test]
fn a_source_is_due_when_its_last_snapshot_is_older_than_the_interval() {
    let log = SnapshotLog::new();
    let http = Canned::new(json!({ "price": 20 }));
    let refresher = Refresher::new(&log, BuildTrust::Trusted, "schedule");
    let sources = [url_source("pricing").taken_at(at(0))];

    assert_eq!(
        refresher
            .due(&sources, at(0), Duration::from_secs(3600))
            .len(),
        1,
        "a source with no snapshot at all is always due"
    );
    block_on(refresher.refresh(&sources, &http, None, at(0)));

    assert!(
        refresher
            .due(&sources, at(1800), Duration::from_secs(3600))
            .is_empty()
    );
    assert_eq!(
        refresher
            .due(&sources, at(3600), Duration::from_secs(3600))
            .len(),
        1
    );
}

#[test]
fn two_sources_claiming_one_fact_is_reported_rather_than_resolved_silently() {
    let log = SnapshotLog::new();
    let http = Canned::new(json!({}));
    let vfs = Arc::new(
        MemoryVfs::new()
            .with("a.json", r#"{"seats": 5}"#)
            .with("b.json", r#"{"seats": 9}"#),
    );
    let sources = [
        source("a", json!({ "kind": "file", "path": "a.json" })).with_vfs(vfs.clone()),
        source("b", json!({ "kind": "file", "path": "b.json" })).with_vfs(vfs),
    ];
    let report = block_on(Refresher::new(&log, BuildTrust::Trusted, "main").refresh(
        &sources,
        &http,
        None,
        at(0),
    ));

    assert_eq!(
        report.facts.get(&FactId::new("seats")).map(|f| &f.value),
        Some(&FactValue::Num(5.0)),
        "the first source keeps the fact"
    );
    assert!(
        report
            .diagnostics
            .iter()
            .any(|d| d.message.contains("both")),
        "{:#?}",
        report.diagnostics
    );
}
