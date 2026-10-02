//! The agent's research phase, served from this instance's own data (AGT-02).
//!
//! `liyasa-agent` declares sixteen tools and gates them; four are the research
//! phase, and its `write_turn` doc comment says why they are not its to run:
//! "the read tools return data this crate does not own — the search index, the
//! OpenAPI documents, the truth graph — and the caller that has them serves
//! them." This is that caller's half.
//!
//! **There is no trait to implement and that is deliberate.** `Run` already
//! exposes the seam: `enter(Phase::Research)` opens the phase, `authorise`
//! checks the tool is offered at this trust level and records a rejection
//! itself, and `record_mut().record_call(..)` audits the outcome. A second
//! trait beside `Pages` would add an indirection for nothing — the write phase
//! needs one because only the caller knows the route-to-path mapping, and
//! nothing here needs the agent to call back.
//!
//! Two of the four are served. `get_fact` and `list_drift` read
//! `liyasa-verify`'s truth graph and drift records, which are WP-13's and
//! WP-20c's data and not reachable from this crate — they answer
//! [`Unavailable`] naming the owner rather than an empty result, because an
//! empty result is indistinguishable from "nothing matched" and would read as
//! a site with no drift.

use liyasa_ai::assistant::tools::Tools;
use liyasa_ai::index::ChunkQuery;
use serde_json::{Value, json};

/// A tool this instance cannot serve, and who owns what is missing.
///
/// Not an error and not an empty result: the agent is told the difference
/// between "there is no drift" and "this server cannot see drift", which are
/// the same JSON otherwise.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Unavailable {
    pub tool: &'static str,
    pub reason: &'static str,
}

impl Unavailable {
    /// The shape the agent records, so an audit shows the refusal rather than
    /// a silent gap in the transcript.
    pub fn as_json(&self) -> Value {
        json!({ "unavailable": self.tool, "reason": self.reason })
    }
}

/// Runs one research tool call against this instance.
///
/// `name` and `input` are what `Run::authorise` has already accepted — this
/// does not re-check the trust level, because two gates disagreeing is worse
/// than one.
pub async fn execute(
    tools: &dyn Tools,
    filter: &ChunkQuery,
    name: &str,
    input: &Value,
) -> Result<Value, Unavailable> {
    match name {
        "search_docs" => {
            let query = input.get("query").and_then(Value::as_str).unwrap_or("");
            let limit = input
                .get("limit")
                .and_then(Value::as_u64)
                .unwrap_or(super::tools::RESULTS as u64) as usize;
            let hits = tools.search(query, filter).await.map_err(|_| Unavailable {
                tool: "search_docs",
                reason: "this instance's search index could not be read",
            })?;
            // The filter is the reader's, applied inside retrieval rather than
            // to the results, so a passage the caller may not see is never
            // scored and cannot leak through a snippet.
            let passages: Vec<Value> = hits
                .iter()
                .take(limit)
                .map(|hit| {
                    json!({
                        // `route#anchor` is the citation shape AST-12 uses, so
                        // an agent quoting a passage can point at it.
                        "route": hit.record.route.as_str(),
                        "anchor": hit.record.anchor,
                        "title": hit.record.title,
                        "score": hit.score,
                    })
                })
                .collect();
            Ok(json!({ "passages": passages }))
        }
        // AGT-02's research clause names four: "search docs, read pages, read
        // code in context repos, fetch allow-listed web pages". `read_page`
        // used to fall to the catch-all below and answer "not a research tool
        // this instance serves", which is wrong as a *description* rather than
        // merely incomplete — it is one of the four, and this instance serves
        // it (WP-25).
        "read_page" => {
            let Some(route) = input.get("route").and_then(Value::as_str) else {
                return Ok(json!({ "found": false, "reason": "`route` is required" }));
            };
            let section = input.get("section").and_then(Value::as_str);
            match tools
                .get_page(&liyasa_core::ids::Route::new(route), section)
                .await
            {
                Ok(Some(page)) => Ok(json!({
                    "route": page.route,
                    "title": page.title,
                    "anchor": page.anchor,
                    "markdown": page.markdown,
                })),
                // A page this reader may not see and a page that does not
                // exist answer the same way on purpose: `get_page` applies the
                // reader's filter, and saying "you may not see this" would
                // confirm the page exists (AST-11).
                Ok(None) => Ok(json!({ "found": false, "route": route })),
                Err(_) => Err(Unavailable {
                    tool: "read_page",
                    reason: "this instance's bundle could not be read",
                }),
            }
        }
        "get_openapi" => {
            let operation = input
                .get("operation")
                .or_else(|| input.get("operationId"))
                .and_then(Value::as_str)
                .unwrap_or("");
            match tools.get_openapi(operation).await {
                Ok(Some(page)) => Ok(json!({
                    "route": page.route,
                    "title": page.title,
                    "markdown": page.markdown,
                })),
                // A spec this site does not publish is a true "not found", not
                // an unavailable tool.
                Ok(None) => Ok(json!({ "found": false, "operation": operation })),
                Err(_) => Err(Unavailable {
                    tool: "get_openapi",
                    reason: "this instance's published specs could not be read",
                }),
            }
        }
        "get_fact" => Err(Unavailable {
            tool: "get_fact",
            reason: "the truth graph lives in liyasa-verify (WP-13) and this server holds no \
                     reader for it",
        }),
        "list_drift" => Err(Unavailable {
            tool: "list_drift",
            reason: "drift records live in liyasa-verify (WP-20c) and this server holds no \
                     reader for them",
        }),
        _ => Err(Unavailable {
            tool: "unknown",
            reason: "not a research tool this instance serves",
        }),
    }
}
