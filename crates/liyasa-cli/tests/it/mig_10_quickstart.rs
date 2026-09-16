//! MIG-10: `liyasa new` then `liyasa dev`, and the site is served.
//!
//! The requirement's minute is a cold-machine figure including the download,
//! which a test in this repository cannot measure; what it can assert is that
//! the two commands compose with nothing in between, and that the second one
//! answers over HTTP with the page the first one wrote.

use std::io::{BufRead, BufReader, Read, Write};
use std::net::TcpStream;
use std::process::{Child, Command, Stdio};
use std::time::{Duration, Instant};

use liyasa_cli::Exit;

use crate::support::{Dir, Run, binary};

/// How long to wait for the dev server to answer. Generous, because the first
/// render is a real build and this machine may be running eleven other ones.
const PATIENCE: Duration = Duration::from_secs(60);

/// A port nothing is listening on. Bound and released, which races with any
/// other process that wants the same one; the window is microseconds and the
/// alternative is a fixed port that collides with the developer's own server.
fn free_port() -> u16 {
    let listener = std::net::TcpListener::bind("127.0.0.1:0").expect("a free port");
    let port = listener.local_addr().expect("an address").port();
    drop(listener);
    port
}

/// The dev server, killed when the test ends however it ends.
struct Dev {
    child: Child,
    port: u16,
}

impl Dev {
    fn start(project: &std::path::Path) -> Self {
        let port = free_port();
        let mut command = Command::new(binary());
        command
            .args(["dev", "--no-open", "--port"])
            .arg(port.to_string())
            .current_dir(project)
            .stdout(Stdio::piped())
            .stderr(Stdio::piped());
        for (name, _) in std::env::vars_os() {
            if name.to_string_lossy().starts_with("LIYASA_") {
                command.env_remove(name);
            }
        }
        command.env("NO_COLOR", "1");
        let child = command.spawn().expect("the dev server starts");
        Self { child, port }
    }

    /// One GET, or `None` while the server is not up yet.
    fn get(&self, path: &str) -> Option<(u16, String)> {
        let mut stream = TcpStream::connect(("127.0.0.1", self.port)).ok()?;
        stream
            .set_read_timeout(Some(Duration::from_secs(10)))
            .ok()?;
        write!(
            stream,
            "GET {path} HTTP/1.1\r\nHost: 127.0.0.1\r\nConnection: close\r\n\r\n"
        )
        .ok()?;

        let mut response = String::new();
        stream.read_to_string(&mut response).ok()?;
        let status = response
            .split_whitespace()
            .nth(1)
            .and_then(|code| code.parse().ok())?;
        let body = response
            .split_once("\r\n\r\n")
            .map_or(String::new(), |(_, body)| body.to_owned());
        Some((status, body))
    }

    /// Polls until the server answers, and reports how long that took.
    fn wait_until_serving(&self) -> Option<Duration> {
        let started = Instant::now();
        while started.elapsed() < PATIENCE {
            if self.get("/").is_some() {
                return Some(started.elapsed());
            }
            std::thread::sleep(Duration::from_millis(100));
        }
        None
    }

    fn output(&mut self) -> String {
        let mut text = String::new();
        if let Some(stdout) = self.child.stdout.take() {
            for line in BufReader::new(stdout).lines().map_while(Result::ok) {
                text.push_str(&line);
                text.push('\n');
            }
        }
        text
    }
}

impl Drop for Dev {
    fn drop(&mut self) {
        let _ = self.child.kill();
        let _ = self.child.wait();
    }
}

#[test]
fn new_then_dev_serves_the_site() {
    let root = Dir::new("mig10-quickstart");
    let project = root.path().join("docs");

    let created = Run::new(["new", "docs", "--yes"]).cwd(root.path()).output();
    assert_eq!(created.code, Exit::Success.code(), "{}", created.all());

    let dev = Dev::start(&project);
    let elapsed = dev
        .wait_until_serving()
        .unwrap_or_else(|| panic!("the dev server did not answer within {PATIENCE:?}"));
    assert!(elapsed < PATIENCE);

    let (status, body) = dev.get("/").expect("a response");
    assert_eq!(status, 200);
    assert!(body.contains("<html"), "not an HTML page: {body}");
    // The scaffold's landing page, so it is this project being served rather
    // than a stale directory.
    assert!(body.contains("Docs"), "not the scaffolded page: {body}");
}

#[test]
fn the_dev_server_injects_live_reload() {
    let root = Dir::new("mig10-reload");
    let project = root.path().join("docs");
    Run::new(["new", "docs", "--yes"]).cwd(root.path()).output();

    let dev = Dev::start(&project);
    dev.wait_until_serving().expect("the dev server answers");

    let (_, body) = dev.get("/").expect("a response");
    assert!(
        body.contains(liyasa_cli::serve::BUILD_ENDPOINT),
        "no reload script: {body}"
    );

    let (status, counter) = dev
        .get(liyasa_cli::serve::BUILD_ENDPOINT)
        .expect("the counter");
    assert_eq!(status, 200);
    assert!(counter.trim().parse::<u64>().is_ok(), "{counter:?}");
}

#[test]
fn the_dev_server_serves_nested_routes_and_markdown_twins() {
    let root = Dir::new("mig10-routes");
    let project = root.path().join("docs");
    Run::new(["new", "docs", "--yes"]).cwd(root.path()).output();

    let dev = Dev::start(&project);
    dev.wait_until_serving().expect("the dev server answers");

    let (status, body) = dev.get("/guides/quickstart").expect("a response");
    assert_eq!(status, 200, "{body}");
    assert!(body.contains("Quickstart"), "{body}");

    let (status, markdown) = dev.get("/guides/quickstart.md").expect("a response");
    assert_eq!(status, 200);
    assert!(markdown.contains("Quickstart"), "{markdown}");
}

#[test]
fn an_unknown_route_is_a_404() {
    let root = Dir::new("mig10-404");
    let project = root.path().join("docs");
    Run::new(["new", "docs", "--yes"]).cwd(root.path()).output();

    let dev = Dev::start(&project);
    dev.wait_until_serving().expect("the dev server answers");

    let (status, _) = dev.get("/no-such-page").expect("a response");
    assert_eq!(status, 404);
}

/// A dev server that served the whole filesystem would be a hole even bound to
/// localhost.
#[test]
fn the_dev_server_refuses_to_climb_out_of_the_output() {
    let root = Dir::new("mig10-traversal");
    let project = root.path().join("docs");
    Run::new(["new", "docs", "--yes"]).cwd(root.path()).output();

    let dev = Dev::start(&project);
    dev.wait_until_serving().expect("the dev server answers");

    let (status, body) = dev.get("/../../liyasa.json").expect("a response");
    assert_eq!(status, 404, "{body}");
    assert!(!body.contains("canonicalOrigin"), "{body}");
}

/// The plan under `--dry-run` says where it would listen and serves nothing.
#[test]
fn a_dry_run_dev_prints_the_plan_and_exits() {
    let root = Dir::new("mig10-dry-run");
    let project = root.path().join("docs");
    Run::new(["new", "docs", "--yes"]).cwd(root.path()).output();

    let outcome = Run::new(["dev", "--dry-run"]).cwd(&project).output();
    assert_eq!(outcome.code, Exit::Success.code(), "{}", outcome.all());
    assert!(outcome.stdout.contains("listen"), "{}", outcome.stdout);
}

/// The whole point of the quickstart: nothing between the two commands.
#[test]
fn the_first_render_reports_the_pages_it_built() {
    let root = Dir::new("mig10-first-render");
    let project = root.path().join("docs");
    Run::new(["new", "docs", "--yes"]).cwd(root.path()).output();

    let mut dev = Dev::start(&project);
    dev.wait_until_serving().expect("the dev server answers");
    drop(dev.get("/"));

    // Killing it closes the pipe, which ends the read below.
    let _ = dev.child.kill();
    let printed = dev.output();
    assert!(printed.contains("serving http://"), "{printed}");
    assert!(printed.contains("pages in"), "{printed}");
}
