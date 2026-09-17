use std::sync::Mutex;

use liyasa_core::conformance::block_on;
use liyasa_core::conformance::fixtures::{MapSecrets, MemoryVfs};
use liyasa_core::net::{HttpResponse, NetError};
use liyasa_core::verify::{SandboxError, SandboxOutput};
use serde_json::json;

use super::*;

#[derive(Default)]
struct Canned {
    status: u16,
    body: Vec<u8>,
    seen: Mutex<Vec<HttpRequest>>,
}

impl Canned {
    fn json(value: serde_json::Value) -> Self {
        Self {
            status: 200,
            body: value.to_string().into_bytes(),
            seen: Mutex::new(Vec::new()),
        }
    }

    fn headers(&self) -> Vec<(String, String)> {
        self.seen
            .lock()
            .expect("not poisoned")
            .first()
            .map(|req| req.headers.clone())
            .unwrap_or_default()
    }

    fn requests(&self) -> usize {
        self.seen.lock().expect("not poisoned").len()
    }
}

impl HttpClient for Canned {
    fn fetch<'a>(
        &'a self,
        req: HttpRequest,
        _policy: &'a HttpPolicy,
    ) -> BoxFut<'a, Result<HttpResponse, NetError>> {
        self.seen.lock().expect("not poisoned").push(req.clone());
        Box::pin(std::future::ready(Ok(HttpResponse {
            status: self.status,
            headers: Vec::new(),
            body: self.body.clone().into(),
            final_url: req.url,
        })))
    }
}

struct Offline;

impl HttpClient for Offline {
    fn fetch<'a>(
        &'a self,
        _req: HttpRequest,
        _policy: &'a HttpPolicy,
    ) -> BoxFut<'a, Result<HttpResponse, NetError>> {
        Box::pin(std::future::ready(Err(NetError::Io(
            "no network in this test".to_owned(),
        ))))
    }
}

/// Echoes a fixed document, and records every job it was asked to run.
#[derive(Default)]
struct Echoing {
    stdout: Vec<u8>,
    exit: i32,
    seen: Mutex<Vec<SandboxJob>>,
}

impl Echoing {
    fn json(value: serde_json::Value) -> Self {
        Self {
            stdout: value.to_string().into_bytes(),
            exit: 0,
            seen: Mutex::new(Vec::new()),
        }
    }

    fn jobs(&self) -> Vec<SandboxJob> {
        self.seen.lock().expect("not poisoned").clone()
    }
}

impl Sandbox for Echoing {
    fn exec<'a>(&'a self, job: SandboxJob) -> BoxFut<'a, Result<SandboxOutput, SandboxError>> {
        self.seen.lock().expect("not poisoned").push(job);
        Box::pin(std::future::ready(Ok(SandboxOutput {
            exit: self.exit,
            stdout: self.stdout.clone().into(),
            stderr: Vec::new().into(),
            duration: Duration::ZERO,
        })))
    }
}

fn spec(id: &str, declaration: serde_json::Value) -> SourceSpec {
    let (spec, problems) = SourceSpec::parse(id, &declaration);
    assert!(problems.is_empty(), "{problems:#?}");
    spec
}

fn at(seconds: u64) -> SystemTime {
    SystemTime::UNIX_EPOCH + Duration::from_secs(seconds)
}

const SCRIPT: &str = "#!/bin/sh\necho '{\"seats\":5}'\n";

fn script_sha() -> String {
    sha256_hex(SCRIPT.as_bytes())
}

#[test]
fn a_file_source_reads_its_document_and_types_its_facts() {
    let vfs = Arc::new(MemoryVfs::new().with("facts/limits.json", r#"{"seats": 5}"#));
    let source = DeclaredSource::new(spec(
        "limits",
        json!({ "kind": "file", "path": "facts/limits.json" }),
    ))
    .with_vfs(vfs)
    .taken_at(at(10));

    let taken = block_on(source.snapshot(&Offline, None)).expect("a file needs no network");
    assert_eq!(taken.source, "limits");
    assert_eq!(taken.taken_at, at(10));
    assert_eq!(taken.values[&FactId::new("seats")], FactValue::Num(5.0));
    assert_eq!(source.trust(), TrustLevel::Operator);
}

#[test]
fn a_yaml_document_reads_the_same_as_a_json_one() {
    let vfs = Arc::new(MemoryVfs::new().with("facts/limits.yaml", "seats: 5\n"));
    let source = DeclaredSource::new(spec(
        "limits",
        json!({ "kind": "file", "path": "facts/limits.yaml" }),
    ))
    .with_vfs(vfs);
    let taken = block_on(source.snapshot(&Offline, None)).expect("YAML is read");
    assert_eq!(taken.values[&FactId::new("seats")], FactValue::Num(5.0));
}

#[test]
fn a_document_in_a_format_this_release_does_not_read_says_so() {
    let vfs = Arc::new(MemoryVfs::new().with("facts/limits.toml", "seats = 5\n"));
    let source = DeclaredSource::new(spec(
        "limits",
        json!({ "kind": "file", "path": "facts/limits.toml" }),
    ))
    .with_vfs(vfs);
    let refused = block_on(source.snapshot(&Offline, None)).expect_err("TOML is not read");
    assert!(format!("{refused}").contains(".json"), "{refused}");
}

#[test]
fn a_url_source_fetches_and_sends_the_credential_it_was_given() {
    let http = Canned::json(json!({ "plans": { "pro": { "price_cents": 2000 } } }));
    let secrets = Arc::new(MapSecrets::new().with("PRICING_TOKEN", "tok-0123456789abcdef"));
    let source = DeclaredSource::new(spec(
        "pricing",
        json!({
            "kind": "url",
            "url": "https://api.example.com/plans",
            "auth": "secret:PRICING_TOKEN",
            "facts": { "plan.pro.price": "/plans/pro/price_cents" },
            "types": {
                "plan.pro.price": {
                    "type": "currency", "code": "USD", "minor": 2, "minorUnits": true
                }
            }
        }),
    ))
    .with_secrets(secrets);

    let taken = block_on(source.snapshot(&http, None)).expect("the fetch succeeds");
    assert_eq!(
        taken.values[&FactId::new("plan.pro.price")],
        FactValue::Currency {
            amount: 2000,
            minor: 2,
            code: "USD".to_owned()
        }
    );
    assert_eq!(
        http.headers(),
        [(
            "authorization".to_owned(),
            "Bearer tok-0123456789abcdef".to_owned()
        )],
        "a configured credential that is never sent is not a credential"
    );
    assert_eq!(source.trust(), TrustLevel::External);
}

#[test]
fn a_url_source_whose_answer_echoes_the_credential_does_not_store_it() {
    let http = Canned::json(json!({ "token": "tok-0123456789abcdef" }));
    let secrets = Arc::new(MapSecrets::new().with("PRICING_TOKEN", "tok-0123456789abcdef"));
    let source = DeclaredSource::new(spec(
        "pricing",
        json!({
            "kind": "url", "url": "https://api.example.com/plans",
            "auth": "secret:PRICING_TOKEN"
        }),
    ))
    .with_secrets(secrets);

    let taken = block_on(source.snapshot(&http, None)).expect("the fetch succeeds");
    let rendered = serde_json::to_string(&taken.values).expect("a snapshot serializes");
    assert!(!rendered.contains("tok-0123456789abcdef"), "{rendered}");
}

#[test]
fn a_credential_that_names_no_secret_stops_the_fetch() {
    let http = Canned::json(json!({}));
    let source = DeclaredSource::new(spec(
        "pricing",
        json!({ "kind": "url", "url": "https://api.example.com/x", "auth": "secret:MISSING" }),
    ));
    let refused = block_on(source.snapshot(&http, None)).expect_err("no such secret");
    assert!(format!("{refused}").contains("MISSING"), "{refused}");
    assert_eq!(
        http.requests(),
        0,
        "nothing is sent unauthenticated instead"
    );
}

#[test]
fn a_plain_http_source_is_never_fetched() {
    let http = Canned::json(json!({ "seats": 5 }));
    let source = DeclaredSource::new(spec(
        "pricing",
        json!({ "kind": "url", "url": "http://api.example.com/plans" }),
    ));
    let refused = block_on(source.snapshot(&http, None)).expect_err("plain http");
    assert!(
        format!("{refused}").contains("allowInsecureHosts"),
        "{refused}"
    );
    assert_eq!(http.requests(), 0);
}

#[test]
fn a_source_that_answers_with_an_error_status_is_not_a_snapshot() {
    let http = Canned {
        status: 503,
        body: b"{}".to_vec(),
        seen: Mutex::new(Vec::new()),
    };
    let source = DeclaredSource::new(spec(
        "pricing",
        json!({ "kind": "url", "url": "https://api.example.com/x" }),
    ));
    let refused = block_on(source.snapshot(&http, None)).expect_err("503");
    assert!(format!("{refused}").contains("503"), "{refused}");
}

#[test]
fn a_document_that_fails_its_schema_produces_no_facts() {
    let vfs = Arc::new(MemoryVfs::new().with("f.json", r#"{"seats": "five"}"#));
    let source = DeclaredSource::new(spec(
        "limits",
        json!({
            "kind": "file", "path": "f.json",
            "schema": { "type": "object", "properties": { "seats": { "type": "number" } } }
        }),
    ))
    .with_vfs(vfs);
    let refused = block_on(source.snapshot(&Offline, None)).expect_err("the schema fails");
    assert!(matches!(refused, SourceError::Schema(_)), "{refused:?}");
}

#[test]
fn a_command_not_on_the_servers_allow_list_is_refused_rather_than_run() {
    let vfs = Arc::new(MemoryVfs::new().with("scripts/facts.sh", SCRIPT));
    let sandbox = Echoing::json(json!({ "seats": 5 }));
    let source = DeclaredSource::new(spec(
        "build",
        json!({ "kind": "command", "path": "scripts/facts.sh", "command": ["sh", "scripts/facts.sh"] }),
    ))
    .with_vfs(vfs);

    let refused = block_on(source.snapshot(&Offline, Some(&sandbox))).expect_err("not allowed");
    assert!(format!("{refused}").contains("allow"), "{refused}");
    assert!(
        sandbox.jobs().is_empty(),
        "a refused command must not have run"
    );
}

#[test]
fn a_command_on_the_list_with_the_wrong_hash_is_refused_too() {
    let vfs = Arc::new(MemoryVfs::new().with("scripts/facts.sh", SCRIPT));
    let sandbox = Echoing::json(json!({ "seats": 5 }));
    let source = DeclaredSource::new(spec(
        "build",
        json!({ "kind": "command", "path": "scripts/facts.sh", "command": ["sh", "scripts/facts.sh"] }),
    ))
    .with_vfs(vfs)
    .with_allow_list(vec![AllowedCommand {
        path: "scripts/facts.sh".to_owned(),
        sha256: "0".repeat(64),
    }]);

    let refused = block_on(source.snapshot(&Offline, Some(&sandbox))).expect_err("wrong hash");
    let text = format!("{refused}");
    assert!(text.contains(&script_sha()), "{text}");
    assert!(sandbox.jobs().is_empty());
}

#[test]
fn a_command_on_the_list_runs_in_the_sandbox() {
    let vfs = Arc::new(MemoryVfs::new().with("scripts/facts.sh", SCRIPT));
    let sandbox = Echoing::json(json!({ "seats": 5 }));
    let source = DeclaredSource::new(spec(
        "build",
        json!({ "kind": "command", "path": "scripts/facts.sh", "command": ["sh", "scripts/facts.sh"] }),
    ))
    .with_vfs(vfs)
    .with_allow_list(vec![AllowedCommand {
        path: "scripts/facts.sh".to_owned(),
        sha256: script_sha(),
    }]);

    let taken = block_on(source.snapshot(&Offline, Some(&sandbox))).expect("the command runs");
    assert_eq!(taken.values[&FactId::new("seats")], FactValue::Num(5.0));
    let jobs = sandbox.jobs();
    assert_eq!(jobs.len(), 1);
    assert_eq!(jobs[0].cmd, ["sh", "scripts/facts.sh"]);
    assert!(
        !jobs[0].network,
        "a command source gets no network by default"
    );
}

#[test]
fn a_command_with_no_sandbox_does_not_fall_back_to_this_process() {
    let vfs = Arc::new(MemoryVfs::new().with("scripts/facts.sh", SCRIPT));
    let source = DeclaredSource::new(spec(
        "build",
        json!({ "kind": "command", "path": "scripts/facts.sh", "command": ["sh"] }),
    ))
    .with_vfs(vfs)
    .with_allow_list(vec![AllowedCommand {
        path: "scripts/facts.sh".to_owned(),
        sha256: script_sha(),
    }]);
    let refused = block_on(source.snapshot(&Offline, None)).expect_err("no sandbox");
    assert!(format!("{refused}").contains("sandbox"), "{refused}");
}

#[test]
fn an_untrusted_build_refreshes_nothing_that_leaves_the_machine() {
    let http = Canned::json(json!({ "seats": 5 }));
    let sandbox = Echoing::json(json!({ "seats": 5 }));
    let vfs = Arc::new(
        MemoryVfs::new()
            .with("scripts/facts.sh", SCRIPT)
            .with("facts/limits.json", r#"{"seats": 5}"#),
    );

    for declaration in [
        json!({ "kind": "url", "url": "https://api.example.com/x" }),
        json!({ "kind": "command", "path": "scripts/facts.sh", "command": ["sh"] }),
        json!({ "kind": "screenshot", "url": "https://api.example.com/x" }),
        json!({ "kind": "openapi", "url": "https://api.example.com/openapi.json" }),
    ] {
        let source = DeclaredSource::new(spec("s", declaration.clone()))
            .with_vfs(vfs.clone())
            .with_allow_list(vec![AllowedCommand {
                path: "scripts/facts.sh".to_owned(),
                sha256: script_sha(),
            }])
            .with_build_trust(BuildTrust::Untrusted);
        assert!(source.leaves_the_machine(), "{declaration}");
        let refused =
            block_on(source.snapshot(&http, Some(&sandbox))).expect_err("an untrusted build");
        assert!(format!("{refused}").contains("untrusted"), "{refused}");
    }
    assert_eq!(http.requests(), 0);
    assert!(sandbox.jobs().is_empty());

    // A file source is on the machine already, so it still refreshes.
    let local = DeclaredSource::new(spec(
        "limits",
        json!({ "kind": "file", "path": "facts/limits.json" }),
    ))
    .with_vfs(vfs)
    .with_build_trust(BuildTrust::Untrusted);
    assert!(!local.leaves_the_machine());
    assert!(block_on(local.snapshot(&http, None)).is_ok());
}

#[test]
fn a_fork_is_untrusted_whatever_its_branch_is_called() {
    let trusted = vec!["main".to_owned()];
    assert_eq!(BuildTrust::of("main", false, &trusted), BuildTrust::Trusted);
    assert_eq!(
        BuildTrust::of("main", true, &trusted),
        BuildTrust::Untrusted
    );
    assert_eq!(
        BuildTrust::of("feature/x", false, &trusted),
        BuildTrust::Untrusted
    );
    assert_eq!(BuildTrust::of("main", false, &[]), BuildTrust::Untrusted);
}

#[test]
fn a_manual_source_is_its_own_document() {
    let source = DeclaredSource::new(spec(
        "sla",
        json!({
            "kind": "manual", "owner": "ops@example.com", "expires": "2026-12-31",
            "values": { "sla.uptime": 99.95 },
            "types": { "sla.uptime": "percentage" }
        }),
    ));
    let taken = block_on(source.snapshot(&Offline, None)).expect("no fetching needed");
    assert_eq!(
        taken.values[&FactId::new("sla.uptime")],
        FactValue::Percent(99.95)
    );
    assert_eq!(source.trust(), TrustLevel::Operator);
}

#[test]
fn an_attestation_warns_inside_the_grace_period_and_errors_after_it() {
    let spec = spec(
        "sla",
        json!({
            "kind": "manual", "owner": "ops", "expires": "2026-01-01",
            "values": { "sla.uptime": 99.95 }
        }),
    );
    let expiry = at(1_767_225_600); // 2026-01-01T00:00:00Z
    assert_eq!(
        attestation_of(&spec, expiry - Duration::from_secs(1)),
        Attestation::Valid
    );
    assert_eq!(
        attestation_of(&spec, expiry + Duration::from_secs(60)),
        Attestation::Expired
    );
    assert_eq!(
        attestation_of(&spec, expiry + DEFAULT_GRACE + Duration::from_secs(60)),
        Attestation::Lapsed
    );

    use liyasa_core::diagnostics::Severity;
    let warning = Attestation::Expired
        .diagnostic("sla", "2026-01-01")
        .expect("a diagnostic");
    assert_eq!(warning.code.as_str(), "E0606");
    assert_eq!(warning.severity, Severity::Warning);
    let error = Attestation::Lapsed
        .diagnostic("sla", "2026-01-01")
        .expect("a diagnostic");
    assert_eq!(error.severity, Severity::Error);
    assert!(Attestation::Valid.diagnostic("sla", "2026-01-01").is_none());
}

#[test]
fn only_a_manual_source_has_an_attestation() {
    let spec = spec("limits", json!({ "kind": "file", "path": "f.json" }));
    assert_eq!(attestation_of(&spec, at(0)), Attestation::NotApplicable);
}

#[test]
fn an_openapi_source_reads_from_the_repository_without_the_network() {
    let vfs = Arc::new(MemoryVfs::new().with("api/openapi.json", r#"{"openapi": "3.1.0"}"#));
    let source = DeclaredSource::new(spec(
        "petstore",
        json!({ "kind": "openapi", "path": "api/openapi.json" }),
    ))
    .with_vfs(vfs);
    let taken = block_on(source.snapshot(&Offline, None)).expect("a repository file");
    assert_eq!(
        taken.values[&FactId::new("openapi")],
        FactValue::Str("3.1.0".to_owned())
    );
    assert_eq!(source.trust(), TrustLevel::Member);
    assert!(!source.leaves_the_machine());
}
