//! ANA-20, against a real analytics database.

use liyasa_analytics::props;
use liyasa_analytics::query::{Filters, Range};
use liyasa_analytics::search;

use crate::support::{DAY, Event, HOUR, T0, analytics};

fn day() -> Range {
    Range::new(T0, T0 + DAY)
}

fn searched(q: &str, results: u32, shown: &[&str], at: i64) -> liyasa_store::records::EventRecord {
    Event::new("search", "/?q=x", at)
        .props(
            serde_json::to_value(props::Search {
                q: q.to_owned(),
                results,
                shown: shown.iter().map(|s| (*s).to_owned()).collect(),
            })
            .expect("search props"),
        )
        .build()
}

fn clicked(q: &str, target: &str, position: u32, at: i64) -> liyasa_store::records::EventRecord {
    Event::new("search_click", "/?q=x", at)
        .props(
            serde_json::to_value(props::SearchClick {
                q: q.to_owned(),
                target: target.to_owned(),
                position,
            })
            .expect("click props"),
        )
        .build()
}

#[tokio::test]
async fn queries_are_counted_and_their_click_through_measured() {
    let mut events = Vec::new();
    for _ in 0..10 {
        events.push(searched(
            "webhooks",
            4,
            &["/webhooks", "/events"],
            T0 + HOUR,
        ));
    }
    for _ in 0..4 {
        events.push(clicked("webhooks", "/webhooks", 1, T0 + HOUR));
    }
    events.push(clicked("webhooks", "/events", 2, T0 + HOUR));
    for _ in 0..3 {
        events.push(searched("billing", 2, &["/billing"], T0 + HOUR));
    }
    let (_dir, writer) = analytics("search-queries", events).await;

    let stats = search::queries(writer.pool(), day(), &Filters::default(), 10)
        .await
        .expect("queries");
    assert_eq!(stats.len(), 2);
    assert_eq!(stats[0].q, "webhooks");
    assert_eq!(stats[0].searches, 10);
    assert_eq!(stats[0].clicks, 5);
    assert_eq!(stats[0].click_through(), Some(0.5));
    assert_eq!(stats[0].top_result.as_deref(), Some("/webhooks"));
    assert_eq!(stats[0].top_result_clicks, 4);

    assert_eq!(stats[1].q, "billing");
    assert_eq!(stats[1].clicks, 0);
    assert_eq!(
        stats[1].click_through(),
        Some(0.0),
        "searched and never clicked is a rate of zero, not an absent rate"
    );
}

#[tokio::test]
async fn a_query_nobody_searched_has_no_rate_at_all() {
    let (_dir, writer) = analytics("search-empty", Vec::new()).await;
    let stats = search::queries(writer.pool(), day(), &Filters::default(), 10)
        .await
        .expect("queries");
    assert!(stats.is_empty());
    assert_eq!(search::QueryStat::default().click_through(), None);
}

#[tokio::test]
async fn no_result_queries_are_the_ones_that_returned_nothing() {
    let events = vec![
        searched("sso saml", 0, &[], T0 + HOUR),
        searched("sso saml", 0, &[], T0 + 2 * HOUR),
        searched("webhooks", 4, &["/webhooks"], T0 + HOUR),
        // A query that sometimes finds something and sometimes does not is
        // still on the list, for the searches that found nothing.
        searched("terraform", 0, &[], T0 + HOUR),
        searched("terraform", 2, &["/terraform"], T0 + 2 * HOUR),
    ];
    let (_dir, writer) = analytics("search-no-result", events).await;

    let empty = search::no_result_queries(writer.pool(), day(), &Filters::default(), 10)
        .await
        .expect("no-result queries");
    assert_eq!(empty.len(), 2);
    assert_eq!(empty[0].q, "sso saml");
    assert_eq!(empty[0].empty, 2);
    assert_eq!(empty[1].q, "terraform");
    assert_eq!(empty[1].empty, 1);
    assert_eq!(empty[1].searches, 2);
    assert!(
        !empty.iter().any(|s| s.q == "webhooks"),
        "a query that always found something is not a no-result query"
    );
}

#[tokio::test]
async fn low_click_queries_need_enough_searches_to_mean_something() {
    let mut events = Vec::new();
    // Searched a lot, clicked once: results exist and none is the answer.
    for _ in 0..20 {
        events.push(searched("rate limits", 6, &["/limits"], T0 + HOUR));
    }
    events.push(clicked("rate limits", "/limits", 1, T0 + HOUR));
    // Searched once, never clicked. A rate over one observation is noise.
    events.push(searched("typo qqq", 3, &["/a"], T0 + HOUR));
    // Searched a lot and clicked a lot.
    for _ in 0..20 {
        events.push(searched("webhooks", 4, &["/webhooks"], T0 + HOUR));
        events.push(clicked("webhooks", "/webhooks", 1, T0 + HOUR));
    }
    // Searched a lot and found nothing: a different problem, a different list.
    for _ in 0..20 {
        events.push(searched("sso saml", 0, &[], T0 + HOUR));
    }
    let (_dir, writer) = analytics("search-low-click", events).await;

    let low = search::low_click_queries(writer.pool(), day(), &Filters::default(), 5, 0.2, 10)
        .await
        .expect("low-click queries");
    let names: Vec<&str> = low.iter().map(|s| s.q.as_str()).collect();
    assert_eq!(names, ["rate limits"]);
}

#[tokio::test]
async fn a_page_has_impressions_and_clicks_of_its_own() {
    let events = vec![
        searched("webhooks", 3, &["/webhooks", "/events", "/api"], T0 + HOUR),
        searched("events", 2, &["/events", "/webhooks"], T0 + HOUR),
        clicked("webhooks", "/webhooks", 1, T0 + HOUR),
        clicked("events", "/events", 1, T0 + HOUR),
        clicked("events", "/events", 1, T0 + 2 * HOUR),
    ];
    let (_dir, writer) = analytics("search-per-page", events).await;

    let pages = search::per_page(writer.pool(), day(), &Filters::default(), 10)
        .await
        .expect("per page");
    let webhooks = pages
        .iter()
        .find(|p| p.route == "/webhooks")
        .expect("/webhooks");
    assert_eq!(webhooks.impressions, 2, "shown by both searches");
    assert_eq!(webhooks.clicks, 1);
    assert_eq!(webhooks.click_through(), Some(0.5));

    let events_page = pages
        .iter()
        .find(|p| p.route == "/events")
        .expect("/events");
    assert_eq!(events_page.impressions, 2);
    assert_eq!(events_page.clicks, 2);

    let api = pages.iter().find(|p| p.route == "/api").expect("/api");
    assert_eq!(api.impressions, 1);
    assert_eq!(
        api.clicks, 0,
        "shown once, never opened, which is the point of the report"
    );
    assert_eq!(api.click_through(), Some(0.0));
}

#[tokio::test]
async fn trending_compares_the_window_with_the_one_before_it() {
    let mut events = Vec::new();
    let previous = T0 - DAY + HOUR;
    // Rising: 2 yesterday, 9 today.
    for _ in 0..2 {
        events.push(searched("mcp", 1, &["/mcp"], previous));
    }
    for _ in 0..9 {
        events.push(searched("mcp", 1, &["/mcp"], T0 + HOUR));
    }
    // Flat.
    for at in [previous, T0 + HOUR] {
        for _ in 0..5 {
            events.push(searched("billing", 1, &["/billing"], at));
        }
    }
    // Falling.
    for _ in 0..8 {
        events.push(searched("legacy", 1, &["/legacy"], previous));
    }
    events.push(searched("legacy", 1, &["/legacy"], T0 + HOUR));
    // Brand new.
    for _ in 0..3 {
        events.push(searched("webhooks", 1, &["/webhooks"], T0 + HOUR));
    }
    let (_dir, writer) = analytics("search-trending", events).await;

    let trends = search::trending(writer.pool(), day(), &Filters::default(), 10)
        .await
        .expect("trends");
    let names: Vec<&str> = trends.iter().map(|t| t.q.as_str()).collect();
    assert_eq!(
        names,
        ["mcp", "webhooks"],
        "flat and falling are not trends"
    );

    let mcp = &trends[0];
    assert_eq!(mcp.searches, 9);
    assert_eq!(mcp.previous, 2);
    assert_eq!(mcp.delta(), 7);
    assert_eq!(mcp.change(), Some(3.5));
    assert!(!mcp.is_new());

    let webhooks = &trends[1];
    assert!(webhooks.is_new());
    assert_eq!(
        webhooks.change(),
        None,
        "a first appearance has no rate of change"
    );
}

#[tokio::test]
async fn a_site_filter_reaches_the_search_reports_too() {
    let events = vec![
        searched("webhooks", 1, &["/webhooks"], T0 + HOUR),
        Event::new("search", "/?q=x", T0 + HOUR)
            .site("other-docs")
            .props(serde_json::json!({ "q": "webhooks", "results": 1 }))
            .build(),
    ];
    let (_dir, writer) = analytics("search-site-filter", events).await;
    let filters = Filters {
        site: Some("acme-docs".to_owned()),
        ..Filters::default()
    };
    let stats = search::queries(writer.pool(), day(), &filters, 10)
        .await
        .expect("queries");
    assert_eq!(stats.len(), 1);
    assert_eq!(stats[0].searches, 1);
    assert_eq!(
        search::queries(writer.pool(), day(), &Filters::default(), 10)
            .await
            .expect("queries")[0]
            .searches,
        2,
        "and unfiltered counts both sites"
    );
}
