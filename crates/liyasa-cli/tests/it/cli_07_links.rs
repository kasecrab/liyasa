//! CLI-07: `liyasa broken-links` checks external links, with the concurrency,
//! timeout and allow list the requirement names.
//!
//! Nothing here leaves the machine. Every external link in these fixtures
//! points at a loopback address, which `liyasa-net` refuses at connect time
//! (§30.2.3) — the whole path runs, the request never reaches a wire, and the
//! answer is the same on a machine with no network at all.

use liyasa_cli::Exit;
use liyasa_cli::links::anchors;

use crate::support::{Dir, Run};

/// `127.0.0.1:9` is the discard port on loopback: a private address, so the
/// policy denies it before a socket is opened.
const DEAD: &str = "http://127.0.0.1:9/gone";

fn site(name: &str) -> Dir {
    let project = Dir::new(name);
    project.write("liyasa.json", r#"{"name":"Acme docs"}"#);
    project.write(
        "index.md",
        &format!(
            "---\ntitle: Home\ndescription: The home page.\n---\n\n# Home\n\nA [dead link]({DEAD}).\n"
        ),
    );
    project
}

#[test]
fn an_external_link_that_cannot_be_reached_is_reported() {
    let project = site("links-dead");
    let outcome = Run::new(["broken-links", "--timeout", "2"])
        .cwd(project.path())
        .output();

    assert_eq!(outcome.code, Exit::Verification.code(), "{}", outcome.all());
    assert!(outcome.all().contains("W0404"), "{}", outcome.all());
    assert!(outcome.all().contains("127.0.0.1"), "{}", outcome.all());
}

/// The page the link is on, because "`http://…` answered 404" on a site with
/// four hundred pages is not an actionable report.
#[test]
fn the_report_names_the_page_the_link_is_on() {
    let project = site("links-page");
    let outcome = Run::new(["broken-links", "--timeout", "2"])
        .cwd(project.path())
        .output();
    assert!(outcome.all().contains("linked from"), "{}", outcome.all());
}

/// HOST-08: `--offline` is a request not to leave the machine, and a run that
/// honours it is not a failed run.
#[test]
fn offline_does_not_check_external_links() {
    let project = site("links-offline");
    let outcome = Run::new(["broken-links", "--offline"])
        .cwd(project.path())
        .output();

    assert_eq!(outcome.code, Exit::Success.code(), "{}", outcome.all());
    assert!(!outcome.all().contains("W0404"), "{}", outcome.all());
}

#[test]
fn internal_only_does_not_check_external_links() {
    let project = site("links-internal");
    let outcome = Run::new(["broken-links", "--internal-only"])
        .cwd(project.path())
        .output();

    assert_eq!(outcome.code, Exit::Success.code(), "{}", outcome.all());
    assert!(!outcome.all().contains("W0404"), "{}", outcome.all());
}

/// CLI-07's allow list: a host that is trusted, or that refuses automated
/// requests, is accepted without being asked.
#[test]
fn an_allowed_host_is_not_requested() {
    let project = site("links-allow");
    let outcome = Run::new(["broken-links", "--allow", "127.0.0.1"])
        .cwd(project.path())
        .output();

    assert_eq!(outcome.code, Exit::Success.code(), "{}", outcome.all());
    assert!(!outcome.all().contains("W0404"), "{}", outcome.all());
}

/// A link that is internal is still checked by the build, so the command is
/// not merely an external-link command wearing a general name.
#[test]
fn an_internal_link_to_nowhere_is_still_reported() {
    let project = Dir::new("links-internal-broken");
    project.write("liyasa.json", r#"{"name":"Acme docs"}"#);
    project.write(
        "index.md",
        "---\ntitle: Home\ndescription: The home page.\n---\n\n# Home\n\nA [missing page](/nope).\n",
    );
    let outcome = Run::new(["broken-links", "--offline"])
        .cwd(project.path())
        .output();

    assert_ne!(outcome.code, Exit::Success.code(), "{}", outcome.all());
}

#[test]
fn the_flags_are_in_the_help() {
    let outcome = Run::new(["broken-links", "--help"]).output();
    assert_eq!(outcome.code, Exit::Success.code(), "{}", outcome.all());
    for flag in ["--concurrency", "--timeout", "--allow", "--internal-only"] {
        assert!(outcome.stdout.contains(flag), "{}", outcome.stdout);
    }
}

#[test]
fn anchors_reads_every_href_and_nothing_else() {
    let html = concat!(
        r#"<a href="https://one.example/a">one</a>"#,
        r#"<abbr title="not a link">x</abbr>"#,
        r#"<a class="c" href='https://two.example/b?x=1&amp;y=2'>two</a>"#,
        r#"<img src="https://three.example/c.png">"#,
        r#"<a href=/local>three</a>"#,
    );
    assert_eq!(
        anchors(html),
        vec![
            "https://one.example/a".to_owned(),
            "https://two.example/b?x=1&y=2".to_owned(),
            "/local".to_owned(),
        ]
    );
}

/// Truncated HTML terminates the scan rather than looping. The href of a tag
/// that never closes is still a link the page meant to make, so it is read.
#[test]
fn anchors_survives_a_tag_that_never_closes() {
    assert_eq!(
        anchors("<a href=\"https://one.example/\""),
        vec!["https://one.example/".to_owned()]
    );
    assert!(anchors("").is_empty());
    assert!(anchors("<a").is_empty());
    assert!(anchors("<a href=").is_empty());
}

/// `--concurrency` promises several requests in flight; the driver that makes
/// that true has to give the answers back in the order it was asked.
#[test]
fn bounded_keeps_the_order_of_its_tasks() {
    let runtime = tokio::runtime::Builder::new_current_thread()
        .enable_all()
        .build()
        .expect("a runtime");
    let out = runtime.block_on(async {
        let tasks: Vec<_> = (0..8u32)
            .map(|n| async move {
                tokio::time::sleep(std::time::Duration::from_millis(u64::from(8 - n))).await;
                n
            })
            .collect();
        liyasa_cli::links::bounded(tasks, 3).await
    });
    assert_eq!(out, (0..8u32).collect::<Vec<_>>());
}
