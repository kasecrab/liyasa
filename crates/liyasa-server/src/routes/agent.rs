//! The agent's research phase, run by this instance (AGT-02, AGT-10).
//!
//! `routes/research.rs` dispatches the four read tools and was reachable from
//! nothing — a `pub mod` and a passing test, which is the eighth instance of
//! the defect `no_caller_ratchet`'s doc opens with. This is what reaches it.
//!
//! **It is a job rather than an HTTP handler**, because `Surface::RestJobApi`
//! is what AGT-10 calls this path and because a research pass is not a request
//! shape: it reads an index, may take seconds, and its output belongs in a run
//! record an operator can audit later. A handler would have to either block a
//! connection for it or invent a second place to put the answer.

use std::sync::Arc;

use liyasa_agent::record::Phase;
use liyasa_agent::run::{Request, Run, Surface};
use liyasa_store::records::JobRecord;
use serde_json::json;

use super::AppState;
use super::work::{JobKind, Outcome, Run as JobRun};

pub const RESEARCH_JOB: &str = "agent.research";

/// Enqueued by whoever asks for a run; the worker only executes it.
pub const RESEARCH: JobKind = JobKind::caller(RESEARCH_JOB, run_research);

/// What the payload has to carry for a research pass to mean anything.
#[derive(Debug, serde::Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
struct Payload {
    /// The question the run is about.
    task: String,
    /// The tool calls to make, in order. Each is `{ "tool": ..., "input": … }`.
    calls: Vec<Call>,
    #[serde(default)]
    run_id: Option<String>,
}

#[derive(Debug, serde::Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
struct Call {
    tool: String,
    #[serde(default)]
    input: serde_json::Value,
}

pub fn run_research<'a>(state: &'a Arc<AppState>, job: &'a JobRecord) -> JobRun<'a> {
    Box::pin(async move {
        let payload: Payload = match serde_json::from_value(job.payload.clone()) {
            Ok(payload) => payload,
            // A payload this handler cannot read is a producer defect, not a
            // missing precondition, so it fails and is recorded.
            Err(e) => return Outcome::Failed(format!("the research payload does not read: {e}")),
        };
        let Some(bundle) = state.bundle.clone() else {
            return Outcome::Skipped(
                "this instance serves no site, and every read tool reads one".to_owned(),
            );
        };

        let mut run = open_run(&payload);
        if let Err(error) = run.enter(Phase::Research) {
            return Outcome::Failed(format!("a run starts at research: {error}"));
        }

        // One reader's view. `ChunkQuery::default()` is the unauthenticated
        // reader: a research pass runs for the project rather than for a
        // person, so it sees what any visitor sees and never more.
        let filter = liyasa_ai::index::ChunkQuery::default();
        let tools = super::tools::ServerTools::new(bundle, filter.clone());

        let mut answers = Vec::new();
        for call in &payload.calls {
            // `authorise` decides and records its own rejection. `execute`
            // does not re-check it: two gates disagreeing is worse than one.
            if let Err(rejection) = run.authorise(&call.tool, &call.input) {
                answers.push(json!({ "tool": call.tool, "rejected": rejection.to_string() }));
                continue;
            }
            let answer =
                match super::research::execute(&tools, &filter, &call.tool, &call.input).await {
                    Ok(answer) => answer,
                    // Recorded, not dropped. An empty drift list and a server that
                    // cannot see drift are the same JSON, and a reviewer reading a
                    // run that concluded "no drift" has to be able to tell whether
                    // it looked (WP-25's ask).
                    Err(unavailable) => unavailable.as_json(),
                };
            run.record_result(&call.tool, &call.tool, &answer);
            answers.push(json!({ "tool": call.tool, "answer": answer }));
        }

        Outcome::Done(json!({
            "task": payload.task,
            "calls": answers,
            "phase": "research",
        }))
    })
}

/// A run for this pass.
///
/// `Surface::RestJobApi` because that is what AGT-10 calls a run started
/// through the job API, and `Trigger` carries the project's own trust: the
/// task text came from whoever enqueued the job, which on this path is the
/// REST API rather than a stranger.
fn open_run(payload: &Payload) -> Run {
    use liyasa_agent::proposal::Attribution;
    use liyasa_agent::trust::{Trigger, TriggerKind, TrustLevel};

    let id = payload
        .run_id
        .clone()
        .unwrap_or_else(|| format!("research-{}", liyasa_store::now_ms()));
    let request = Request {
        run: liyasa_agent::record::RunId(id.clone()),
        surface: Surface::RestJobApi,
        trigger: Trigger::new(TriggerKind::Prompt, TrustLevel::Member),
        task: payload.task.clone(),
        inputs: Vec::new(),
        policy: liyasa_agent::policy::Policy::default(),
        attribution: Attribution::person("the job API", "jobs@localhost"),
        content_tree: None,
        // `None`, not `Read`: `get_fact` and `list_drift` both answer
        // `Unavailable` on this instance, so a header claiming graph access
        // would overstate what the run could reach. WP-25 caught the
        // overstatement; it is the record's own claim about itself, and a
        // reviewer reads it.
        graph: liyasa_agent::record::GraphAccess::None,
        retention_days: 90,
        branch: format!("agent/{id}"),
    };
    liyasa_agent::run::start(
        request,
        liyasa_agent::config::AgentConfig::default(),
        liyasa_agent::scope::Layout::default(),
        liyasa_agent::agents_md::AgentsMd::default(),
        liyasa_agent::hosts::KnownHosts::new(Vec::<String>::new()),
    )
}
