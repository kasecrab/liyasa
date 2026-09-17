//! VER-25: a `command` source runs only if the server's allow list names its
//! script and its content hash, and an untrusted build refreshes nothing that
//! leaves the machine.
//!
//! The sandbox and the HTTP client here record what they were asked to do, so
//! "refused" is asserted as *nothing ran* rather than as an error value. A
//! refusal that still ran the command would pass the second kind of test.

use std::sync::{Arc, Mutex};
use std::time::{Duration, SystemTime};

use liyasa_core::conformance::block_on;
use liyasa_core::conformance::fixtures::MemoryVfs;
use liyasa_core::ids::FactId;
use liyasa_core::net::{BoxFut, HttpClient, HttpPolicy, HttpRequest, HttpResponse, NetError};
use liyasa_core::verify::{FactValue, Sandbox, SandboxError, SandboxJob, SandboxOutput};
use liyasa_verify::core::config::AllowedCommand;
use liyasa_verify::sources::kinds::{BuildTrust, DeclaredSource};
use liyasa_verify::sources::refresh::{RefreshReport, Refresher};
use liyasa_verify::sources::snapshot::SnapshotLog;
use liyasa_verify::sources::spec::SourceSpec;
use serde_json::json;

/// `sha256` of this is what the allow list has to carry.
const SCRIPT: &str = "#!/bin/sh\nprintf '{\"seats\": 5}'\n";
/// `printf '%s' "$SCRIPT" | sha256sum`
const SCRIPT_SHA: &str = "5a94cee1908dbf243b959532ecdaa3049d869493dc5025f439c619ca175428a7";

#[derive(Default)]
struct Recording {
    jobs: Mutex<Vec<SandboxJob>>,
    requests: Mutex<usize>,
}

impl Sandbox for Recording {
    fn exec<'a>(&'a self, job: SandboxJob) -> BoxFut<'a, Result<SandboxOutput, SandboxError>> {
        self.jobs.lock().expect("not poisoned").push(job);
        Box::pin(std::future::ready(Ok(SandboxOutput {
            exit: 0,
            stdout: br#"{"seats": 5}"#.to_vec().into(),
            stderr: Vec::new().into(),
            duration: Duration::ZERO,
        })))
    }
}

impl HttpClient for Recording {
    fn fetch<'a>(
        &'a self,
        req: HttpRequest,
        _policy: &'a HttpPolicy,
    ) -> BoxFut<'a, Result<HttpResponse, NetError>> {
        *self.requests.lock().expect("not poisoned") += 1;
        Box::pin(std::future::ready(Ok(HttpResponse {
            status: 200,
            headers: Vec::new(),
            body: br#"{"price": 999}"#.to_vec().into(),
            final_url: req.url,
        })))
    }
}

impl Recording {
    fn ran(&self) -> usize {
        self.jobs.lock().expect("not poisoned").len()
    }

    fn fetched(&self) -> usize {
        *self.requests.lock().expect("not poisoned")
    }
}

fn vfs() -> Arc<MemoryVfs> {
    Arc::new(MemoryVfs::new().with("scripts/facts.sh", SCRIPT))
}

fn command_source(allow: Vec<AllowedCommand>) -> DeclaredSource {
    let (spec, problems) = SourceSpec::parse(
        "build",
        &json!({
            "kind": "command",
            "path": "scripts/facts.sh",
            "command": ["sh", "scripts/facts.sh"]
        }),
    );
    assert!(problems.is_empty(), "{problems:#?}");
    DeclaredSource::new(spec)
        .with_vfs(vfs())
        .with_allow_list(allow)
}

fn refresh(sources: &[DeclaredSource], seams: &Recording) -> RefreshReport {
    refresh_as(sources, seams, BuildTrust::Trusted, "main")
}

fn refresh_as(
    sources: &[DeclaredSource],
    seams: &Recording,
    build: BuildTrust,
    by: &str,
) -> RefreshReport {
    let log = SnapshotLog::new();
    block_on(Refresher::new(&log, build, by).refresh(
        sources,
        seams,
        Some(seams),
        SystemTime::UNIX_EPOCH,
    ))
}

fn codes(report: &RefreshReport) -> Vec<&str> {
    report.diagnostics.iter().map(|d| d.code.as_str()).collect()
}

#[test]
fn the_allow_list_entry_is_the_hash_of_the_script_this_test_uses() {
    // If this drifts, the two refusals below stop testing what they claim to.
    let allowed = command_source(vec![AllowedCommand {
        path: "scripts/facts.sh".to_owned(),
        sha256: SCRIPT_SHA.to_owned(),
    }]);
    assert!(
        allowed
            .check_allow_list("scripts/facts.sh", SCRIPT.as_bytes())
            .is_ok(),
        "SCRIPT_SHA is not the digest of SCRIPT"
    );
}

#[test]
fn a_command_the_allow_list_does_not_name_is_refused_with_e0621() {
    let seams = Recording::default();
    let report = refresh(&[command_source(Vec::new())], &seams);
    assert_eq!(codes(&report), ["E0621"]);
    assert_eq!(seams.ran(), 0, "a refused command must not have run");
}

#[test]
fn a_command_on_the_list_with_the_wrong_hash_is_refused_with_e0621() {
    let seams = Recording::default();
    let report = refresh(
        &[command_source(vec![AllowedCommand {
            path: "scripts/facts.sh".to_owned(),
            sha256: "0".repeat(64),
        }])],
        &seams,
    );
    assert_eq!(codes(&report), ["E0621"]);
    assert_eq!(seams.ran(), 0);
}

#[test]
fn a_command_the_server_named_and_hashed_runs() {
    let seams = Recording::default();
    let report = refresh(
        &[command_source(vec![AllowedCommand {
            path: "scripts/facts.sh".to_owned(),
            sha256: SCRIPT_SHA.to_owned(),
        }])],
        &seams,
    );
    assert!(report.diagnostics.is_empty(), "{:#?}", report.diagnostics);
    assert_eq!(seams.ran(), 1);
    assert_eq!(
        report.facts.get(&FactId::new("seats")).map(|f| &f.value),
        Some(&FactValue::Num(5.0))
    );
}

#[test]
fn a_fork_build_runs_nothing_that_leaves_the_machine_and_reads_production() {
    let allow = vec![AllowedCommand {
        path: "scripts/facts.sh".to_owned(),
        sha256: SCRIPT_SHA.to_owned(),
    }];
    let url = |id: &str| {
        let (spec, problems) = SourceSpec::parse(
            id,
            &json!({
                "kind": "url",
                "url": "https://api.example.com/plans",
                "facts": { "plan.pro.price": "/price" }
            }),
        );
        assert!(problems.is_empty(), "{problems:#?}");
        DeclaredSource::new(spec)
    };

    // One trusted build, so there is a production snapshot to fall back to.
    let log = SnapshotLog::new();
    let trusted = Recording::default();
    block_on(Refresher::new(&log, BuildTrust::Trusted, "main").refresh(
        &[url("pricing"), command_source(allow.clone())],
        &trusted,
        Some(&trusted),
        SystemTime::UNIX_EPOCH,
    ));
    assert_eq!((trusted.fetched(), trusted.ran()), (1, 1));

    let fork = Recording::default();
    let report = block_on(
        Refresher::new(&log, BuildTrust::Untrusted, "fork/pr-7").refresh(
            &[
                url("pricing").with_build_trust(BuildTrust::Untrusted),
                command_source(allow).with_build_trust(BuildTrust::Untrusted),
            ],
            &fork,
            Some(&fork),
            SystemTime::UNIX_EPOCH,
        ),
    );

    assert_eq!(
        (fork.fetched(), fork.ran()),
        (0, 0),
        "a fork build reaches nothing"
    );
    assert_eq!(report.reused, ["pricing", "build"]);
    assert!(report.refreshed.is_empty());
    assert!(report.diagnostics.is_empty(), "{:#?}", report.diagnostics);
    // The values are the production ones, not whatever the fork's seams say.
    assert_eq!(
        report.facts.get(&FactId::new("seats")).map(|f| &f.value),
        Some(&FactValue::Num(5.0))
    );
    assert_eq!(
        report
            .facts
            .get(&FactId::new("plan.pro.price"))
            .map(|f| &f.value),
        Some(&FactValue::Num(999.0))
    );
}

#[test]
fn a_fork_build_does_not_overwrite_the_production_snapshot() {
    let log = SnapshotLog::new();
    let seams = Recording::default();
    let allow = vec![AllowedCommand {
        path: "scripts/facts.sh".to_owned(),
        sha256: SCRIPT_SHA.to_owned(),
    }];
    block_on(Refresher::new(&log, BuildTrust::Trusted, "main").refresh(
        &[command_source(allow.clone())],
        &seams,
        Some(&seams),
        SystemTime::UNIX_EPOCH,
    ));
    block_on(
        Refresher::new(&log, BuildTrust::Untrusted, "fork/pr-7").refresh(
            &[command_source(allow).with_build_trust(BuildTrust::Untrusted)],
            &seams,
            Some(&seams),
            SystemTime::UNIX_EPOCH,
        ),
    );
    let production = log
        .latest_production("build")
        .expect("a query")
        .expect("a production snapshot");
    assert_eq!(production.by, "main");
}
