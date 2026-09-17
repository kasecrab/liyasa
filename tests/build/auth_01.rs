//! AUTH-01: `public: true` is the default, and a public site has no auth in it.
//!
//! "No auth code paths are included in the output" is checked against the
//! built site, not against the server: a static export is a directory of files
//! served by something that is not Liyasa, so anything auth-shaped in it is
//! either dead weight or a promise the host cannot keep. Every route then has
//! to serve with no session, which is checked through the server as well,
//! because a public site served by `liyasa serve` must not start asking for
//! one either.

use liyasa_build::engine::Options;
use liyasa_server::auth::config::{AuthConfig, Mode};
use liyasa_server::auth::groups::{Decision, Declared, SiteDefault, decide};
use liyasa_tests::hosting::{CONFIG, Site};
use liyasa_tests::server::{Harness, Setup, expect_status};

/// Text that would mean the output is expecting a session.
const AUTH_SHAPED: &[&str] = &[
    "/_liyasa/auth",
    "liyasa_session",
    "x-liyasa-csrf",
    "liyasa-site-verification",
];

fn output_files(dist: &std::path::Path) -> Vec<std::path::PathBuf> {
    let mut out = Vec::new();
    let mut stack = vec![dist.to_path_buf()];
    while let Some(dir) = stack.pop() {
        let Ok(entries) = std::fs::read_dir(&dir) else {
            continue;
        };
        for entry in entries.flatten() {
            let path = entry.path();
            match path.is_dir() {
                true => stack.push(path),
                false => out.push(path),
            }
        }
    }
    out.sort();
    out
}

#[test]
fn a_site_that_says_nothing_about_auth_is_public() {
    // The default is the requirement, so it is asserted from the config that
    // an ordinary site actually has rather than from `AuthConfig::default()`.
    let config: serde_json::Value = serde_json::from_str(CONFIG).expect("the fixture config");
    assert!(config.get("auth").is_none(), "the fixture declares no auth");
    let auth = AuthConfig::from_site_config(&config).expect("a default");
    assert_eq!(auth.mode, Mode::Public);
    assert!(auth.mode.is_public());
}

#[test]
fn every_route_of_a_public_site_is_allowed_without_a_reader() {
    // The one decision function, asked the way a public site asks it.
    assert_eq!(
        decide(SiteDefault::Public, &[Declared::default()], None),
        Decision::Allow
    );
}

#[test]
fn a_statically_built_public_site_carries_no_auth_in_its_output() {
    let site = Site::build("auth-01", CONFIG, Options::default());
    let dist = site.dist();
    let files = output_files(&dist);
    assert!(!files.is_empty(), "the fixture built something");

    let mut offenders: Vec<String> = Vec::new();
    for path in &files {
        // Only text output can carry a code path; an image cannot.
        let Ok(text) = std::fs::read_to_string(path) else {
            continue;
        };
        for needle in AUTH_SHAPED {
            if text.contains(needle) {
                offenders.push(format!(
                    "{}: {needle}",
                    path.strip_prefix(&dist).unwrap_or(path).display()
                ));
            }
        }
    }
    assert!(
        offenders.is_empty(),
        "a public site's output names auth: {offenders:#?}"
    );
}

#[test]
fn a_public_sites_agent_surfaces_are_not_gated_either() {
    // AUTH-07 says `llms.txt` and the `.md` routes follow the same rules as
    // the pages, so on a public site they are ungated like the pages.
    let site = Site::build("auth-01-agents", CONFIG, Options::default());
    for surface in ["llms.txt", "guides/install.md"] {
        let path = site.dist().join(surface);
        if !path.exists() {
            continue;
        }
        let text = std::fs::read_to_string(&path).expect("a readable surface");
        for needle in AUTH_SHAPED {
            assert!(!text.contains(needle), "{surface} names {needle}");
        }
    }
}

#[tokio::test]
async fn every_route_serves_without_a_session() {
    let (harness, site) = Harness::new(Setup::new("auth-01")).await;
    let site = site.expect("the fixture site");
    let manifest: serde_json::Value =
        serde_json::from_str(&site.read("liyasa-manifest.json")).expect("the manifest is JSON");
    let routes: Vec<String> = manifest["routes"]
        .as_array()
        .expect("the manifest lists routes")
        .iter()
        .filter(|entry| entry["hidden"] != serde_json::Value::Bool(true))
        .filter_map(|entry| entry["route"].as_str().map(str::to_owned))
        .collect();
    assert!(!routes.is_empty(), "the fixture has routes");

    for route in &routes {
        let response = harness.get(route).await;
        let status = response.status();
        assert!(
            status.is_success() || status.is_redirection(),
            "{route} answered {status} with no session"
        );
        assert!(
            response.headers().get(http::header::SET_COOKIE).is_none(),
            "{route} set a cookie on a public site"
        );
        assert_ne!(
            status,
            http::StatusCode::UNAUTHORIZED,
            "{route} asked for a session"
        );
    }

    // And the auth endpoint table is not mounted on a public site: nothing
    // routes there, so `/_liyasa/auth/session` is a 404 like any other path
    // the site does not have.
    let response = harness.get("/_liyasa/auth/session").await;
    assert_ne!(
        response.status(),
        http::StatusCode::OK,
        "a public site answers no auth endpoint"
    );
    expect_status(
        harness.get("/_liyasa/auth/login").await,
        http::StatusCode::NOT_FOUND,
    );
}
