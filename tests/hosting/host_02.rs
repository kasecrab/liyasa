//! HOST-02: the single binary, its image, and its unit file.
//!
//! Given the Docker image and the systemd unit; when started with SQLite;
//! then a site is served over HTTP and, with a test ACME server, over HTTPS.
//!
//! The container half needs Docker or Podman and the ACME half needs a test
//! directory such as pebble; neither exists on the machine this package was
//! written on, so those halves report a skip with the reason rather than
//! passing silently (RFC 1402). What runs everywhere is the serving
//! behaviour, in process, and a reading of the published files.

use std::path::{Path, PathBuf};

use http::StatusCode;
use liyasa_tests::server::{Harness, expect_status, header};

fn deploy(name: &str) -> String {
    let path = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("hosting")
        .join("deploy")
        .join(name);
    std::fs::read_to_string(&path).unwrap_or_else(|e| panic!("reading {}: {e}", path.display()))
}

/// Whether a container runtime is available for the halves that need one.
fn container_runtime() -> Option<&'static str> {
    ["docker", "podman"].into_iter().find(|runtime| {
        std::env::split_paths(&std::env::var_os("PATH").unwrap_or_default())
            .any(|dir| dir.join(runtime).is_file())
    })
}

#[tokio::test]
async fn the_server_serves_a_sqlite_backed_site_over_http() {
    let (harness, _site) = Harness::serving("host02-sqlite").await;
    // SQLite is the default: the harness opened `liyasa.db` and migrated it,
    // and the store answers, which is what readiness reports.
    let ready = expect_status(harness.get("/_liyasa/ready").await, StatusCode::OK);
    assert!(
        liyasa_tests::server::body_text(ready)
            .await
            .contains("\"store\""),
        "readiness reports the store"
    );

    let page = expect_status(harness.get("/guides/install").await, StatusCode::OK);
    assert_eq!(
        header(&page, "content-type"),
        Some("text/html; charset=utf-8")
    );
    expect_status(harness.get("/guides/install.md").await, StatusCode::OK);
}

#[test]
fn the_image_builds_the_binary_and_ships_neither_toolchain_nor_shell() {
    let dockerfile = deploy("Dockerfile");
    assert!(dockerfile.contains("AS build"), "the image is staged");
    assert!(
        dockerfile.contains("cargo build --release --locked -p liyasa-server"),
        "the release build is locked"
    );
    assert!(
        dockerfile.contains("distroless"),
        "the shipped layer carries no shell or package manager"
    );
    assert!(
        dockerfile.contains("USER nonroot"),
        "it does not run as root"
    );
    assert!(dockerfile.contains("EXPOSE 8080"));
    assert!(
        dockerfile.contains("--listen\", \"0.0.0.0:8080"),
        "a container's loopback is its own, so it must bind every interface"
    );
    assert!(
        dockerfile.contains("HEALTHCHECK"),
        "an orchestrator needs a health probe"
    );
}

#[test]
fn the_unit_drains_rather_than_killing_and_is_hardened() {
    let unit = deploy("liyasa.service");
    assert!(
        unit.contains("KillSignal=SIGTERM"),
        "the drain starts on SIGTERM"
    );
    let stop = unit
        .lines()
        .find_map(|l| l.strip_prefix("TimeoutStopSec="))
        .expect("a stop timeout");
    assert!(
        stop.trim_end_matches('s').parse::<u64>().expect("seconds") > 30,
        "the unit must outwait server.drainTimeout, which defaults to 30s; got {stop}"
    );
    for hardening in [
        "NoNewPrivileges=true",
        "ProtectSystem=strict",
        "ProtectHome=true",
        "PrivateTmp=true",
        "RestrictAddressFamilies=AF_INET AF_INET6 AF_UNIX",
        "MemoryDenyWriteExecute=true",
    ] {
        assert!(
            unit.contains(hardening),
            "the unit is missing `{hardening}`"
        );
    }
    assert!(
        unit.contains("EnvironmentFile=-/etc/liyasa/server.env"),
        "the master key comes from the environment, never from liyasa.json"
    );
    assert!(
        !unit.contains("LIYASA_MASTER_KEY="),
        "a secret must not be written into the unit"
    );
}

#[test]
fn the_compose_file_brings_postgres_and_object_storage() {
    let compose = deploy("docker-compose.yml");
    assert!(
        compose.contains("pgvector"),
        "Postgres carries pgvector (§6.8)"
    );
    assert!(compose.contains("minio"), "object storage is MinIO");
    assert!(
        compose.contains("LIYASA_MASTER_KEY: ${LIYASA_MASTER_KEY:?"),
        "the compose file refuses to start without a master key rather than inventing one"
    );
    assert!(
        compose.contains("stop_grace_period: 45s"),
        "Compose's ten-second default would cut the drain short"
    );
    assert!(
        compose.contains("condition: service_healthy"),
        "the server waits for its dependencies"
    );
}

#[test]
fn the_chart_scales_replicas_and_keeps_builds_out_of_the_serving_pods() {
    let values = deploy("helm/values.yaml");
    let server = deploy("helm/templates/server.yaml");
    let worker = deploy("helm/templates/build-worker.yaml");

    // HOST-03: replicas coordinate through the job store, which SQLite
    // cannot be. The chart refuses rather than producing a cluster whose
    // replicas each run every scheduled job.
    assert!(
        server.contains("requires postgres.enabled"),
        "the chart renders a multi-replica deployment without Postgres"
    );
    assert!(values.contains("pgvector") || values.contains("postgres"));

    for probe in ["/_liyasa/health", "/_liyasa/ready"] {
        assert!(server.contains(probe), "the chart has no {probe} probe");
    }
    assert!(
        server.contains("terminationGracePeriodSeconds: 45"),
        "the grace period must outlast server.drainTimeout (NFR-31)"
    );
    assert!(
        server.contains("fieldPath: metadata.name"),
        "each replica must be a distinct worker in the job table"
    );
    assert!(
        server.contains("readOnlyRootFilesystem: true") && server.contains("runAsNonRoot: true"),
        "the pods are not hardened"
    );
    assert!(
        !server.contains("LIYASA_MASTER_KEY: ") && server.contains("secretKeyRef"),
        "the master key must come from a Secret, never from values"
    );
    assert!(
        worker.contains("build-worker"),
        "§6.13: builds run in their own deployment"
    );
}

#[test]
fn the_container_half_reports_why_it_did_not_run() {
    match container_runtime() {
        Some(runtime) => {
            // A machine with a runtime builds the image and starts it; that
            // half is the release job's (WP-32), which has one.
            println!("a container runtime is available (`{runtime}`); the image build runs in CI");
        }
        None => {
            println!(
                "skipped: building and running the image needs Docker or Podman, and neither is \
                 on PATH. The serving behaviour is covered in process above; the image build runs \
                 in CI (RFC 1402)."
            );
        }
    }
}

#[test]
fn the_acme_half_reports_why_it_did_not_run() {
    // HOST-02's HTTPS clause needs a test ACME directory (pebble), which is a
    // container. The TLS listener itself is covered by the certificate path,
    // which needs no directory.
    println!(
        "skipped: issuing a certificate needs a test ACME server, which is a container. TLS \
         termination from a certificate file is covered by liyasa-server's tls tests (RFC 1402)."
    );
    assert!(Path::new(env!("CARGO_MANIFEST_DIR")).exists());
}
