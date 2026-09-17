use std::sync::Mutex;

use liyasa_core::conformance::block_on;

use super::*;

const DIGEST: &str = "sha256:0123456789abcdef0123456789abcdef0123456789abcdef0123456789abcdef";

struct Canned {
    status: u16,
    body: Vec<u8>,
    seen: Mutex<Vec<HttpRequest>>,
    policies: Mutex<Vec<HttpPolicy>>,
}

impl Canned {
    fn new(status: u16, body: impl Into<Vec<u8>>) -> Arc<Self> {
        Arc::new(Self {
            status,
            body: body.into(),
            seen: Mutex::new(Vec::new()),
            policies: Mutex::new(Vec::new()),
        })
    }

    fn json(value: serde_json::Value) -> Arc<Self> {
        Self::new(200, value.to_string().into_bytes())
    }
}

impl HttpClient for Canned {
    fn fetch<'a>(
        &'a self,
        req: HttpRequest,
        policy: &'a HttpPolicy,
    ) -> BoxFut<'a, Result<liyasa_core::net::HttpResponse, NetError>> {
        self.seen.lock().expect("not poisoned").push(req.clone());
        self.policies
            .lock()
            .expect("not poisoned")
            .push(policy.clone());
        let response = liyasa_core::net::HttpResponse {
            status: self.status,
            headers: Vec::new(),
            body: Bytes::from(self.body.clone()),
            final_url: req.url,
        };
        Box::pin(std::future::ready(Ok(response)))
    }
}

struct Refuses(NetError);

impl HttpClient for Refuses {
    fn fetch<'a>(
        &'a self,
        _req: HttpRequest,
        _policy: &'a HttpPolicy,
    ) -> BoxFut<'a, Result<liyasa_core::net::HttpResponse, NetError>> {
        Box::pin(std::future::ready(Err(self.0.clone())))
    }
}

fn url() -> Url {
    Url::parse("https://runners.liyasa.dev/v1/jobs").expect("a url")
}

fn job() -> SandboxJob {
    SandboxJob {
        image: "docker.io/library/python".to_owned(),
        digest: DIGEST.to_owned(),
        cmd: vec!["python".to_owned(), "main.py".to_owned()],
        files: vec![(
            liyasa_core::vfs::VfsPath::new("main.py"),
            Bytes::from(b"print('hi')".to_vec()),
        )],
        env: vec![("LANG".to_owned(), "C".to_owned())],
        timeout: Duration::from_secs(30),
        network: false,
        cpu_millis: 500,
        mem_bytes: 1024,
    }
}

fn sandbox(client: Arc<dyn HttpClient>) -> RemoteSandbox {
    RemoteSandbox::new(RemoteService::new(client, url()).with_token("t0ken"))
}

#[test]
fn a_job_is_posted_as_json_with_its_files_and_limits() {
    let client = Canned::json(serde_json::json!({
        "exit": 0,
        "stdout": "aGk=",
        "stderr": "",
        "durationMs": 12
    }));
    let out = block_on(sandbox(client.clone()).exec(job())).expect("the service answered");
    assert_eq!(out.exit, 0);
    assert_eq!(out.stdout.as_ref(), b"hi");
    assert_eq!(out.duration, Duration::from_millis(12));

    let seen = client.seen.lock().expect("not poisoned");
    assert_eq!(seen[0].method, Method::POST);
    let sent: serde_json::Value =
        serde_json::from_slice(seen[0].body.as_ref().expect("a body").as_ref()).expect("json");
    assert_eq!(sent["digest"], DIGEST);
    assert_eq!(sent["cmd"][1], "main.py");
    assert_eq!(sent["files"][0]["path"], "main.py");
    assert_eq!(sent["env"][0]["name"], "LANG");
    assert_eq!(sent["timeoutMs"], 30_000);
    assert_eq!(sent["cpuMillis"], 500);
    assert_eq!(sent["memBytes"], 1024);
    assert_eq!(sent["network"], false);
}

#[test]
fn the_token_authorises_the_call_and_is_in_no_other_field() {
    let client = Canned::json(serde_json::json!({"exit": 0}));
    block_on(sandbox(client.clone()).exec(job())).expect("answered");
    let seen = client.seen.lock().expect("not poisoned");
    assert!(
        seen[0]
            .headers
            .iter()
            .any(|(k, v)| k == "authorization" && v == "Bearer t0ken")
    );
    let body = String::from_utf8_lossy(seen[0].body.as_ref().expect("a body").as_ref()).to_string();
    assert!(!body.contains("t0ken"), "the token is in the body: {body}");
}

#[test]
fn only_the_services_own_host_is_allowed_and_no_redirect_is_followed() {
    let client = Canned::json(serde_json::json!({"exit": 0}));
    block_on(sandbox(client.clone()).exec(job())).expect("answered");
    let policies = client.policies.lock().expect("not poisoned");
    assert!(policies[0].allow_hosts.matches("runners.liyasa.dev"));
    assert!(!policies[0].allow_hosts.matches("elsewhere.example"));
    assert_eq!(policies[0].max_redirects, 0);
    assert!(!policies[0].allow_private);
}

#[test]
fn an_unpinned_image_never_leaves_the_process() {
    let client = Canned::json(serde_json::json!({"exit": 0}));
    let error = block_on(sandbox(client.clone()).exec(SandboxJob {
        digest: String::new(),
        ..job()
    }))
    .expect_err("refused");
    assert!(matches!(error, SandboxError::Image(_)), "{error:?}");
    assert!(client.seen.lock().expect("not poisoned").is_empty());
}

#[test]
fn a_service_that_says_the_job_timed_out_is_a_timeout() {
    let client = Canned::json(serde_json::json!({"exit": 0, "timedOut": true}));
    assert_eq!(
        block_on(sandbox(client).exec(job())).expect_err("timeout"),
        SandboxError::Timeout
    );
}

#[test]
fn a_gateway_timeout_is_a_timeout_not_a_broken_service() {
    for status in [408, 504] {
        let client = Canned::new(status, Vec::new());
        assert_eq!(
            block_on(sandbox(client).exec(job())).expect_err("timeout"),
            SandboxError::Timeout,
            "{status}"
        );
    }
}

#[test]
fn a_service_error_is_reported_with_its_status() {
    let client = Canned::new(503, Vec::new());
    let error = block_on(sandbox(client).exec(job())).expect_err("refused");
    assert!(
        matches!(&error, SandboxError::Io(detail) if detail.contains("503")),
        "{error:?}"
    );
}

#[test]
fn a_body_the_service_cannot_have_meant_is_an_error_not_a_pass() {
    let client = Canned::new(200, b"not json".to_vec());
    assert!(matches!(
        block_on(sandbox(client).exec(job())).expect_err("refused"),
        SandboxError::Io(_)
    ));
}

#[test]
fn a_network_timeout_stays_a_timeout() {
    let client = Arc::new(Refuses(NetError::Timeout));
    assert_eq!(
        block_on(sandbox(client).exec(job())).expect_err("timeout"),
        SandboxError::Timeout
    );
}

#[test]
fn a_policy_denial_is_an_io_error_the_operator_can_read() {
    let client = Arc::new(Refuses(NetError::PolicyDenied {
        reason: liyasa_core::net::DenyReason::Scheme,
    }));
    let error = block_on(sandbox(client).exec(job())).expect_err("denied");
    assert!(
        matches!(&error, SandboxError::Io(detail) if detail.contains("https")),
        "{error:?}"
    );
}

#[test]
fn base64_round_trips_every_length() {
    for len in 0..32usize {
        let bytes: Vec<u8> = (0..len).map(|i| (i * 7 + 3) as u8).collect();
        let text = base64::encode(&bytes);
        assert_eq!(base64::decode(&text).expect("decodes"), bytes, "len {len}");
        assert_eq!(text.len() % 4, 0, "padding is kept: {text}");
    }
}

#[test]
fn base64_matches_the_encoding_everyone_else_uses() {
    assert_eq!(base64::encode(b""), "");
    assert_eq!(base64::encode(b"f"), "Zg==");
    assert_eq!(base64::encode(b"fo"), "Zm8=");
    assert_eq!(base64::encode(b"foo"), "Zm9v");
    assert_eq!(base64::encode(b"foob"), "Zm9vYg==");
    assert_eq!(base64::encode(b"fooba"), "Zm9vYmE=");
    assert_eq!(base64::encode(b"foobar"), "Zm9vYmFy");
    assert_eq!(base64::decode("Zm9vYmFy").expect("decodes"), b"foobar");
}

#[test]
fn base64_refuses_a_character_that_is_not_in_the_alphabet() {
    assert!(base64::decode("Zm9v*mFy").is_none());
}
