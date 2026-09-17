//! ANA-40 and ANA-42, against real databases.

use liyasa_analytics::digest;
use liyasa_analytics::insights::{self, ActionKind, CardKind, Inputs, PageFacts};
use liyasa_analytics::query::{Filters, Range};
use liyasa_analytics::schema;
use liyasa_store::records::{FeedbackKind, FeedbackRecord, FeedbackStatus};
use liyasa_store::repos::Feedback;

use crate::support::{DAY, Event, HOUR, T0, analytics, app};

fn week() -> Range {
    Range::new(T0, T0 + 7 * DAY)
}

fn rating(id: &str, route: &str, up: bool, at: i64) -> FeedbackRecord {
    FeedbackRecord {
        id: id.to_owned(),
        project: None,
        route: route.to_owned(),
        kind: FeedbackKind::Page,
        rating: Some(if up { 1 } else { -1 }),
        category: None,
        text: None,
        block_id: None,
        task: None,
        status: FeedbackStatus::Open,
        notes: String::new(),
        created_at: at,
        updated_at: at,
    }
}

fn views(route: &str, n: i64, at: i64) -> Vec<liyasa_store::records::EventRecord> {
    (0..n)
        .map(|i| Event::new("page_view", route, at).session(i as u32).build())
        .collect()
}

#[tokio::test]
async fn a_page_read_more_and_rated_worse_gets_a_card_with_an_action() {
    let mut events = views("/payments", 40, T0 + HOUR);
    events.extend(views("/payments", 10, T0 - 6 * DAY));
    // A page whose traffic rose but whose rating held: not a card.
    events.extend(views("/steady", 40, T0 + HOUR));
    events.extend(views("/steady", 10, T0 - 6 * DAY));
    let (_adir, writer) = analytics("insight-falling", events).await;

    let (_ddir, pool) = app("insight-falling-app").await;
    let repo = Feedback::new(pool.clone());
    for (i, up) in [true, true, true, true].iter().enumerate() {
        repo.insert(&rating(&format!("b{i}"), "/payments", *up, T0 - 6 * DAY))
            .await
            .expect("a row");
        repo.insert(&rating(&format!("sb{i}"), "/steady", *up, T0 - 6 * DAY))
            .await
            .expect("a row");
    }
    for (i, up) in [true, false, false, false].iter().enumerate() {
        repo.insert(&rating(&format!("n{i}"), "/payments", *up, T0 + HOUR))
            .await
            .expect("a row");
    }
    for (i, up) in [true, true, true, true].iter().enumerate() {
        repo.insert(&rating(&format!("sn{i}"), "/steady", *up, T0 + HOUR))
            .await
            .expect("a row");
    }

    let inputs = Inputs {
        analytics: writer.pool(),
        app: &pool,
        pages: &[],
    };
    let cards = insights::compute(&inputs, week(), &Filters::default(), T0 + 7 * DAY)
        .await
        .expect("cards");

    let falling: Vec<_> = cards
        .iter()
        .filter(|c| c.kind == CardKind::RisingTrafficFallingRatings)
        .collect();
    assert_eq!(falling.len(), 1, "only the page that actually fell");
    assert_eq!(falling[0].route.as_deref(), Some("/payments"));
    assert_eq!(falling[0].metrics["views"], 40);
    assert_eq!(falling[0].metrics["previousViews"], 10);
    assert_eq!(falling[0].metrics["score"], 0.25);
    assert_eq!(falling[0].metrics["previousScore"], 1.0);
    let action = falling[0].action.as_ref().expect("an action");
    assert_eq!(action.kind, ActionKind::FixPage);
    assert_eq!(action.target, "/payments");
    assert!(insights::action_job(action, None).is_some());
}

#[tokio::test]
async fn a_page_nobody_rated_is_not_a_falling_rating() {
    let mut events = views("/unrated", 40, T0 + HOUR);
    events.extend(views("/unrated", 10, T0 - 6 * DAY));
    let (_adir, writer) = analytics("insight-unrated", events).await;
    let (_ddir, pool) = app("insight-unrated-app").await;
    let inputs = Inputs {
        analytics: writer.pool(),
        app: &pool,
        pages: &[],
    };
    let cards = insights::compute(&inputs, week(), &Filters::default(), T0 + 7 * DAY)
        .await
        .expect("cards");
    assert!(
        !cards
            .iter()
            .any(|c| c.kind == CardKind::RisingTrafficFallingRatings),
        "no votes is not a falling score, or every page would be on this list"
    );
}

#[tokio::test]
async fn pages_only_one_side_reads_are_named_in_both_directions() {
    let mut events = Vec::new();
    for i in 0..30 {
        events.push(
            Event::new("markdown_fetch", "/api/reference.md", T0 + HOUR)
                .agent("claudebot")
                .session(i)
                .build(),
        );
    }
    events.extend(views("/getting-started", 30, T0 + HOUR));
    // A page both read: not a card either way.
    events.extend(views("/both", 25, T0 + HOUR));
    for i in 0..25 {
        events.push(
            Event::new("markdown_fetch", "/both", T0 + HOUR)
                .agent("gptbot")
                .session(100 + i)
                .build(),
        );
    }
    let (_adir, writer) = analytics("insight-one-sided", events).await;
    let (_ddir, pool) = app("insight-one-sided-app").await;

    let inputs = Inputs {
        analytics: writer.pool(),
        app: &pool,
        pages: &[],
    };
    let cards = insights::compute(&inputs, week(), &Filters::default(), T0 + 7 * DAY)
        .await
        .expect("cards");

    let agents_only: Vec<_> = cards
        .iter()
        .filter(|c| c.kind == CardKind::ReadOnlyByAgents)
        .map(|c| c.route.as_deref().unwrap_or_default())
        .collect();
    assert_eq!(agents_only, ["/api/reference.md"]);

    let humans_only: Vec<_> = cards
        .iter()
        .filter(|c| c.kind == CardKind::ReadOnlyByHumans)
        .map(|c| c.route.as_deref().unwrap_or_default())
        .collect();
    assert_eq!(humans_only, ["/getting-started"]);
}

#[tokio::test]
async fn a_no_result_search_becomes_a_card_that_queues_the_agent() {
    let mut events = Vec::new();
    for i in 0..6 {
        events.push(
            Event::new("search", "/?q=x", T0 + HOUR)
                .session(i)
                .props(serde_json::json!({ "q": "sso saml", "results": 0 }))
                .build(),
        );
    }
    // The assistant also failed on the same topic.
    events.push(
        Event::new("assistant_message", "/", T0 + HOUR)
            .props(serde_json::json!({ "topic": "saml sso setup", "unanswered": true }))
            .build(),
    );
    for i in 0..3 {
        events.push(
            Event::new("search", "/?q=y", T0 + HOUR)
                .session(50 + i)
                .props(serde_json::json!({ "q": "terraform", "results": 0 }))
                .build(),
        );
    }
    let (_adir, writer) = analytics("insight-demand", events).await;
    let (_ddir, pool) = app("insight-demand-app").await;

    let inputs = Inputs {
        analytics: writer.pool(),
        app: &pool,
        pages: &[],
    };
    let cards = insights::compute(&inputs, week(), &Filters::default(), T0 + 7 * DAY)
        .await
        .expect("cards");
    let demand: Vec<_> = cards
        .iter()
        .filter(|c| c.kind == CardKind::UnansweredDemand)
        .collect();
    assert_eq!(demand.len(), 2);
    assert_eq!(
        demand[0].metrics["query"], "sso saml",
        "the one the assistant also failed on comes first"
    );
    assert_eq!(demand[0].metrics["assistantGap"], true);
    assert_eq!(demand[1].metrics["assistantGap"], false);

    let action = demand[0].action.as_ref().expect("an action");
    assert_eq!(action.kind, ActionKind::CreatePage);
    let job = insights::action_job(action, None).expect("a job");
    assert_eq!(job.name, liyasa_analytics::actions::CREATE_PAGE);
    assert_eq!(job.payload["query"], "sso saml");
}

#[tokio::test]
async fn the_three_cards_that_need_build_facts_appear_only_when_they_are_given() {
    let events = views("/old-popular", 40, T0 + HOUR);
    let (_adir, writer) = analytics("insight-facts", events).await;
    let (_ddir, pool) = app("insight-facts-app").await;

    let without = insights::compute(
        &Inputs {
            analytics: writer.pool(),
            app: &pool,
            pages: &[],
        },
        week(),
        &Filters::default(),
        T0 + 7 * DAY,
    )
    .await
    .expect("cards");
    for kind in [
        CardKind::DriftOnPopularPages,
        CardKind::StalePopularPages,
        CardKind::MissingDescriptions,
    ] {
        assert!(
            !without.iter().any(|c| c.kind == kind),
            "{kind:?} cannot be computed from the databases alone and must not be guessed"
        );
    }

    let pages = [PageFacts {
        route: "/old-popular".to_owned(),
        updated_at: T0 - 200 * DAY,
        has_description: false,
        open_drift: 2,
    }];
    let with = insights::compute(
        &Inputs {
            analytics: writer.pool(),
            app: &pool,
            pages: &pages,
        },
        week(),
        &Filters::default(),
        T0 + 7 * DAY,
    )
    .await
    .expect("cards");

    let drift = with
        .iter()
        .find(|c| c.kind == CardKind::DriftOnPopularPages)
        .expect("a drift card");
    assert_eq!(drift.metrics["openDrift"], 2);
    assert_eq!(drift.metrics["views"], 40);
    assert_eq!(
        drift.action.as_ref().expect("an action").kind,
        ActionKind::ReviewDrift
    );

    let stale = with
        .iter()
        .find(|c| c.kind == CardKind::StalePopularPages)
        .expect("a stale card");
    assert_eq!(stale.metrics["ageDays"], 207);

    assert!(
        with.iter().any(|c| c.kind == CardKind::MissingDescriptions),
        "a popular page with no description is a card"
    );
}

#[tokio::test]
async fn a_page_that_is_stale_but_unread_is_not_a_card() {
    let events = views("/quiet", 3, T0 + HOUR);
    let (_adir, writer) = analytics("insight-quiet", events).await;
    let (_ddir, pool) = app("insight-quiet-app").await;
    let pages = [PageFacts {
        route: "/quiet".to_owned(),
        updated_at: T0 - 900 * DAY,
        has_description: false,
        open_drift: 5,
    }];
    let cards = insights::compute(
        &Inputs {
            analytics: writer.pool(),
            app: &pool,
            pages: &pages,
        },
        week(),
        &Filters::default(),
        T0 + 7 * DAY,
    )
    .await
    .expect("cards");
    assert!(
        !cards.iter().any(|c| matches!(
            c.kind,
            CardKind::DriftOnPopularPages
                | CardKind::StalePopularPages
                | CardKind::MissingDescriptions
        )),
        "ANA-40 prioritises by traffic; three views is not a priority"
    );
}

#[tokio::test]
async fn the_funnels_count_sessions_and_not_events() {
    let mut events = Vec::new();
    // Session 1 searches twice and clicks once: one journey, not two.
    events.push(
        Event::new("search", "/?q=a", T0 + HOUR)
            .session(1)
            .props(serde_json::json!({ "q": "a", "results": 2 }))
            .build(),
    );
    events.push(
        Event::new("search", "/?q=b", T0 + HOUR + 1000)
            .session(1)
            .props(serde_json::json!({ "q": "b", "results": 2 }))
            .build(),
    );
    events.push(
        Event::new("search_click", "/?q=b", T0 + HOUR + 2000)
            .session(1)
            .props(serde_json::json!({ "q": "b", "target": "/x", "position": 1 }))
            .build(),
    );
    // Session 2 searches and gives up.
    events.push(
        Event::new("search", "/?q=c", T0 + HOUR)
            .session(2)
            .props(serde_json::json!({ "q": "c", "results": 0 }))
            .build(),
    );
    // Session 3 searches, then asks the assistant, then opens a page.
    events.push(
        Event::new("search", "/?q=d", T0 + HOUR)
            .session(3)
            .props(serde_json::json!({ "q": "d", "results": 1 }))
            .build(),
    );
    events.push(
        Event::new("assistant_message", "/", T0 + HOUR + 1000)
            .session(3)
            .build(),
    );
    events.push(
        Event::new("page_view", "/answer", T0 + HOUR + 2000)
            .session(3)
            .build(),
    );
    let (_adir, writer) = analytics("insight-funnels", events).await;
    let (_ddir, pool) = app("insight-funnels-app").await;

    let cards = insights::compute(
        &Inputs {
            analytics: writer.pool(),
            app: &pool,
            pages: &[],
        },
        week(),
        &Filters::default(),
        T0 + 7 * DAY,
    )
    .await
    .expect("cards");
    let funnels = cards
        .iter()
        .find(|c| c.kind == CardKind::Funnels)
        .expect("a funnel card");
    assert_eq!(funnels.metrics["searchToClick"]["from"], 3);
    assert_eq!(funnels.metrics["searchToClick"]["to"], 1);
    assert_eq!(funnels.metrics["searchToAssistant"]["from"], 3);
    assert_eq!(funnels.metrics["searchToAssistant"]["to"], 1);
    assert_eq!(funnels.metrics["assistantToPage"]["from"], 1);
    assert_eq!(funnels.metrics["assistantToPage"]["to"], 1);
}

#[tokio::test]
async fn version_adoption_appears_only_when_there_is_more_than_one_version() {
    let one: Vec<_> = (0..30)
        .map(|i| {
            Event::new("page_view", "/a", T0 + HOUR)
                .session(i)
                .variant(serde_json::json!({ "version": "v2" }))
                .build()
        })
        .collect();
    let (_adir, writer) = analytics("insight-one-version", one.clone()).await;
    let (_ddir, pool) = app("insight-one-version-app").await;
    let cards = insights::compute(
        &Inputs {
            analytics: writer.pool(),
            app: &pool,
            pages: &[],
        },
        week(),
        &Filters::default(),
        T0 + 7 * DAY,
    )
    .await
    .expect("cards");
    assert!(!cards.iter().any(|c| c.kind == CardKind::VersionAdoption));

    let mut two = one;
    two.extend((0..10).map(|i| {
        Event::new("page_view", "/a", T0 + HOUR)
            .session(100 + i)
            .variant(serde_json::json!({ "version": "v1" }))
            .build()
    }));
    let (_adir2, writer2) = analytics("insight-two-versions", two).await;
    let (_ddir2, pool2) = app("insight-two-versions-app").await;
    let cards = insights::compute(
        &Inputs {
            analytics: writer2.pool(),
            app: &pool2,
            pages: &[],
        },
        week(),
        &Filters::default(),
        T0 + 7 * DAY,
    )
    .await
    .expect("cards");
    let adoption = cards
        .iter()
        .find(|c| c.kind == CardKind::VersionAdoption)
        .expect("an adoption card");
    assert_eq!(adoption.metrics["total"], 40);
    assert_eq!(adoption.metrics["versions"][0]["version"], "v2");
    assert_eq!(adoption.metrics["versions"][0]["reads"], 30);
    assert!(adoption.detail.contains("75%"));
}

#[test]
fn a_week_starts_on_a_monday() {
    // 2026-09-14 is a Monday.
    assert_eq!(digest::week_starting(T0), T0);
    assert_eq!(digest::week_starting(T0 + 6 * DAY + HOUR), T0);
    assert_eq!(digest::week_starting(T0 + 7 * DAY), T0 + 7 * DAY);
    assert_eq!(digest::week_starting(T0 - HOUR), T0 - 7 * DAY);
    assert_eq!(schema::format_date_ms(T0), "2026-09-14");
    assert_eq!(schema::format_date_ms(T0 - 7 * DAY), "2026-09-07");
}

#[tokio::test]
async fn the_weekly_digest_carries_the_numbers_and_the_top_cards() {
    let mut events = views("/payments", 30, T0 + HOUR);
    events.extend(views("/payments", 10, T0 - 6 * DAY));
    events.extend(views("/guides", 12, T0 + 2 * HOUR));
    for i in 0..8 {
        events.push(
            Event::new("markdown_fetch", "/payments.md", T0 + HOUR)
                .agent("claudebot")
                .session(500 + i)
                .build(),
        );
    }
    let (_adir, writer) = analytics("digest-week", events).await;
    let (_ddir, pool) = app("digest-week-app").await;

    let inputs = Inputs {
        analytics: writer.pool(),
        app: &pool,
        pages: &[],
    };
    let digest = digest::weekly(&inputs, "acme-docs", T0, &Filters::default())
        .await
        .expect("a digest");

    assert_eq!(digest.views.current, 42.0);
    assert_eq!(digest.views.previous, 10.0);
    assert_eq!(digest.agent_views.current, 8.0);
    assert_eq!(digest.top_pages[0].route, "/payments");
    assert_eq!(digest.subject(), "acme-docs: week of 2026-09-14");

    let markdown = digest.to_markdown();
    assert!(markdown.contains("# acme-docs: week of 2026-09-14"));
    assert!(markdown.contains("**42** page views (+320%)"));
    assert!(markdown.contains("`/payments`"));

    let slack = digest.to_slack();
    let blocks = slack["blocks"].as_array().expect("blocks");
    assert_eq!(blocks[0]["type"], "header");
    assert_eq!(blocks[0]["text"]["text"], "acme-docs: week of 2026-09-14");
    assert!(
        blocks[1]["fields"][0]["text"]
            .as_str()
            .expect("a field")
            .contains("42")
    );

    let email = digest.to_email();
    assert_eq!(email.subject, digest.subject());
    assert!(
        email
            .html
            .contains("<h1>acme-docs: week of 2026-09-14</h1>")
    );
    assert!(email.text.contains("42"));
}

#[tokio::test]
async fn a_quiet_week_says_so_rather_than_inventing_a_percentage() {
    let (_adir, writer) = analytics("digest-quiet", Vec::new()).await;
    let (_ddir, pool) = app("digest-quiet-app").await;
    let digest = digest::weekly(
        &Inputs {
            analytics: writer.pool(),
            app: &pool,
            pages: &[],
        },
        "acme-docs",
        T0,
        &Filters::default(),
    )
    .await
    .expect("a digest");

    assert_eq!(digest.views.current, 0.0);
    assert_eq!(digest.views.change(), None);
    let markdown = digest.to_markdown();
    assert!(markdown.contains("**0** page views (—)"));
    assert!(markdown.contains("Nothing stood out this week."));
    assert!(
        !markdown.contains("+100%"),
        "a change from nothing has no percentage"
    );
    let blocks = digest.to_slack();
    assert!(
        blocks["blocks"]
            .as_array()
            .expect("blocks")
            .iter()
            .any(|b| b["text"]["text"] == "Nothing stood out this week.")
    );
}

#[tokio::test]
async fn a_route_that_looks_like_markup_does_not_reach_the_html_as_markup() {
    let route = "/a<script>alert(1)</script>";
    let mut events = views(route, 30, T0 + HOUR);
    events.extend(views(route, 5, T0 - 6 * DAY));
    let (_adir, writer) = analytics("digest-escaping", events).await;
    let (_ddir, pool) = app("digest-escaping-app").await;
    let pages = [PageFacts {
        route: route.to_owned(),
        updated_at: T0 - 900 * DAY,
        has_description: true,
        open_drift: 0,
    }];
    let digest = digest::weekly(
        &Inputs {
            analytics: writer.pool(),
            app: &pool,
            pages: &pages,
        },
        "acme-docs",
        T0,
        &Filters::default(),
    )
    .await
    .expect("a digest");
    let html = digest.to_email().html;
    assert!(
        html.contains("&lt;script&gt;"),
        "a route comes from a request and reaches the email"
    );
    assert!(!html.contains("<script>"));
}
