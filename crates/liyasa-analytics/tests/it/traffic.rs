//! ANA-10, against a real analytics database.
//!
//! Every test writes its events through `liyasa_store`'s own queue and batch
//! writer into a freshly migrated `analytics.db`, then reads them back with
//! this crate's queries. Nothing between the two is this package's model of
//! anything: the `agg_hour` rows the rollup queries read were produced by the
//! writer, not by the test.

use liyasa_analytics::CallerKind;
use liyasa_analytics::query::{Filters, Grain, Range};
use liyasa_analytics::traffic::{self, Source};

use crate::support::{DAY, Event, HOUR, T0, analytics};

fn day() -> Range {
    Range::new(T0, T0 + DAY)
}

#[tokio::test]
async fn an_hourly_series_splits_readers_from_agents() {
    let mut events = Vec::new();
    for _ in 0..3 {
        events.push(Event::new("page_view", "/a", T0 + HOUR).build());
    }
    for _ in 0..2 {
        events.push(
            Event::new("page_view", "/a", T0 + HOUR)
                .agent("claudebot")
                .build(),
        );
    }
    events.push(Event::new("page_view", "/a", T0 + 5 * HOUR).build());
    let (_dir, writer) = analytics("series-hourly", events).await;

    let series = traffic::series(
        writer.pool(),
        day(),
        Grain::Hour,
        &Filters::default(),
        &["page_view"],
    )
    .await
    .expect("a series");

    assert_eq!(series.points.len(), 24, "a point per hour of the range");
    assert_eq!(series.source, Source::Rollup);
    assert!(
        !series.sampled_by_client,
        "a page view is counted server-side"
    );

    let first = series.points[1];
    assert_eq!(first.bucket, T0 + HOUR);
    assert_eq!(first.counts.human, 3);
    assert_eq!(first.counts.agent, 2);
    assert_eq!(series.points[5].counts.human, 1);
    assert_eq!(series.total().human, 4);
    assert_eq!(series.total().agent, 2);
}

#[tokio::test]
async fn an_hour_with_nothing_in_it_is_a_zero_and_not_a_missing_point() {
    let (_dir, writer) = analytics(
        "series-gap",
        vec![Event::new("page_view", "/a", T0 + 2 * HOUR).build()],
    )
    .await;
    let series = traffic::series(
        writer.pool(),
        day(),
        Grain::Hour,
        &Filters::default(),
        &["page_view"],
    )
    .await
    .expect("a series");
    assert_eq!(series.points.len(), 24);
    assert_eq!(series.points[0].counts.total(), 0);
    assert_eq!(series.points[2].counts.total(), 1);
    assert_eq!(series.points[23].counts.total(), 0);
}

#[tokio::test]
async fn a_daily_series_folds_the_hours_of_each_day() {
    let events: Vec<_> = [0, 3, 11, DAY / HOUR + 2]
        .iter()
        .map(|h| Event::new("page_view", "/a", T0 + h * HOUR).build())
        .collect();
    let (_dir, writer) = analytics("series-daily", events).await;
    let series = traffic::series(
        writer.pool(),
        Range::new(T0, T0 + 2 * DAY),
        Grain::Day,
        &Filters::default(),
        &["page_view"],
    )
    .await
    .expect("a series");
    assert_eq!(series.points.len(), 2);
    assert_eq!(series.points[0].counts.human, 3);
    assert_eq!(series.points[1].counts.human, 1);
}

#[tokio::test]
async fn a_filter_that_matches_nothing_returns_zero_rather_than_everything() {
    let (_dir, writer) = analytics(
        "filter-empty",
        vec![Event::new("page_view", "/a", T0 + HOUR).build()],
    )
    .await;
    let filters = Filters {
        site: Some("some-other-site".to_owned()),
        ..Filters::default()
    };
    let series = traffic::series(writer.pool(), day(), Grain::Hour, &filters, &["page_view"])
        .await
        .expect("a series");
    assert_eq!(series.total().total(), 0);
    let totals = traffic::totals(writer.pool(), day(), &filters, &["page_view"])
        .await
        .expect("totals");
    assert_eq!(totals.total(), 0);
}

#[tokio::test]
async fn a_range_that_excludes_the_events_excludes_them() {
    let (_dir, writer) = analytics(
        "range-exclusive",
        vec![Event::new("page_view", "/a", T0 + HOUR).build()],
    )
    .await;
    let before = Range::new(T0 - DAY, T0);
    let totals = traffic::totals(writer.pool(), before, &Filters::default(), &["page_view"])
        .await
        .expect("totals");
    assert_eq!(totals.total(), 0);
    // The window is half-open: an event exactly at `to` belongs to the next.
    let touching = Range::new(T0 - DAY, T0 + HOUR);
    assert_eq!(
        traffic::totals(writer.pool(), touching, &Filters::default(), &["page_view"])
            .await
            .expect("totals")
            .total(),
        0
    );
}

#[tokio::test]
async fn unique_sessions_counts_sessions_and_not_events() {
    let events = vec![
        Event::new("page_view", "/a", T0 + HOUR).session(1).build(),
        Event::new("page_view", "/b", T0 + HOUR).session(1).build(),
        Event::new("page_view", "/c", T0 + HOUR).session(1).build(),
        Event::new("page_view", "/a", T0 + HOUR).session(2).build(),
        Event::new("page_view", "/a", T0 + HOUR)
            .session(3)
            .agent("gptbot")
            .build(),
    ];
    let (_dir, writer) = analytics("unique-sessions", events).await;

    let views = traffic::totals(writer.pool(), day(), &Filters::default(), &["page_view"])
        .await
        .expect("totals");
    assert_eq!(views.human, 4, "four human page views");

    let sessions = traffic::unique_sessions(writer.pool(), day(), &Filters::default())
        .await
        .expect("sessions");
    assert_eq!(sessions.human, 2, "from two human sessions");
    assert_eq!(sessions.agent, 1);
}

#[tokio::test]
async fn the_rollup_and_the_raw_table_agree_on_the_same_question() {
    // Every event carries the same variant, so a filter on it selects all of
    // them — and takes the raw path, because `agg_hour` has no variant column
    // (RFC 1702). The two numbers must match, or one of the paths is wrong.
    let variant = serde_json::json!({ "version": "v2", "locale": "en" });
    let events: Vec<_> = (0..7)
        .map(|i| {
            Event::new("page_view", "/a", T0 + i * HOUR)
                .variant(variant.clone())
                .build()
        })
        .collect();
    let (_dir, writer) = analytics("rollup-vs-raw", events).await;

    let from_rollup = traffic::series(
        writer.pool(),
        day(),
        Grain::Hour,
        &Filters::default(),
        &["page_view"],
    )
    .await
    .expect("a series");
    let filtered = Filters {
        version: Some("v2".to_owned()),
        ..Filters::default()
    };
    let from_raw = traffic::series(writer.pool(), day(), Grain::Hour, &filtered, &["page_view"])
        .await
        .expect("a series");

    assert_eq!(from_rollup.source, Source::Rollup);
    assert_eq!(from_raw.source, Source::Raw);
    assert_eq!(from_rollup.total().human, 7);
    assert_eq!(from_raw.total().human, 7);
    assert_eq!(from_rollup.points, from_raw.points);

    // And a version nobody published selects nothing.
    let absent = Filters {
        version: Some("v1".to_owned()),
        ..Filters::default()
    };
    assert_eq!(
        traffic::series(writer.pool(), day(), Grain::Hour, &absent, &["page_view"])
            .await
            .expect("a series")
            .total()
            .total(),
        0
    );
}

#[tokio::test]
async fn top_pages_rank_by_total_and_keep_the_caller_split() {
    let mut events = Vec::new();
    for _ in 0..5 {
        events.push(Event::new("page_view", "/popular", T0 + HOUR).build());
    }
    for _ in 0..4 {
        events.push(
            Event::new("page_view", "/agents-only", T0 + HOUR)
                .agent("claudebot")
                .build(),
        );
    }
    events.push(Event::new("page_view", "/quiet", T0 + HOUR).build());
    let (_dir, writer) = analytics("top-routes", events).await;

    let top = traffic::top_routes(writer.pool(), day(), &Filters::default(), &["page_view"], 2)
        .await
        .expect("top routes");
    assert_eq!(top.len(), 2, "the limit is a limit");
    assert_eq!(top[0].route, "/popular");
    assert_eq!(top[0].counts.human, 5);
    assert_eq!(top[1].route, "/agents-only");
    assert_eq!(
        top[1].counts.agent, 4,
        "a page only agents read still ranks, and says so"
    );
    assert_eq!(top[1].counts.human, 0);
}

#[tokio::test]
async fn a_route_prefix_filter_means_that_prefix_and_not_a_pattern() {
    let events = vec![
        Event::new("page_view", "/guides/start", T0 + HOUR).build(),
        Event::new("page_view", "/guides/deep/one", T0 + HOUR).build(),
        Event::new("page_view", "/reference/cli", T0 + HOUR).build(),
        // A route with a LIKE metacharacter in it. An unescaped `_` matches
        // any character, so without the escape this would pull in the others.
        Event::new("page_view", "/a_b", T0 + HOUR).build(),
        Event::new("page_view", "/axb", T0 + HOUR).build(),
    ];
    let (_dir, writer) = analytics("route-prefix", events).await;

    let guides = Filters {
        route_prefix: Some("/guides".to_owned()),
        ..Filters::default()
    };
    assert_eq!(
        traffic::totals(writer.pool(), day(), &guides, &["page_view"])
            .await
            .expect("totals")
            .total(),
        2
    );

    let literal = Filters {
        route_prefix: Some("/a_b".to_owned()),
        ..Filters::default()
    };
    assert_eq!(
        traffic::totals(writer.pool(), day(), &literal, &["page_view"])
            .await
            .expect("totals")
            .total(),
        1,
        "`_` is a literal underscore, not `match any character`"
    );
}

#[tokio::test]
async fn markdown_fetches_and_mcp_calls_are_series_of_their_own() {
    let events = vec![
        Event::new("page_view", "/a", T0 + HOUR).build(),
        Event::new("markdown_fetch", "/a.md", T0 + HOUR)
            .caller("agent")
            .format("markdown")
            .build(),
        Event::new("markdown_fetch", "/b.md", T0 + 2 * HOUR)
            .caller("agent")
            .format("markdown")
            .build(),
        Event::new("mcp_call", "/_liyasa/mcp", T0 + 2 * HOUR)
            .caller("agent")
            .format("json")
            .build(),
    ];
    let (_dir, writer) = analytics("agent-surfaces", events).await;

    let markdown = traffic::series(
        writer.pool(),
        day(),
        Grain::Hour,
        &Filters::default(),
        &["markdown_fetch"],
    )
    .await
    .expect("a series");
    assert_eq!(markdown.total().agent, 2);
    assert_eq!(markdown.total().human, 0);

    let mcp = traffic::totals(writer.pool(), day(), &Filters::default(), &["mcp_call"])
        .await
        .expect("totals");
    assert_eq!(mcp.agent, 1);

    // And a page view is not folded into either.
    let views = traffic::totals(writer.pool(), day(), &Filters::default(), &["page_view"])
        .await
        .expect("totals");
    assert_eq!(views.total(), 1);
}

#[tokio::test]
async fn referrers_are_counted_by_host() {
    let events = vec![
        Event::new("page_view", "/a", T0 + HOUR)
            .referrer("news.example.com")
            .build(),
        Event::new("page_view", "/b", T0 + HOUR)
            .referrer("news.example.com")
            .build(),
        Event::new("page_view", "/a", T0 + HOUR)
            .referrer("forum.example.org")
            .build(),
        // No referrer at all: direct traffic is not a host called "".
        Event::new("page_view", "/a", T0 + HOUR).build(),
    ];
    let (_dir, writer) = analytics("referrers", events).await;
    let hosts = traffic::referrers(writer.pool(), day(), &Filters::default(), 10)
        .await
        .expect("referrers");
    assert_eq!(hosts.len(), 2);
    assert_eq!(hosts[0].name, "news.example.com");
    assert_eq!(hosts[0].count, 2);
    assert_eq!(hosts[1].name, "forum.example.org");
}

#[tokio::test]
async fn entry_and_exit_are_the_first_and_last_page_of_a_session() {
    let events = vec![
        Event::new("page_view", "/landing", T0 + HOUR)
            .session(1)
            .build(),
        Event::new("page_view", "/middle", T0 + HOUR + 1000)
            .session(1)
            .build(),
        Event::new("page_view", "/leaving", T0 + HOUR + 2000)
            .session(1)
            .build(),
        Event::new("page_view", "/landing", T0 + 2 * HOUR)
            .session(2)
            .build(),
        Event::new("page_view", "/leaving", T0 + 2 * HOUR + 500)
            .session(2)
            .build(),
    ];
    let (_dir, writer) = analytics("entry-exit", events).await;
    let (entry, exit) = traffic::entry_and_exit(writer.pool(), day(), &Filters::default(), 10)
        .await
        .expect("entry and exit");
    assert_eq!(entry.len(), 1);
    assert_eq!(entry[0].name, "/landing");
    assert_eq!(entry[0].count, 2);
    assert_eq!(exit.len(), 1);
    assert_eq!(exit[0].name, "/leaving");
    assert_eq!(exit[0].count, 2);
}

#[tokio::test]
async fn two_views_in_the_same_millisecond_do_not_make_one_session_into_two() {
    // A redirect, a resolved prefetch, or a client clock with millisecond
    // resolution puts two page views on one instant. Without a tie-break the
    // session counts as having entered on both of them.
    let events = vec![
        Event::new("page_view", "/landing", T0 + HOUR)
            .session(1)
            .build(),
        Event::new("page_view", "/redirected", T0 + HOUR)
            .session(1)
            .build(),
        Event::new("page_view", "/leaving", T0 + HOUR + 1000)
            .session(1)
            .build(),
    ];
    let (_dir, writer) = analytics("entry-exit-ties", events).await;
    let (entry, exit) = traffic::entry_and_exit(writer.pool(), day(), &Filters::default(), 10)
        .await
        .expect("entry and exit");
    assert_eq!(
        entry.iter().map(|e| e.count).sum::<i64>(),
        1,
        "one session entered once"
    );
    assert_eq!(
        entry[0].name, "/landing",
        "the first row written wins the tie"
    );
    assert_eq!(exit.len(), 1);
    assert_eq!(exit[0].name, "/leaving");
}

#[tokio::test]
async fn a_variant_split_names_the_versions_that_were_read() {
    let events = vec![
        Event::new("page_view", "/a", T0 + HOUR)
            .variant(serde_json::json!({ "version": "v2", "locale": "en" }))
            .build(),
        Event::new("page_view", "/a", T0 + HOUR)
            .variant(serde_json::json!({ "version": "v2", "locale": "de" }))
            .build(),
        Event::new("page_view", "/a", T0 + HOUR)
            .variant(serde_json::json!({ "version": "v1", "locale": "en" }))
            .build(),
    ];
    let (_dir, writer) = analytics("by-variant", events).await;

    let versions = traffic::by_variant(
        writer.pool(),
        day(),
        &Filters::default(),
        "version",
        &["page_view"],
        10,
    )
    .await
    .expect("versions");
    assert_eq!(versions.len(), 2);
    assert_eq!(versions[0].name, "v2");
    assert_eq!(versions[0].count, 2);

    let locales = traffic::by_variant(
        writer.pool(),
        day(),
        &Filters::default(),
        "locale",
        &["page_view"],
        10,
    )
    .await
    .expect("locales");
    assert_eq!(locales.len(), 2);
    assert_eq!(locales[0].name, "en");

    // The dimension is a whitelist, so a column name cannot be smuggled in.
    assert!(
        traffic::by_variant(
            writer.pool(),
            day(),
            &Filters::default(),
            "session_key",
            &["page_view"],
            10,
        )
        .await
        .is_err()
    );
}

#[tokio::test]
async fn the_beacon_delivery_ratio_is_measured_rather_than_assumed() {
    let mut events = Vec::new();
    for i in 0..10 {
        events.push(Event::new("page_view", "/a", T0 + HOUR).session(i).build());
    }
    for i in 0..7 {
        events.push(Event::new("page_load", "/a", T0 + HOUR).session(i).build());
    }
    // An agent never runs the reader runtime; counting its views would make
    // delivery look broken.
    for _ in 0..20 {
        events.push(
            Event::new("page_view", "/a", T0 + HOUR)
                .agent("claudebot")
                .build(),
        );
    }
    let (_dir, writer) = analytics("beacon-delivery", events).await;

    let delivery = traffic::beacon_delivery(writer.pool(), day(), &Filters::default())
        .await
        .expect("delivery");
    assert_eq!(delivery.server_page_views, 10);
    assert_eq!(delivery.client_page_loads, 7);
    assert_eq!(delivery.ratio(), Some(0.7));

    // A window with no traffic has no ratio, rather than a ratio of zero.
    let empty =
        traffic::beacon_delivery(writer.pool(), Range::new(T0 - DAY, T0), &Filters::default())
            .await
            .expect("delivery");
    assert_eq!(empty.ratio(), None);
}

#[tokio::test]
async fn a_client_measured_series_says_so() {
    let (_dir, writer) = analytics(
        "sampled-label",
        vec![Event::new("scroll_depth", "/a", T0 + HOUR).build()],
    )
    .await;
    let scroll = traffic::series(
        writer.pool(),
        day(),
        Grain::Hour,
        &Filters::default(),
        &["scroll_depth"],
    )
    .await
    .expect("a series");
    assert!(
        scroll.sampled_by_client,
        "ANA-10 asks the dashboard to label this one"
    );
    assert!(traffic::is_client_measured("copy_code"));
    assert!(!traffic::is_client_measured("page_view"));
    assert!(!traffic::is_client_measured("markdown_fetch"));
}

#[tokio::test]
async fn a_caller_filter_selects_one_kind() {
    let events = vec![
        Event::new("page_view", "/a", T0 + HOUR).build(),
        Event::new("page_view", "/a", T0 + HOUR)
            .agent("claudebot")
            .build(),
        Event::new("page_view", "/a", T0 + HOUR)
            .caller("bot")
            .build(),
    ];
    let (_dir, writer) = analytics("caller-filter", events).await;
    for (kind, expected) in [
        (CallerKind::Human, 1),
        (CallerKind::Agent, 1),
        (CallerKind::Bot, 1),
        (CallerKind::Integration, 0),
    ] {
        let filters = Filters {
            caller: Some(kind),
            ..Filters::default()
        };
        assert_eq!(
            traffic::totals(writer.pool(), day(), &filters, &["page_view"])
                .await
                .expect("totals")
                .total(),
            expected,
            "for {kind:?}"
        );
    }
}

#[tokio::test]
async fn two_sites_in_one_database_do_not_mix() {
    let events = vec![
        Event::new("page_view", "/a", T0 + HOUR).build(),
        Event::new("page_view", "/a", T0 + HOUR)
            .site("other-docs")
            .build(),
    ];
    let (_dir, writer) = analytics("two-sites", events).await;
    let filters = Filters {
        site: Some("acme-docs".to_owned()),
        ..Filters::default()
    };
    assert_eq!(
        traffic::totals(writer.pool(), day(), &filters, &["page_view"])
            .await
            .expect("totals")
            .total(),
        1
    );
    assert_eq!(
        traffic::totals(writer.pool(), day(), &Filters::default(), &["page_view"])
            .await
            .expect("totals")
            .total(),
        2,
        "and unfiltered counts both"
    );
}
