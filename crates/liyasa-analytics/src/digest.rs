//! The weekly digest (ANA-42).
//!
//! One computed digest, three renderings: Markdown for a page, Slack blocks
//! for a webhook, and an email with a text and an HTML part. Delivery belongs
//! to `liyasa-server` — it holds the webhook signer and the only crate allowed
//! to open a socket is `liyasa-net` — so this produces the message and stops.

use liyasa_core::store::StoreError;
use serde::{Deserialize, Serialize};
use serde_json::{Value, json};

use crate::insights::{self, Card, Inputs};
use crate::query::{Comparison, DAY_MS, Filters, Range};
use crate::{schema, traffic};

/// Seven days.
pub const WEEK_MS: i64 = 7 * DAY_MS;

/// How many cards a digest carries. A weekly message that lists thirty things
/// to do is a message nobody opens.
pub const TOP_CARDS: usize = 5;

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Digest {
    pub site: String,
    /// Midnight UTC of the Monday the week starts on.
    pub week_starting: i64,
    pub views: Comparison,
    pub sessions: Comparison,
    pub agent_views: Comparison,
    pub top_pages: Vec<traffic::RouteCount>,
    pub cards: Vec<Card>,
}

/// The Monday at or before `now`, at midnight UTC.
///
/// 1970-01-01 was a Thursday, so the epoch day of a Monday is congruent to 4
/// modulo 7.
pub fn week_starting(now: i64) -> i64 {
    let day = now.div_euclid(DAY_MS);
    let monday = day - (day - 4).rem_euclid(7);
    monday * DAY_MS
}

/// Computes a week's digest.
pub async fn weekly(
    inputs: &Inputs<'_>,
    site: &str,
    week_starting: i64,
    filters: &Filters,
) -> Result<Digest, StoreError> {
    let range = Range::new(week_starting, week_starting + WEEK_MS);
    let previous = range.previous();

    let views = traffic::totals(inputs.analytics, range, filters, &["page_view"]).await?;
    let views_before = traffic::totals(inputs.analytics, previous, filters, &["page_view"]).await?;
    let sessions = traffic::unique_sessions(inputs.analytics, range, filters).await?;
    let sessions_before = traffic::unique_sessions(inputs.analytics, previous, filters).await?;
    let agent = traffic::totals(
        inputs.analytics,
        range,
        filters,
        &["markdown_fetch", "mcp_call"],
    )
    .await?;
    let agent_before = traffic::totals(
        inputs.analytics,
        previous,
        filters,
        &["markdown_fetch", "mcp_call"],
    )
    .await?;
    let top_pages =
        traffic::top_routes(inputs.analytics, range, filters, &["page_view"], 5).await?;

    let mut cards = insights::compute(inputs, range, filters, range.to).await?;
    cards.truncate(TOP_CARDS);

    Ok(Digest {
        site: site.to_owned(),
        week_starting,
        views: Comparison::new(views.total() as f64, views_before.total() as f64),
        sessions: Comparison::new(sessions.total() as f64, sessions_before.total() as f64),
        agent_views: Comparison::new(agent.total() as f64, agent_before.total() as f64),
        top_pages,
        cards,
    })
}

/// `+12%`, `-3%`, or `new` when there is nothing to compare against.
fn change(comparison: &Comparison) -> String {
    match comparison.change() {
        Some(rate) => format!("{}{:.0}%", if rate >= 0.0 { "+" } else { "" }, rate * 100.0),
        None if comparison.current > 0.0 => "new".to_owned(),
        None => "—".to_owned(),
    }
}

impl Digest {
    pub fn range(&self) -> Range {
        Range::new(self.week_starting, self.week_starting + WEEK_MS)
    }

    pub fn subject(&self) -> String {
        format!(
            "{}: week of {}",
            self.site,
            schema::format_date_ms(self.week_starting)
        )
    }

    pub fn to_markdown(&self) -> String {
        let mut out = format!("# {}\n\n", self.subject());
        out.push_str(&format!(
            "- **{:.0}** page views ({})\n- **{:.0}** sessions ({})\n- **{:.0}** agent fetches ({})\n",
            self.views.current,
            change(&self.views),
            self.sessions.current,
            change(&self.sessions),
            self.agent_views.current,
            change(&self.agent_views),
        ));
        if !self.top_pages.is_empty() {
            out.push_str("\n## Most read\n\n");
            for page in &self.top_pages {
                out.push_str(&format!(
                    "- `{}` — {} views ({} agent)\n",
                    page.route,
                    page.counts.total(),
                    page.counts.agent
                ));
            }
        }
        out.push_str("\n## What to look at\n\n");
        if self.cards.is_empty() {
            out.push_str("Nothing stood out this week.\n");
        } else {
            for card in &self.cards {
                out.push_str(&format!("- **{}** — {}", card.title, card.detail));
                if let Some(action) = &card.action {
                    out.push_str(&format!(" _{}_", action.label));
                }
                out.push('\n');
            }
        }
        out
    }

    /// Slack's Block Kit payload.
    pub fn to_slack(&self) -> Value {
        let mut blocks = vec![
            json!({
                "type": "header",
                "text": { "type": "plain_text", "text": self.subject() }
            }),
            json!({
                "type": "section",
                "fields": [
                    { "type": "mrkdwn", "text": format!("*Page views*\n{:.0} ({})", self.views.current, change(&self.views)) },
                    { "type": "mrkdwn", "text": format!("*Sessions*\n{:.0} ({})", self.sessions.current, change(&self.sessions)) },
                    { "type": "mrkdwn", "text": format!("*Agent fetches*\n{:.0} ({})", self.agent_views.current, change(&self.agent_views)) },
                ]
            }),
        ];
        for card in &self.cards {
            blocks.push(json!({
                "type": "section",
                "text": { "type": "mrkdwn", "text": format!("*{}*\n{}", card.title, card.detail) }
            }));
        }
        if self.cards.is_empty() {
            blocks.push(json!({
                "type": "section",
                "text": { "type": "mrkdwn", "text": "Nothing stood out this week." }
            }));
        }
        json!({ "blocks": blocks })
    }

    pub fn to_email(&self) -> Email {
        Email {
            subject: self.subject(),
            text: self.to_markdown(),
            html: self.to_html(),
        }
    }

    fn to_html(&self) -> String {
        let mut out = format!("<h1>{}</h1>\n<ul>", escape(&self.subject()));
        for (label, comparison) in [
            ("page views", &self.views),
            ("sessions", &self.sessions),
            ("agent fetches", &self.agent_views),
        ] {
            out.push_str(&format!(
                "<li><strong>{:.0}</strong> {label} ({})</li>",
                comparison.current,
                escape(&change(comparison))
            ));
        }
        out.push_str("</ul>");
        if !self.cards.is_empty() {
            out.push_str("<h2>What to look at</h2>\n<ul>");
            for card in &self.cards {
                out.push_str(&format!(
                    "<li><strong>{}</strong> — {}</li>",
                    escape(&card.title),
                    escape(&card.detail)
                ));
            }
            out.push_str("</ul>");
        }
        out
    }
}

/// A rendered message. `liyasa-server` sends it.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Email {
    pub subject: String,
    pub text: String,
    pub html: String,
}

/// A route or a query reaches the HTML part, and both come from a reader.
fn escape(text: &str) -> String {
    let mut out = String::with_capacity(text.len());
    for c in text.chars() {
        match c {
            '&' => out.push_str("&amp;"),
            '<' => out.push_str("&lt;"),
            '>' => out.push_str("&gt;"),
            '"' => out.push_str("&quot;"),
            '\'' => out.push_str("&#39;"),
            other => out.push(other),
        }
    }
    out
}
