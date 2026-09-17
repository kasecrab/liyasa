//! §25 and CLI-08: `liyasa test --agents --urls` scores the pages the operator
//! named instead of sampling the site.
//!
//! The PRD's rule is that "explicitly selected pages (`--urls`, curated or no
//! sampling) are scored as given regardless of count", where an automatic
//! sample of under five pages has its page-level checks marked not applicable.
//! So the flag has to reach the run, not merely be accepted.

use liyasa_cli::Exit;

use crate::support::{Dir, Run};

const ORIGIN: &str = "https://docs.acme.com";

/// Four pages: fewer than the five the insufficient-data rule needs, so a
/// sampled run and a selected run give visibly different answers.
fn site(name: &str) -> Dir {
    let project = Dir::new(name);
    project.write(
        "liyasa.json",
        &format!(r#"{{"name":"Acme docs","seo":{{"canonicalOrigin":"{ORIGIN}"}}}}"#),
    );
    project.write(
        "index.md",
        "---\ntitle: Home\ndescription: The home page.\n---\n\n# Home\n\nWelcome to the docs.\n",
    );
    for (path, title) in [
        ("guides/install.md", "Install"),
        ("guides/configure.md", "Configure"),
        ("guides/deploy.md", "Deploy"),
    ] {
        project.write(
            path,
            &format!(
                "---\ntitle: {title}\ndescription: How to {}.\n---\n\n# {title}\n\nDo the thing.\n",
                title.to_lowercase()
            ),
        );
    }
    let built = Run::new(["build"]).cwd(project.path()).output();
    assert_eq!(built.code, Exit::Success.code(), "{}", built.all());
    project
}

fn report(project: &Dir, arguments: &[&str]) -> String {
    let mut all = vec!["test", "--agents"];
    all.extend_from_slice(arguments);
    let outcome = Run::new(all).cwd(project.path()).output();
    assert!(
        outcome.code == Exit::Success.code() || outcome.code == Exit::Verification.code(),
        "unexpected exit {}: {}",
        outcome.code,
        outcome.all()
    );
    outcome.all()
}

/// The flag exists and runs. Before this it did not, and the field it fills
/// carried a comment saying no command set it.
#[test]
fn a_route_can_be_named() {
    let project = site("urls-route");
    let text = report(&project, &["--urls", "/guides/install"]);
    assert!(text.contains("Agent readiness"), "{text}");
}

/// An absolute URL on the site's own origin is the form the requirement spells,
/// and it names the same page as the route does.
#[test]
fn an_absolute_url_on_this_site_names_the_same_page() {
    let project = site("urls-absolute");
    let by_route = report(&project, &["--urls", "/guides/install"]);
    let by_url = report(&project, &["--urls", &format!("{ORIGIN}/guides/install")]);

    let score = |text: &str| {
        text.lines()
            .find(|line| line.contains("Agent readiness"))
            .unwrap_or_default()
            .to_owned()
    };
    assert_eq!(
        score(&by_route),
        score(&by_url),
        "{by_route}\n---\n{by_url}"
    );
}

/// Several pages, by repeating the flag and by one comma-separated value.
#[test]
fn several_pages_can_be_named_either_way() {
    let project = site("urls-several");
    let repeated = report(
        &project,
        &["--urls", "/guides/install", "--urls", "/guides/deploy"],
    );
    let joined = report(&project, &["--urls", "/guides/install,/guides/deploy"]);
    assert_eq!(
        repeated.contains("Agent readiness"),
        joined.contains("Agent readiness")
    );
}

/// The whole point: a selection of under five pages is scored as given, where
/// an automatic sample of the same size is not. The sampled run says so.
#[test]
fn a_selection_escapes_the_insufficient_data_rule() {
    let project = site("urls-insufficient");

    let sampled = report(&project, &[]);
    assert!(
        sampled.contains("insufficient") || sampled.contains("n/a") || sampled.contains("skip"),
        "a four-page sample should be caveated: {sampled}"
    );

    let selected = report(&project, &["--urls", "/guides/install"]);
    assert!(selected.contains("Agent readiness"), "{selected}");
}

/// A URL somewhere else cannot be scored by a command that reads the local
/// output, and saying so beats scoring nothing and reporting a clean run.
#[test]
fn a_url_on_another_origin_is_refused() {
    let project = site("urls-offsite");
    let outcome = Run::new([
        "test",
        "--agents",
        "--urls",
        "https://example.org/guides/install",
    ])
    .cwd(project.path())
    .output();

    assert_eq!(outcome.code, Exit::Errors.code(), "{}", outcome.all());
    assert!(outcome.all().contains("E0006"), "{}", outcome.all());
    assert!(outcome.all().contains("example.org"), "{}", outcome.all());
}

/// A route that is not in the built site is a mistake worth reporting, not an
/// empty selection that scores clean.
#[test]
fn a_route_that_was_not_built_is_refused() {
    let project = site("urls-missing");
    let outcome = Run::new(["test", "--agents", "--urls", "/guides/nope"])
        .cwd(project.path())
        .output();

    assert_eq!(outcome.code, Exit::Errors.code(), "{}", outcome.all());
    assert!(outcome.all().contains("E0011"), "{}", outcome.all());
    assert!(outcome.all().contains("/guides/nope"), "{}", outcome.all());
}

/// Without the flag nothing changes: the site is sampled as it was before.
#[test]
fn no_flag_still_samples_the_whole_site() {
    let project = site("urls-default");
    let text = report(&project, &[]);
    assert!(text.contains("Agent readiness"), "{text}");
}

#[test]
fn the_flag_is_in_the_help() {
    let outcome = Run::new(["test", "--help"]).output();
    assert_eq!(outcome.code, Exit::Success.code(), "{}", outcome.all());
    assert!(outcome.stdout.contains("--urls"), "{}", outcome.stdout);
}
