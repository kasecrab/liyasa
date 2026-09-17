//! ANA-71's filter contract, against the fixture the dashboard reads.
//!
//! `web/dashboard/test/filters.fixture.json` is the shared document.
//! `web/dashboard/test/filters.test.ts` holds the TypeScript encoder and
//! parser to the same cases. Neither side asserts against the other's output;
//! both assert against the fixture, which is what makes this a contract rather
//! than two copies of one implementation agreeing with itself.

use liyasa_analytics::query::{Filters, QUERY_NAMES, Range, RangeSpec, SavedView};
use liyasa_analytics::{CallerKind, Comparison, Grain};
use serde_json::Value;

use crate::support::{DAY, HOUR, T0};

fn fixture() -> Value {
    let text = include_str!(concat!(
        env!("CARGO_MANIFEST_DIR"),
        "/../../web/dashboard/test/filters.fixture.json"
    ));
    serde_json::from_str(text).expect("the filter fixture parses")
}

#[test]
fn the_fixture_and_this_crate_agree_on_the_names() {
    let names: Vec<String> =
        serde_json::from_value(fixture()["names"].clone()).expect("the name list");
    assert_eq!(names, QUERY_NAMES);
}

#[test]
fn every_fixture_case_encodes_and_parses_back() {
    for case in fixture()["cases"].as_array().expect("cases") {
        let why = case["why"].as_str().unwrap_or_default();
        let filters: Filters = serde_json::from_value(case["filters"].clone())
            .unwrap_or_else(|e| panic!("{why}: the fixture's filters do not deserialise: {e}"));
        let expected = case["query"].as_str().expect("a query string");
        assert_eq!(filters.to_query(), expected, "{why}");
        assert_eq!(
            Filters::from_query(expected),
            filters,
            "{why}: parsing the fixture's query string"
        );
    }
}

#[test]
fn the_lenient_cases_are_lenient_in_the_same_way() {
    for case in fixture()["lenient"].as_array().expect("lenient cases") {
        let why = case["why"].as_str().unwrap_or_default();
        let query = case["query"].as_str().expect("a query string");
        let expected: Filters =
            serde_json::from_value(case["filters"].clone()).expect("the expected filters");
        assert_eq!(Filters::from_query(query), expected, "{why}");
    }
}

#[test]
fn a_leading_question_mark_is_not_part_of_a_name() {
    assert_eq!(
        Filters::from_query("?version=v2"),
        Filters {
            version: Some("v2".to_owned()),
            ..Filters::default()
        }
    );
}

#[test]
fn a_range_is_half_open_and_its_previous_period_abuts_it() {
    let range = Range::new(T0, T0 + 7 * DAY);
    assert_eq!(range.span(), 7 * DAY);
    assert!(range.contains(T0));
    assert!(!range.contains(T0 + 7 * DAY), "the end belongs to the next");
    let previous = range.previous();
    assert_eq!(previous.to, range.from);
    assert_eq!(previous.span(), range.span());
    // Reversed bounds are the same range, not an empty one.
    assert_eq!(Range::new(T0 + DAY, T0), Range::new(T0, T0 + DAY));
}

#[test]
fn last_days_means_whole_utc_days() {
    // Mid-afternoon on the 14th; "last 7 days" ends at midnight on the 15th.
    let now = T0 + 14 * HOUR + 22 * 60 * 1000;
    let range = Range::last_days(now, 7);
    assert_eq!(range.to, T0 + DAY);
    assert_eq!(range.from, T0 - 6 * DAY);
    assert_eq!(range.span(), 7 * DAY);
    assert!(range.contains(now));
    // Zero days is a day, not an empty window.
    assert_eq!(Range::last_days(now, 0).span(), DAY);
}

#[test]
fn a_range_has_a_bucket_for_every_step_it_covers() {
    let range = Range::new(T0, T0 + 3 * HOUR);
    assert_eq!(
        range.buckets(Grain::Hour),
        vec![T0, T0 + HOUR, T0 + 2 * HOUR]
    );
    assert_eq!(range.buckets(Grain::Day), vec![T0]);
    // A range that starts mid-bucket starts at the bucket's boundary, so a
    // point's label is the hour it belongs to.
    let ragged = Range::new(T0 + 90 * 60 * 1000, T0 + 3 * HOUR);
    assert_eq!(ragged.buckets(Grain::Hour), vec![T0 + HOUR, T0 + 2 * HOUR]);
}

#[test]
fn a_saved_view_that_says_last_28_days_means_the_last_28_days_whenever_it_is_opened() {
    let view = SavedView {
        id: "v1".to_owned(),
        name: "Payments, last 28 days".to_owned(),
        page: "traffic".to_owned(),
        range: RangeSpec::Last { days: 28 },
        filters: Filters {
            product: Some("payments".to_owned()),
            caller: Some(CallerKind::Agent),
            ..Filters::default()
        },
        compare: true,
        grain: None,
    };
    let today = view.resolve(T0 + 12 * HOUR);
    let next_month = view.resolve(T0 + 30 * DAY);
    assert_ne!(
        today, next_month,
        "a relative view follows the calendar; a view that stored two instants would mean one fortnight forever"
    );
    assert_eq!(today.span(), 28 * DAY);

    let pinned = SavedView {
        range: RangeSpec::Between {
            from: T0,
            to: T0 + DAY,
        },
        ..view.clone()
    };
    assert_eq!(pinned.resolve(T0), pinned.resolve(T0 + 30 * DAY));

    // The grain follows the span unless the view pins one.
    assert_eq!(view.grain_for(Range::new(T0, T0 + DAY)), Grain::Hour);
    assert_eq!(view.grain_for(Range::new(T0, T0 + 28 * DAY)), Grain::Day);
    let pinned_grain = SavedView {
        grain: Some(Grain::Hour),
        ..view.clone()
    };
    assert_eq!(
        pinned_grain.grain_for(Range::new(T0, T0 + 28 * DAY)),
        Grain::Hour
    );

    // And it round-trips as JSON, which is how it is stored.
    let json = serde_json::to_value(&view).expect("a saved view serialises");
    assert_eq!(json["range"]["kind"], "last");
    assert_eq!(json["filters"]["caller"], "agent");
    let back: SavedView = serde_json::from_value(json).expect("it round-trips");
    assert_eq!(back, view);
}

#[test]
fn a_comparison_against_nothing_has_no_percentage() {
    let grew = Comparison::new(120.0, 100.0);
    assert_eq!(grew.delta(), 20.0);
    assert_eq!(grew.change(), Some(0.2));

    let shrank = Comparison::new(80.0, 100.0);
    assert_eq!(shrank.change(), Some(-0.2));

    let from_nothing = Comparison::new(50.0, 0.0);
    assert_eq!(from_nothing.delta(), 50.0);
    assert_eq!(
        from_nothing.change(),
        None,
        "a change from nothing is not +100% and is not infinite"
    );
}

#[test]
fn the_dimensions_agg_hour_cannot_answer_are_named() {
    assert!(Filters::default().served_by_rollup());
    assert!(
        Filters {
            site: Some("acme-docs".to_owned()),
            env: Some("production".to_owned()),
            caller: Some(CallerKind::Agent),
            route_prefix: Some("/guides".to_owned()),
            ..Filters::default()
        }
        .served_by_rollup(),
        "the rollup carries site, env, route and caller kind"
    );
    for filters in [
        Filters {
            version: Some("v2".to_owned()),
            ..Filters::default()
        },
        Filters {
            locale: Some("en".to_owned()),
            ..Filters::default()
        },
        Filters {
            region: Some("us".to_owned()),
            ..Filters::default()
        },
        Filters {
            product: Some("payments".to_owned()),
            ..Filters::default()
        },
    ] {
        assert!(
            !filters.served_by_rollup(),
            "agg_hour has no variant column (RFC 1702)"
        );
    }
}
