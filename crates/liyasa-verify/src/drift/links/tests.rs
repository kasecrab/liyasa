use std::time::{Duration, SystemTime};

use liyasa_core::ids::Route;

use crate::core::config::LinksConfig;
use crate::core::duration::DurationSetting;
use crate::core::links::{FailingSince, LinkOutcome, LinkStatus};
use crate::drift::record::{DriftKey, DriftKind};

use super::{FailingLink, candidates, coverage};

fn at(seconds: u64) -> SystemTime {
    SystemTime::UNIX_EPOCH + Duration::from_secs(seconds)
}

fn config(grace_hours: u64) -> LinksConfig {
    LinksConfig {
        grace: DurationSetting::hours(grace_hours),
        ..LinksConfig::default()
    }
}

fn link(url: &str, outcome: LinkOutcome, since: u64) -> FailingLink {
    FailingLink {
        status: LinkStatus {
            url: url.to_owned(),
            outcome,
            attempts: Vec::new(),
        },
        since: FailingSince(at(since)),
        pages: vec![Route::new("/install")],
    }
}

fn broken(status: u16) -> LinkOutcome {
    LinkOutcome::Broken {
        status: Some(status),
        reason: "not found".to_owned(),
    }
}

#[test]
fn a_link_inside_its_grace_period_is_a_build_warning_and_not_yet_a_record() {
    let config = config(72);
    let fresh = [link("https://example.com/gone", broken(404), 0)];
    assert!(candidates(&fresh, &config, at(3600)).is_empty());

    // Exactly at the grace period it is drift: `is_drift` is `>=`.
    let found = candidates(&fresh, &config, at(72 * 3600));
    assert_eq!(found.len(), 1);
    assert_eq!(
        found[0].key(),
        DriftKey::Link("https://example.com/gone".to_owned())
    );
    assert_eq!(found[0].pages, vec![Route::new("/install")]);
    assert!(matches!(
        &found[0].kind,
        DriftKind::Link { reason, failing_since, .. }
            if reason.contains("404") && *failing_since == at(0)
    ));
}

#[test]
fn a_link_that_works_is_not_a_record_however_old_the_date_beside_it_is() {
    let config = config(1);
    let working = [
        link("https://example.com/ok", LinkOutcome::Ok { status: 200 }, 0),
        link(
            "https://example.com/moved",
            LinkOutcome::Redirected {
                status: 301,
                final_url: "https://example.com/new".to_owned(),
            },
            0,
        ),
        link(
            "https://example.com/skipped",
            LinkOutcome::Skipped {
                reason: "allow list".to_owned(),
            },
            0,
        ),
    ];
    assert!(candidates(&working, &config, at(1_000_000)).is_empty());
}

#[test]
fn a_broken_link_with_no_status_still_says_why() {
    let config = config(0);
    let refused = [link(
        "https://example.com/down",
        LinkOutcome::Broken {
            status: None,
            reason: "connection refused".to_owned(),
        },
        0,
    )];
    let found = candidates(&refused, &config, at(1));
    assert!(matches!(
        &found[0].kind,
        DriftKind::Link { reason, .. } if reason == "connection refused"
    ));
}

#[test]
fn coverage_is_every_link_the_sweep_requested_so_one_that_came_back_can_close() {
    let swept = [
        link("https://example.com/gone", broken(404), 0),
        link("https://example.com/ok", LinkOutcome::Ok { status: 200 }, 0),
    ];
    let covered = coverage(&swept);
    assert_eq!(covered.len(), 2);
    assert!(covered.contains(&DriftKey::Link("https://example.com/ok".to_owned())));
}
