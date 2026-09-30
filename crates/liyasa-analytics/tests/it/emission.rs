//! Mapping a search into the event schema (ANA-01, ANA-20).
//!
//! `liyasa-search` builds a `SearchEvent` on every query and nothing consumes
//! it: `routes/search.rs` says in a comment that the mapping to `EventRecord`
//! was not invented there, and it was right not to — which field goes where is
//! this package's to decide, because this package is what reads them back.
//!
//! These assert the mapping against the queries that consume it rather than
//! against a restatement of the shape. A mapping that agreed with itself and
//! disagreed with `search.rs` would pass a shape test and report nothing.

use liyasa_analytics::props::{self, Emission};
use liyasa_analytics::query::{Filters, Range};
use liyasa_analytics::search;
use liyasa_store::records::EventRecord;

use crate::support::{DAY, T0, analytics};

fn day() -> Range {
    Range::new(T0, T0 + DAY)
}

/// The request-derived half the server fills in; none of it comes from the
/// search itself.
fn base() -> EventRecord {
    EventRecord {
        ts: T0 + 3_600_000,
        site: "acme-docs".to_owned(),
        env: "production".to_owned(),
        route: "/guides/start".to_owned(),
        format: "html".to_owned(),
        session_key: "k1:00000000000000000000000000000001".to_owned(),
        caller: serde_json::json!({ "kind": "human", "agent_name": null }),
        ..EventRecord::default()
    }
}

#[test]
fn a_query_becomes_a_search_event_the_reports_can_read() {
    let emission = props::search_event("rate limits", 4, Some("en"), &["version".to_owned()]);
    assert_eq!(emission.kind, "search");
    assert_eq!(emission.props["q"], "rate limits");
    assert_eq!(emission.props["results"], 4);
    assert_eq!(
        emission.variant["locale"], "en",
        "locale is a variant dimension (ANA-71 filters by it), not a prop"
    );
    assert_eq!(
        emission.props["filters"],
        serde_json::json!(["version"]),
        "which facets were used, by name — liyasa-search never sends the values"
    );
}

#[test]
fn a_search_that_found_nothing_records_a_count_of_zero() {
    // ANA-20's no-result list is `props.results = 0`, so this cannot be
    // expressed by omitting the field: a missing `results` is not zero to
    // `json_extract`, it is null, and the row would vanish from the report.
    let emission = props::search_no_results("quinoa", Some("de"));
    assert_eq!(emission.kind, "search");
    assert_eq!(emission.props["results"], 0);
    assert_eq!(emission.props["q"], "quinoa");
    assert_eq!(emission.variant["locale"], "de");
}

#[test]
fn a_click_records_the_target_and_a_one_based_rank() {
    let emission = props::search_click("rate limits", "/guides/limits", 1);
    assert_eq!(emission.kind, "search_click");
    assert_eq!(emission.props["q"], "rate limits");
    assert_eq!(emission.props["target"], "/guides/limits");
    assert_eq!(
        emission.props["position"], 1,
        "`SearchEvent::click` already counts from one; this must not add another"
    );
}

#[test]
fn a_search_with_no_locale_carries_no_locale_rather_than_an_empty_one() {
    let emission = props::search_event("limits", 1, None, &[]);
    assert_eq!(emission.variant["locale"], serde_json::Value::Null);
    // An absent facet list is absent, not `[]`, so the column stays small.
    assert!(emission.props.get("filters").is_none());
}

#[test]
fn an_emission_fills_only_what_the_search_knows() {
    // The request-derived fields are the caller's and must survive untouched:
    // a mapping that reset the session key or the caller kind would silently
    // break unique-session counts and the human/agent split.
    let record = props::search_event("limits", 2, Some("en"), &[]).into_record(base());
    assert_eq!(record.kind, "search");
    assert_eq!(record.site, "acme-docs");
    assert_eq!(record.env, "production");
    assert_eq!(record.route, "/guides/start");
    assert_eq!(record.session_key, "k1:00000000000000000000000000000001");
    assert_eq!(record.caller["kind"], "human");
    assert_eq!(record.ts, T0 + 3_600_000);
}

#[tokio::test]
async fn the_mapped_events_are_what_the_ana_20_reports_actually_read() {
    // The test that matters: drive the real reports over rows built by the
    // real mapping. A shape assertion alone would pass while `search.rs` read
    // a key the mapping never writes.
    let mut events = Vec::new();
    for i in 0..5u32 {
        let mut row = props::search_event("webhooks", 2, Some("en"), &[]).into_record(base());
        row.session_key = format!("k1:{i:032}");
        events.push(row);
    }
    for i in 0..3u32 {
        let mut row = props::search_no_results("sso saml", None).into_record(base());
        row.session_key = format!("k1:{:032}", 100 + i);
        events.push(row);
    }
    let mut click = props::search_click("webhooks", "/webhooks", 1).into_record(base());
    click.session_key = "k1:00000000000000000000000000000200".to_owned();
    events.push(click);

    let (_dir, writer) = analytics("emission-reports", events).await;

    let stats = search::queries(writer.pool(), day(), &Filters::default(), 10)
        .await
        .expect("queries");
    let webhooks = stats.iter().find(|s| s.q == "webhooks").expect("webhooks");
    assert_eq!(webhooks.searches, 5);
    assert_eq!(webhooks.clicks, 1);
    assert_eq!(webhooks.empty, 0);
    assert_eq!(webhooks.top_result.as_deref(), Some("/webhooks"));

    let empty = search::no_result_queries(writer.pool(), day(), &Filters::default(), 10)
        .await
        .expect("no-result queries");
    assert_eq!(empty.len(), 1, "only `sso saml` found nothing");
    assert_eq!(empty[0].q, "sso saml");
    assert_eq!(empty[0].empty, 3);
}

#[tokio::test]
async fn a_locale_from_a_search_is_filterable_as_a_variant() {
    // The reason locale goes in `variant` and not `props`: ANA-71's filters
    // read `json_extract(variant, '$.locale')`. In props it would be invisible
    // to every filter on the page.
    let mut events = Vec::new();
    for (n, locale) in [(0u32, "en"), (1, "en"), (2, "de")] {
        let mut row = props::search_event("limits", 1, Some(locale), &[]).into_record(base());
        row.session_key = format!("k1:{n:032}");
        events.push(row);
    }
    let (_dir, writer) = analytics("emission-locale", events).await;

    let german = Filters {
        locale: Some("de".to_owned()),
        ..Filters::default()
    };
    let stats = search::queries(writer.pool(), day(), &german, 10)
        .await
        .expect("queries");
    assert_eq!(stats.len(), 1);
    assert_eq!(stats[0].searches, 1, "the locale filter reached the query");

    let all = search::queries(writer.pool(), day(), &Filters::default(), 10)
        .await
        .expect("queries");
    assert_eq!(all[0].searches, 3);
}

#[test]
fn what_the_mapping_cannot_fill_is_empty_rather_than_wrong() {
    // ANA-20's per-page impressions read `props.shown`, the routes a result
    // list showed. `SearchEvent::of` takes `&[Hit]` and keeps only
    // `hits.len()`, so the URLs are gone before this mapping is reached.
    // Empty is the honest value; inventing one would make `search::per_page`
    // report impressions that never happened.
    let emission = props::search_event("webhooks", 3, None, &[]);
    assert!(
        emission.props.get("shown").is_none(),
        "no `shown` at all, rather than a fabricated list"
    );
    assert_eq!(
        Emission::SHOWN_NEEDS,
        "liyasa-search::SearchEvent::Query carries no result routes",
        "the gap is named in the code so it is findable, not folklore"
    );
}
