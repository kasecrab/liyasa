//! VER-22: a `url` source refreshed twice with a changed value leaves two
//! snapshots with different digests, and the change is attributable.

use std::sync::Mutex;
use std::time::{Duration, SystemTime};

use liyasa_core::conformance::block_on;
use liyasa_core::ids::FactId;
use liyasa_core::net::{BoxFut, HttpClient, HttpPolicy, HttpRequest, HttpResponse, NetError};
use liyasa_core::verify::{ChangeKind, FactValue};
use liyasa_verify::sources::kinds::{BuildTrust, DeclaredSource};
use liyasa_verify::sources::refresh::Refresher;
use liyasa_verify::sources::snapshot::SnapshotLog;
use liyasa_verify::sources::spec::SourceSpec;
use serde_json::json;

struct Api(Mutex<serde_json::Value>);

impl Api {
    fn answering(value: serde_json::Value) -> Self {
        Self(Mutex::new(value))
    }

    fn now_answers(&self, value: serde_json::Value) {
        *self.0.lock().expect("not poisoned") = value;
    }
}

impl HttpClient for Api {
    fn fetch<'a>(
        &'a self,
        req: HttpRequest,
        _policy: &'a HttpPolicy,
    ) -> BoxFut<'a, Result<HttpResponse, NetError>> {
        let body = self.0.lock().expect("not poisoned").to_string();
        Box::pin(std::future::ready(Ok(HttpResponse {
            status: 200,
            headers: Vec::new(),
            body: body.into_bytes().into(),
            final_url: req.url,
        })))
    }
}

fn source(at: SystemTime) -> DeclaredSource {
    let (spec, problems) = SourceSpec::parse(
        "pricing",
        &json!({
            "kind": "url",
            "url": "https://api.example.com/plans",
            "facts": { "plan.pro.price": "/plans/pro/price" }
        }),
    );
    assert!(problems.is_empty(), "{problems:#?}");
    DeclaredSource::new(spec).taken_at(at)
}

fn at(seconds: u64) -> SystemTime {
    SystemTime::UNIX_EPOCH + Duration::from_secs(seconds)
}

#[test]
fn two_refreshes_with_a_changed_value_are_two_attributable_snapshots() {
    let api = Api::answering(json!({ "plans": { "pro": { "price": 20 } } }));
    let log = SnapshotLog::new();

    let first = block_on(
        Refresher::new(&log, BuildTrust::Trusted, "main@aaaaaaa").refresh(
            &[source(at(0))],
            &api,
            None,
            at(0),
        ),
    );
    assert_eq!(first.refreshed, ["pricing"]);

    api.now_answers(json!({ "plans": { "pro": { "price": 25 } } }));
    let second = block_on(
        Refresher::new(&log, BuildTrust::Trusted, "main@bbbbbbb").refresh(
            &[source(at(86_400))],
            &api,
            None,
            at(86_400),
        ),
    );

    let history = log.history("pricing").expect("a history");
    assert_eq!(history.len(), 2);
    assert_ne!(
        history[0].snapshot.digest, history[1].snapshot.digest,
        "a changed value is a changed digest"
    );
    assert_eq!(history[0].by, "main@aaaaaaa");
    assert_eq!(history[1].by, "main@bbbbbbb");
    assert_eq!(history[0].snapshot.taken_at, at(0));
    assert_eq!(history[1].snapshot.taken_at, at(86_400));

    assert_eq!(second.changes.len(), 1);
    let change = &second.changes[0];
    assert_eq!(change.fact, FactId::new("plan.pro.price"));
    assert_eq!(change.kind, ChangeKind::Changed);
    assert_eq!(change.old, Some(FactValue::Num(20.0)));
    assert_eq!(change.new, Some(FactValue::Num(25.0)));
}

#[test]
fn a_refresh_that_changes_nothing_leaves_the_digest_alone() {
    let api = Api::answering(json!({ "plans": { "pro": { "price": 20 } } }));
    let log = SnapshotLog::new();
    let refresher = Refresher::new(&log, BuildTrust::Trusted, "schedule");

    block_on(refresher.refresh(&[source(at(0))], &api, None, at(0)));
    let second = block_on(refresher.refresh(&[source(at(3600))], &api, None, at(3600)));

    let history = log.history("pricing").expect("a history");
    assert_eq!(history.len(), 2, "each refresh is recorded");
    assert_eq!(
        history[0].snapshot.digest, history[1].snapshot.digest,
        "the clock moved and the values did not"
    );
    assert!(second.changes.is_empty());
}

#[test]
fn a_source_is_refreshed_on_its_schedule_and_not_before() {
    let api = Api::answering(json!({ "plans": { "pro": { "price": 20 } } }));
    let log = SnapshotLog::new();
    let refresher = Refresher::new(&log, BuildTrust::Trusted, "schedule");
    let every_day = Duration::from_secs(86_400);
    let sources = [source(at(0))];

    assert_eq!(refresher.due(&sources, at(0), every_day).len(), 1);
    block_on(refresher.refresh(&sources, &api, None, at(0)));

    assert!(refresher.due(&sources, at(3600), every_day).is_empty());
    assert_eq!(refresher.due(&sources, at(86_400), every_day).len(), 1);
}
