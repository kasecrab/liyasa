//! Insight cards (ANA-40).
//!
//! Each card is a claim with the numbers behind it and one thing to do about
//! it. A card with no action is a number on a screen, so [`Card::action`] is
//! what the dashboard renders as a button and [`action_job`] turns into a row
//! in the job table.
//!
//! Three of the eight cards need facts that are not in either database: when a
//! page was last changed, whether it has a description, and whether it has open
//! drift. Those come from the build and from the verification engine, so they
//! arrive as [`PageFacts`] rather than being invented here. A caller that
//! passes an empty slice gets no cards of those three kinds and is told so by
//! their absence, which is better than a card computed from a default.

use liyasa_core::ids::ProjectId;
use liyasa_core::store::StoreError;
use liyasa_store::db::sql_error;
use liyasa_store::jobs::Enqueue;
use serde::{Deserialize, Serialize};
use serde_json::{Value, json};
use sqlx::Row;
use sqlx::sqlite::SqlitePool;

use crate::query::{DAY_MS, Filters, Range};
use crate::sql::{bind_all, sql};
use crate::{actions, feedback, search, traffic};

/// 180 days, which ANA-40 names as the staleness threshold.
pub const STALE_DAYS: i64 = 180;

/// What the build and the verification engine know about a page. Supplied by
/// the caller; this crate reads neither.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct PageFacts {
    pub route: String,
    /// When the page's source last changed.
    pub updated_at: i64,
    pub has_description: bool,
    /// Open drift findings against the page (§21).
    pub open_drift: i64,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum CardKind {
    RisingTrafficFallingRatings,
    ReadOnlyByAgents,
    ReadOnlyByHumans,
    UnansweredDemand,
    DriftOnPopularPages,
    StalePopularPages,
    MissingDescriptions,
    Funnels,
    VersionAdoption,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ActionKind {
    /// Open a page in the dashboard.
    OpenPage,
    /// Queue the agent to write a page for a query (ANA-20).
    CreatePage,
    /// Queue the agent to fix a page (ANA-30).
    FixPage,
    /// Open the drift queue filtered to a page.
    ReviewDrift,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Action {
    pub kind: ActionKind,
    pub label: String,
    /// A route, or a query string for [`ActionKind::CreatePage`].
    pub target: String,
}

/// The job an action queues, or `None` when it only navigates.
pub fn action_job(action: &Action, project: Option<ProjectId>) -> Option<Enqueue> {
    match action.kind {
        ActionKind::CreatePage => Some(actions::create_page_for_query(&action.target, project)),
        ActionKind::FixPage => Some(actions::fix_page(&action.target, "insight", project)),
        ActionKind::OpenPage | ActionKind::ReviewDrift => None,
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Card {
    pub kind: CardKind,
    pub title: String,
    pub detail: String,
    /// The page the card is about, when it is about one.
    pub route: Option<String>,
    /// The numbers behind the claim, so the card can be audited rather than
    /// believed.
    pub metrics: Value,
    pub action: Option<Action>,
}

/// One step of a funnel (ANA-40).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub struct Funnel {
    pub from: i64,
    pub to: i64,
}

impl Funnel {
    /// `None` when nobody took the first step, rather than a rate of zero.
    pub fn rate(&self) -> Option<f64> {
        (self.from > 0).then(|| self.to as f64 / self.from as f64)
    }
}

/// Everything a pass reads.
pub struct Inputs<'a> {
    pub analytics: &'a SqlitePool,
    /// Feedback lives in the application database.
    pub app: &'a SqlitePool,
    /// From the build and the verification engine; empty is allowed.
    pub pages: &'a [PageFacts],
}

/// How much traffic a page needs before a card claims anything about it. One
/// view is not a trend, and a list of one-view pages buries the real ones.
const POPULAR: i64 = 20;

/// Computes the cards for a window (ANA-40).
pub async fn compute(
    inputs: &Inputs<'_>,
    range: Range,
    filters: &Filters,
    now: i64,
) -> Result<Vec<Card>, StoreError> {
    let mut cards = Vec::new();
    let routes = traffic::top_routes(
        inputs.analytics,
        range,
        filters,
        &["page_view", "markdown_fetch"],
        500,
    )
    .await?;
    let previous = traffic::top_routes(
        inputs.analytics,
        range.previous(),
        filters,
        &["page_view", "markdown_fetch"],
        500,
    )
    .await?;
    let ratings = feedback::by_page(inputs.app, range, 500).await?;
    let ratings_before = feedback::by_page(inputs.app, range.previous(), 500).await?;

    cards.extend(rising_traffic_falling_ratings(
        &routes,
        &previous,
        &ratings,
        &ratings_before,
    ));
    cards.extend(read_by_one_side_only(&routes));
    cards.extend(unanswered_demand(inputs.analytics, range, filters).await?);
    cards.extend(page_fact_cards(&routes, inputs.pages, now));
    if let Some(card) = funnels(inputs.analytics, range, filters).await? {
        cards.push(card);
    }
    if let Some(card) = version_adoption(inputs.analytics, range, filters).await? {
        cards.push(card);
    }
    Ok(cards)
}

/// Traffic up, ratings down: the page more people are reading and fewer of
/// them are happy with.
fn rising_traffic_falling_ratings(
    routes: &[traffic::RouteCount],
    previous: &[traffic::RouteCount],
    ratings: &[feedback::PageRating],
    ratings_before: &[feedback::PageRating],
) -> Vec<Card> {
    let mut out = Vec::new();
    for route in routes {
        let now_views = route.counts.total();
        if now_views < POPULAR {
            continue;
        }
        let then_views = previous
            .iter()
            .find(|r| r.route == route.route)
            .map_or(0, |r| r.counts.total());
        if then_views == 0 || now_views <= then_views {
            continue;
        }
        let (Some(score), Some(before)) = (
            ratings
                .iter()
                .find(|r| r.route == route.route)
                .and_then(feedback::PageRating::score),
            ratings_before
                .iter()
                .find(|r| r.route == route.route)
                .and_then(feedback::PageRating::score),
        ) else {
            // A page nobody rated in either window has no falling rating. A
            // card that treated "no votes" as zero would list every page.
            continue;
        };
        if score >= before {
            continue;
        }
        out.push(Card {
            kind: CardKind::RisingTrafficFallingRatings,
            title: format!("{} is read more and rated worse", route.route),
            detail: format!(
                "{then_views} to {now_views} views, satisfaction {:.0}% to {:.0}%",
                before * 100.0,
                score * 100.0
            ),
            route: Some(route.route.clone()),
            metrics: json!({
                "views": now_views,
                "previousViews": then_views,
                "score": score,
                "previousScore": before,
            }),
            action: Some(Action {
                kind: ActionKind::FixPage,
                label: "Ask the agent to fix it".to_owned(),
                target: route.route.clone(),
            }),
        });
    }
    out.sort_by(|a, b| {
        b.metrics["views"]
            .as_i64()
            .cmp(&a.metrics["views"].as_i64())
    });
    out.truncate(5);
    out
}

/// Pages one kind of caller reads and the other does not (ANA-40).
fn read_by_one_side_only(routes: &[traffic::RouteCount]) -> Vec<Card> {
    let mut out = Vec::new();
    for route in routes {
        let human = route.counts.human;
        let agent = route.counts.agent;
        if agent >= POPULAR && human == 0 {
            out.push(Card {
                kind: CardKind::ReadOnlyByAgents,
                title: format!("{} is fetched by agents and read by nobody", route.route),
                detail: format!("{agent} agent fetches, no human views"),
                route: Some(route.route.clone()),
                metrics: json!({ "agent": agent, "human": human }),
                action: Some(Action {
                    kind: ActionKind::OpenPage,
                    label: "Open the page".to_owned(),
                    target: route.route.clone(),
                }),
            });
        } else if human >= POPULAR && agent == 0 {
            out.push(Card {
                kind: CardKind::ReadOnlyByHumans,
                title: format!("{} is read by people and never by an agent", route.route),
                detail: format!("{human} human views, no agent fetches"),
                route: Some(route.route.clone()),
                metrics: json!({ "agent": agent, "human": human }),
                action: Some(Action {
                    kind: ActionKind::OpenPage,
                    label: "Open the page".to_owned(),
                    target: route.route.clone(),
                }),
            });
        }
    }
    out.truncate(10);
    out
}

/// No-result searches that the assistant also could not answer (ANA-40).
///
/// The assistant half is `props.unanswered` on an `assistant_message` event
/// ([`crate::props::AssistantMessage`]). Until the assistant package emits it,
/// this card falls back to no-result searches alone and says which it used.
async fn unanswered_demand(
    pool: &SqlitePool,
    range: Range,
    filters: &Filters,
) -> Result<Vec<Card>, StoreError> {
    let empty = search::no_result_queries(pool, range, filters, 10).await?;
    let gaps = assistant_gaps(pool, range, filters).await?;
    let mut out = Vec::new();
    for stat in empty {
        let matched = gaps.iter().any(|gap| overlaps(gap, &stat.q));
        out.push(Card {
            kind: CardKind::UnansweredDemand,
            title: format!("`{}` finds nothing", stat.q),
            detail: if matched {
                format!(
                    "{} searches returned nothing, and the assistant could not answer it either",
                    stat.empty
                )
            } else {
                format!("{} searches returned nothing", stat.empty)
            },
            route: None,
            metrics: json!({
                "query": stat.q,
                "emptySearches": stat.empty,
                "searches": stat.searches,
                "assistantGap": matched,
            }),
            action: Some(Action {
                kind: ActionKind::CreatePage,
                label: "Create a page for this".to_owned(),
                target: stat.q,
            }),
        });
    }
    // The ones the assistant also failed on are the strongest signal.
    out.sort_by(|a, b| {
        b.metrics["assistantGap"]
            .as_bool()
            .cmp(&a.metrics["assistantGap"].as_bool())
            .then_with(|| {
                b.metrics["emptySearches"]
                    .as_i64()
                    .cmp(&a.metrics["emptySearches"].as_i64())
            })
    });
    out.truncate(5);
    Ok(out)
}

/// Questions the assistant reported it could not answer.
async fn assistant_gaps(
    pool: &SqlitePool,
    range: Range,
    filters: &Filters,
) -> Result<Vec<String>, StoreError> {
    let predicate = filters.predicate();
    let statement = format!(
        "SELECT DISTINCT json_extract(props, '$.topic') AS topic FROM event \
         WHERE ts >= ? AND ts < ? AND type = 'assistant_message' \
           AND json_extract(props, '$.unanswered') = 1 AND topic IS NOT NULL{} \
         LIMIT 200",
        predicate.sql
    );
    let rows = bind_all(
        sql(statement).bind(range.from).bind(range.to),
        &predicate.binds,
    )
    .fetch_all(pool)
    .await
    .map_err(sql_error)?;
    rows.iter()
        .map(|row| row.try_get("topic").map_err(sql_error))
        .collect()
}

/// Whether two short strings are about the same thing. Word overlap rather
/// than equality: "sso saml" and "saml sso setup" are the same question.
fn overlaps(a: &str, b: &str) -> bool {
    let words = |text: &str| -> Vec<String> {
        text.to_lowercase()
            .split(|c: char| !c.is_alphanumeric())
            .filter(|w| w.len() > 2)
            .map(str::to_owned)
            .collect()
    };
    let left = words(a);
    let right = words(b);
    if left.is_empty() || right.is_empty() {
        return false;
    }
    let shared = left.iter().filter(|w| right.contains(w)).count();
    shared * 2 >= left.len().min(right.len())
}

/// The three cards that need build and verification facts (ANA-40).
fn page_fact_cards(routes: &[traffic::RouteCount], pages: &[PageFacts], now: i64) -> Vec<Card> {
    let mut out = Vec::new();
    for route in routes {
        let views = route.counts.total();
        if views < POPULAR {
            continue;
        }
        let Some(facts) = pages.iter().find(|p| p.route == route.route) else {
            continue;
        };
        if facts.open_drift > 0 {
            out.push(Card {
                kind: CardKind::DriftOnPopularPages,
                title: format!("{} has open drift and {views} views", route.route),
                detail: format!(
                    "{} open finding{}",
                    facts.open_drift,
                    if facts.open_drift == 1 { "" } else { "s" }
                ),
                route: Some(route.route.clone()),
                metrics: json!({ "views": views, "openDrift": facts.open_drift }),
                action: Some(Action {
                    kind: ActionKind::ReviewDrift,
                    label: "Review the drift".to_owned(),
                    target: route.route.clone(),
                }),
            });
        }
        let age_days = (now - facts.updated_at) / DAY_MS;
        if age_days >= STALE_DAYS {
            out.push(Card {
                kind: CardKind::StalePopularPages,
                title: format!("{} has not changed in {age_days} days", route.route),
                detail: format!("{views} views over the window"),
                route: Some(route.route.clone()),
                metrics: json!({ "views": views, "ageDays": age_days }),
                action: Some(Action {
                    kind: ActionKind::FixPage,
                    label: "Ask the agent to refresh it".to_owned(),
                    target: route.route.clone(),
                }),
            });
        }
        if !facts.has_description {
            out.push(Card {
                kind: CardKind::MissingDescriptions,
                title: format!("{} has no description", route.route),
                detail: format!("{views} views, and nothing for a search result to show"),
                route: Some(route.route.clone()),
                metrics: json!({ "views": views }),
                action: Some(Action {
                    kind: ActionKind::FixPage,
                    label: "Ask the agent to write one".to_owned(),
                    target: route.route.clone(),
                }),
            });
        }
    }
    out
}

/// The three funnels ANA-40 names, counted in sessions rather than events: a
/// reader who searched twice before clicking is one journey, not two.
async fn funnels(
    pool: &SqlitePool,
    range: Range,
    filters: &Filters,
) -> Result<Option<Card>, StoreError> {
    let search_to_click = funnel(pool, range, filters, "search", "search_click").await?;
    let search_to_assistant = funnel(pool, range, filters, "search", "assistant_message").await?;
    let assistant_to_page = funnel(pool, range, filters, "assistant_message", "page_view").await?;
    if search_to_click.from == 0 && search_to_assistant.from == 0 && assistant_to_page.from == 0 {
        // Nobody took a first step. A funnel over nothing is three zeroes, and
        // three zeroes in a weekly digest read as a report rather than as the
        // absence of one.
        return Ok(None);
    }
    Ok(Some(Card {
        kind: CardKind::Funnels,
        title: "How readers move through the site".to_owned(),
        detail: match search_to_click.rate() {
            Some(rate) => format!("{:.0}% of searching sessions opened a result", rate * 100.0),
            None => "nobody searched over this window".to_owned(),
        },
        route: None,
        metrics: json!({
            "searchToClick": search_to_click,
            "searchToAssistant": search_to_assistant,
            "assistantToPage": assistant_to_page,
        }),
        action: None,
    }))
}

/// Sessions that did `first`, and how many of them then did `then`.
async fn funnel(
    pool: &SqlitePool,
    range: Range,
    filters: &Filters,
    first: &str,
    then: &str,
) -> Result<Funnel, StoreError> {
    let predicate = filters.predicate();
    let statement = format!(
        "WITH started AS (\
             SELECT session_key, MIN(ts) AS at FROM event \
             WHERE ts >= ? AND ts < ? AND type = ? AND session_key <> ''{} \
             GROUP BY session_key\
         ) \
         SELECT COUNT(*) AS n, \
                SUM(CASE WHEN EXISTS (\
                    SELECT 1 FROM event AS later \
                    WHERE later.session_key = started.session_key \
                      AND later.type = ? AND later.ts >= started.at AND later.ts < ?\
                ) THEN 1 ELSE 0 END) AS followed \
         FROM started",
        predicate.sql
    );
    let row = bind_all(
        sql(statement)
            .bind(range.from)
            .bind(range.to)
            .bind(first.to_owned()),
        &predicate.binds,
    )
    .bind(then.to_owned())
    .bind(range.to)
    .fetch_one(pool)
    .await
    .map_err(sql_error)?;
    Ok(Funnel {
        from: row.try_get("n").map_err(sql_error)?,
        to: row
            .try_get::<Option<i64>, _>("followed")
            .map_err(sql_error)?
            .unwrap_or(0),
    })
}

/// Which documentation version readers are actually on (ANA-40).
async fn version_adoption(
    pool: &SqlitePool,
    range: Range,
    filters: &Filters,
) -> Result<Option<Card>, StoreError> {
    let versions = traffic::by_variant(
        pool,
        range,
        filters,
        "version",
        &["page_view", "markdown_fetch"],
        20,
    )
    .await?;
    if versions.len() < 2 {
        // One version, or none: there is no adoption question to answer.
        return Ok(None);
    }
    let total: i64 = versions.iter().map(|v| v.count).sum();
    let leader = &versions[0];
    Ok(Some(Card {
        kind: CardKind::VersionAdoption,
        title: "Version adoption".to_owned(),
        detail: format!(
            "{:.0}% of reads are on {}",
            leader.count as f64 / total as f64 * 100.0,
            leader.name
        ),
        route: None,
        metrics: json!({
            "total": total,
            "versions": versions
                .iter()
                .map(|v| json!({ "version": v.name, "reads": v.count }))
                .collect::<Vec<_>>(),
        }),
        action: None,
    }))
}
