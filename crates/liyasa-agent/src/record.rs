//! The run record (AGT-01, AGT-05).
//!
//! AGT-05 asks for "every tool call and model exchange (with secrets redacted)".
//! The redaction is not a step a caller performs before pushing: every method that
//! puts text or JSON into a record runs it through [`crate::secrets::redact`]
//! first, and the fields are private so there is no way in that skips it. A
//! scrubber a caller has to remember is a scrubber that gets forgotten once, which
//! is all it takes.
//!
//! The header is AGT-01's list: the task, the trust level, the content tree
//! snapshot, graph access, the context repositories, and the policy. It is built
//! once when the run opens and does not change, because it is what the run was
//! *allowed* to do and a record that could be widened afterwards would not be
//! evidence of anything.
//!
//! There is no clock here. `liyasa-core` performs no I/O and builds for wasm, and
//! a record that minted its own timestamps could not be replayed in a test. The
//! caller stamps entries; the record orders them by a sequence number of its own,
//! which is the ordering that matters for "what did this run do, and in what
//! order".

use std::collections::BTreeSet;

use liyasa_core::ids::Route;
use serde::{Deserialize, Serialize};
use serde_json::Value;

use crate::policy::{Decision, Policy};
use crate::trust::{Input, TriggerKind, TrustLevel, WriteScope};

/// A run's identity. Minted by the caller, like every other id in this
/// workspace.
#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
#[serde(transparent)]
pub struct RunId(pub String);

impl RunId {
    pub fn new(value: impl Into<String>) -> Self {
        Self(value.into())
    }

    pub fn as_str(&self) -> &str {
        &self.0
    }
}

impl std::fmt::Display for RunId {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(&self.0)
    }
}

/// AGT-02's phases, in order.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum Phase {
    Research,
    Plan,
    Write,
    Validate,
    Publish,
}

impl Phase {
    /// The sequence AGT-02 names.
    pub const ALL: [Phase; 5] = [
        Phase::Research,
        Phase::Plan,
        Phase::Write,
        Phase::Validate,
        Phase::Publish,
    ];

    pub const fn as_str(self) -> &'static str {
        match self {
            Phase::Research => "research",
            Phase::Plan => "plan",
            Phase::Write => "write",
            Phase::Validate => "validate",
            Phase::Publish => "publish",
        }
    }

    /// The phase after this one, or `None` after `publish`.
    pub const fn next(self) -> Option<Phase> {
        match self {
            Phase::Research => Some(Phase::Plan),
            Phase::Plan => Some(Phase::Write),
            Phase::Write => Some(Phase::Validate),
            Phase::Validate => Some(Phase::Publish),
            Phase::Publish => None,
        }
    }
}

/// How much of the truth graph a run reached (AGT-01).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum GraphAccess {
    #[default]
    None,
    Read,
    /// Read, and may propose a fact update for review.
    ReadAndPropose,
}

/// What the run was asked to do.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Task {
    pub trigger: TriggerKind,
    /// The prompt or the signal's text. Untrusted for every trigger but a
    /// prompt, and redacted on the way in like everything else.
    pub text: String,
    /// The pages the trigger named.
    pub pages: BTreeSet<Route>,
}

/// AGT-01's list, fixed when the run opens.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Header {
    pub task: Task,
    pub trust: TrustLevel,
    pub scope: WriteScope,
    pub inputs: Vec<Input>,
    /// A fingerprint of the content tree the run read, so a record can be tied
    /// to the site it was about.
    pub content_tree: Option<String>,
    pub graph: GraphAccess,
    pub context_repos: Vec<String>,
    pub policy: Policy,
    /// AGT-05's "retention per policy", in days.
    pub retention_days: u32,
}

/// What became of one tool call.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "result", rename_all = "kebab-case")]
pub enum CallOutcome {
    Ok,
    /// Refused before it ran, with the reason a reviewer reads.
    Rejected {
        reason: String,
    },
    /// Ran and failed.
    Failed {
        reason: String,
    },
}

/// One line of the record.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "kebab-case")]
pub enum Entry {
    Phase {
        seq: u32,
        phase: Phase,
    },
    ToolCall {
        seq: u32,
        name: String,
        input: Value,
        outcome: CallOutcome,
    },
    /// One request to a model and what came back.
    ///
    /// The prompt is summarised rather than stored whole: the system text is
    /// operator text and is stored, the data blocks are recorded by label and
    /// trust level rather than by content, because a record that inlined every
    /// retrieved page would be larger than the site.
    ModelExchange {
        seq: u32,
        model: String,
        system: String,
        data: Vec<BlockRef>,
        output: String,
        usage: Option<Usage>,
    },
    Note {
        seq: u32,
        text: String,
    },
    Decided {
        seq: u32,
        decision: Decision,
    },
}

impl Entry {
    pub const fn seq(&self) -> u32 {
        match self {
            Entry::Phase { seq, .. }
            | Entry::ToolCall { seq, .. }
            | Entry::ModelExchange { seq, .. }
            | Entry::Note { seq, .. }
            | Entry::Decided { seq, .. } => *seq,
        }
    }
}

/// A data block as the record names it: what it was and how far it could travel,
/// not what it said.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct BlockRef {
    pub label: String,
    pub trust: TrustLevel,
    pub bytes: u32,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub struct Usage {
    pub input: u32,
    pub output: u32,
}

/// The record of one run.
///
/// The fields are private. `entries` is append-only through the `record_*`
/// methods, each of which redacts, and the header is redacted when the record
/// opens.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct RunRecord {
    run: RunId,
    header: Header,
    entries: Vec<Entry>,
}

impl RunRecord {
    /// Opens a record. The header is redacted here, once.
    pub fn open(run: RunId, mut header: Header) -> Self {
        header.task.text = crate::secrets::redact(&header.task.text);
        for input in &mut header.inputs {
            input.label = crate::secrets::redact(&input.label);
        }
        Self {
            run,
            header,
            entries: Vec::new(),
        }
    }

    pub fn run(&self) -> &RunId {
        &self.run
    }

    pub fn header(&self) -> &Header {
        &self.header
    }

    pub fn entries(&self) -> &[Entry] {
        &self.entries
    }

    fn next_seq(&self) -> u32 {
        self.entries.len() as u32
    }

    pub fn enter(&mut self, phase: Phase) {
        let seq = self.next_seq();
        self.entries.push(Entry::Phase { seq, phase });
    }

    /// The phase the run is in, or `None` before the first.
    pub fn phase(&self) -> Option<Phase> {
        self.entries.iter().rev().find_map(|entry| match entry {
            Entry::Phase { phase, .. } => Some(*phase),
            _ => None,
        })
    }

    /// The phases the run passed through, in order.
    pub fn phases(&self) -> Vec<Phase> {
        self.entries
            .iter()
            .filter_map(|entry| match entry {
                Entry::Phase { phase, .. } => Some(*phase),
                _ => None,
            })
            .collect()
    }

    /// Records one tool call. Every call, whether it ran or was refused.
    pub fn record_call(&mut self, name: &str, input: &Value, outcome: CallOutcome) {
        let seq = self.next_seq();
        self.entries.push(Entry::ToolCall {
            seq,
            name: name.to_owned(),
            input: redact_value(input),
            outcome: match outcome {
                CallOutcome::Ok => CallOutcome::Ok,
                CallOutcome::Rejected { reason } => CallOutcome::Rejected {
                    reason: crate::secrets::redact(&reason),
                },
                CallOutcome::Failed { reason } => CallOutcome::Failed {
                    reason: crate::secrets::redact(&reason),
                },
            },
        });
    }

    /// Records one model exchange.
    pub fn record_exchange(
        &mut self,
        model: &str,
        request: &liyasa_core::ai::ChatRequest,
        output: &str,
        usage: Option<Usage>,
    ) {
        let seq = self.next_seq();
        self.entries.push(Entry::ModelExchange {
            seq,
            model: model.to_owned(),
            system: crate::secrets::redact(&request.system),
            data: request
                .data
                .iter()
                .map(|block| BlockRef {
                    label: crate::secrets::redact(&block.label),
                    trust: block.trust,
                    bytes: block.content.len() as u32,
                })
                .collect(),
            output: crate::secrets::redact(output),
            usage,
        });
    }

    pub fn note(&mut self, text: &str) {
        let seq = self.next_seq();
        self.entries.push(Entry::Note {
            seq,
            text: crate::secrets::redact(text),
        });
    }

    pub fn record_decision(&mut self, decision: Decision) {
        let seq = self.next_seq();
        self.entries.push(Entry::Decided { seq, decision });
    }

    /// Every tool call, in order.
    pub fn calls(&self) -> impl Iterator<Item = (&str, &Value, &CallOutcome)> {
        self.entries.iter().filter_map(|entry| match entry {
            Entry::ToolCall {
                name,
                input,
                outcome,
                ..
            } => Some((name.as_str(), input, outcome)),
            _ => None,
        })
    }

    /// Every model exchange, in order.
    pub fn exchanges(&self) -> impl Iterator<Item = &Entry> {
        self.entries
            .iter()
            .filter(|entry| matches!(entry, Entry::ModelExchange { .. }))
    }

    /// How many tool calls the run has made, for the budget.
    pub fn call_count(&self) -> u16 {
        u16::try_from(self.calls().count()).unwrap_or(u16::MAX)
    }

    /// Tokens spent across every exchange, input and output.
    ///
    /// Read off the record for the same reason the call count is: two gates over
    /// one record must not each allow a full budget, and a counter one of them
    /// holds is a counter the other does not see. An exchange whose provider
    /// reported no usage contributes nothing — which under-counts, so
    /// `max_tokens` is a cap on what is KNOWN to have been spent. Saying so is
    /// better than guessing a number: the alternative is a cap that stops a run
    /// on an estimate the operator cannot reconcile with their bill.
    pub fn tokens_spent(&self) -> u32 {
        self.entries
            .iter()
            .filter_map(|entry| match entry {
                Entry::ModelExchange { usage, .. } => *usage,
                _ => None,
            })
            .fold(0u32, |total, usage| {
                total
                    .saturating_add(usage.input)
                    .saturating_add(usage.output)
            })
    }
}

/// Redacts every string in a JSON value, at any depth.
///
/// A key is redacted as well as a value. A tool input is a model's output, so a
/// key like `{"sk-ant-…": 1}` is a thing it can produce, and a scrubber that
/// only walked values would store it.
pub fn redact_value(value: &Value) -> Value {
    match value {
        Value::String(text) => Value::String(crate::secrets::redact(text)),
        Value::Array(items) => Value::Array(items.iter().map(redact_value).collect()),
        Value::Object(fields) => Value::Object(
            fields
                .iter()
                .map(|(key, value)| (crate::secrets::redact(key), redact_value(value)))
                .collect(),
        ),
        other => other.clone(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::trust::InputKind;
    use liyasa_core::ai::{ChatRequest, DataBlock};
    use serde_json::json;

    const SECRET: &str = "AKIAQWERTYUIOPASDFGH";

    fn header() -> Header {
        Header {
            task: Task {
                trigger: TriggerKind::Drift,
                text: "the seat limit changed".to_owned(),
                pages: [Route::new("/pricing")].into(),
            },
            trust: TrustLevel::Member,
            scope: WriteScope::Anywhere,
            inputs: vec![Input::new(
                InputKind::Task,
                "drift record 41",
                TrustLevel::Member,
            )],
            content_tree: Some("blake3:abc".to_owned()),
            graph: GraphAccess::Read,
            context_repos: vec!["acme/api".to_owned()],
            policy: Policy::Proposal,
            retention_days: 90,
        }
    }

    fn record() -> RunRecord {
        RunRecord::open(RunId::new("run-1"), header())
    }

    fn request() -> ChatRequest {
        ChatRequest {
            system: "You write documentation.".to_owned(),
            messages: Vec::new(),
            data: vec![DataBlock {
                label: "ticket 88".to_owned(),
                trust: TrustLevel::External,
                content: format!("my key is {SECRET} and it does not work"),
            }],
            tools: Vec::new(),
            output_schema: None,
            budget: crate::config::default_budget(),
        }
    }

    fn as_text(record: &RunRecord) -> String {
        serde_json::to_string(record).expect("the record serializes")
    }

    #[test]
    fn the_header_is_agt_01s_list() {
        let record = record();
        let header = record.header();
        assert_eq!(header.task.trigger, TriggerKind::Drift);
        assert_eq!(header.trust, TrustLevel::Member);
        assert_eq!(header.content_tree.as_deref(), Some("blake3:abc"));
        assert_eq!(header.graph, GraphAccess::Read);
        assert_eq!(header.context_repos, ["acme/api"]);
        assert_eq!(header.policy, Policy::Proposal);
        assert_eq!(header.scope, WriteScope::Anywhere);
        assert_eq!(header.retention_days, 90);
        assert_eq!(header.inputs.len(), 1);
    }

    #[test]
    fn every_tool_call_is_recorded_whether_it_ran_or_not() {
        let mut record = record();
        record.record_call(
            "read_page",
            &json!({ "route": "/pricing" }),
            CallOutcome::Ok,
        );
        record.record_call(
            "edit_navigation",
            &json!({ "operation": "remove", "route": "/pricing" }),
            CallOutcome::Rejected {
                reason: "below min_trust".to_owned(),
            },
        );
        let calls: Vec<&str> = record.calls().map(|(name, _, _)| name).collect();
        assert_eq!(calls, ["read_page", "edit_navigation"]);
        assert_eq!(record.call_count(), 2);
    }

    #[test]
    fn a_secret_in_a_tool_input_is_not_stored() {
        let mut record = record();
        record.record_call(
            "write_page",
            &json!({ "route": "/pricing", "markdown": format!("use {SECRET}") }),
            CallOutcome::Ok,
        );
        let text = as_text(&record);
        assert!(!text.contains(SECRET), "the record stored the key: {text}");
        assert!(text.contains("[redacted]"), "{text}");
    }

    #[test]
    fn a_secret_in_a_nested_tool_input_is_not_stored() {
        let mut record = record();
        record.record_call(
            "propose_fact_update",
            &json!({ "value": { "items": [{ "token": SECRET }] } }),
            CallOutcome::Ok,
        );
        assert!(!as_text(&record).contains(SECRET));
    }

    #[test]
    fn a_secret_in_a_json_key_is_not_stored() {
        // A tool input is a model's output, so a key is as much a place for one as
        // a value is.
        let mut record = record();
        record.record_call("write_page", &json!({ SECRET: 1 }), CallOutcome::Ok);
        assert!(!as_text(&record).contains(SECRET));
    }

    #[test]
    fn a_secret_in_a_rejection_reason_is_not_stored() {
        let mut record = record();
        record.record_call(
            "web_fetch",
            &json!({ "url": "https://example.test/" }),
            CallOutcome::Rejected {
                reason: format!("the url carried {SECRET}"),
            },
        );
        assert!(!as_text(&record).contains(SECRET));
    }

    #[test]
    fn a_secret_in_the_task_text_is_not_stored() {
        // The trigger text is the untrusted half of a run and the likeliest place
        // for a pasted credential: a support ticket saying "my key sk-… fails".
        let mut header = header();
        header.task.text = format!("my key {SECRET} does not work");
        let record = RunRecord::open(RunId::new("run-2"), header);
        assert!(!as_text(&record).contains(SECRET));
    }

    #[test]
    fn a_model_exchange_is_recorded_with_its_data_blocks_named_not_inlined() {
        let mut record = record();
        record.record_exchange(
            "anthropic:claude-opus-5-5",
            &request(),
            "I will update the pricing page.",
            Some(Usage {
                input: 900,
                output: 40,
            }),
        );
        assert_eq!(record.exchanges().count(), 1);
        let text = as_text(&record);
        assert!(!text.contains(SECRET), "{text}");
        // The block is named and measured, and its content is not stored.
        assert!(text.contains("ticket 88"), "{text}");
        assert!(!text.contains("does not work"), "{text}");
        assert!(text.contains("\"bytes\""), "{text}");
    }

    #[test]
    fn a_secret_in_a_models_own_output_is_not_stored() {
        let mut record = record();
        record.record_exchange(
            "anthropic:claude-opus-5-5",
            &request(),
            &format!("the key is {SECRET}"),
            None,
        );
        assert!(!as_text(&record).contains(SECRET));
    }

    #[test]
    fn a_secret_in_a_note_is_not_stored() {
        let mut record = record();
        record.note(&format!("saw {SECRET} in the repo"));
        assert!(!as_text(&record).contains(SECRET));
    }

    #[test]
    fn a_secret_in_the_system_prompt_is_not_stored() {
        // Operator text, so not untrusted — and an operator who pasted a key into
        // `ai.instructions` should not have it copied into every run record.
        let mut record = record();
        let mut request = request();
        request.system = format!("Use {SECRET} when you call the API.");
        record.record_exchange("m", &request, "ok", None);
        assert!(!as_text(&record).contains(SECRET));
    }

    #[test]
    fn entries_are_ordered_and_numbered_from_zero() {
        let mut record = record();
        record.enter(Phase::Research);
        record.record_call("search_docs", &json!({ "query": "seats" }), CallOutcome::Ok);
        record.enter(Phase::Plan);
        record.note("two pages to change");
        let seqs: Vec<u32> = record.entries().iter().map(Entry::seq).collect();
        assert_eq!(seqs, [0, 1, 2, 3]);
    }

    #[test]
    fn the_record_knows_which_phases_it_passed_through() {
        let mut record = record();
        assert_eq!(record.phase(), None);
        for phase in Phase::ALL {
            record.enter(phase);
        }
        assert_eq!(record.phases(), Phase::ALL);
        assert_eq!(record.phase(), Some(Phase::Publish));
    }

    #[test]
    fn the_phases_are_the_sequence_agt_02_names() {
        let mut phase = Phase::Research;
        let mut walked = vec![phase];
        while let Some(next) = phase.next() {
            walked.push(next);
            phase = next;
        }
        assert_eq!(walked, Phase::ALL);
        assert_eq!(Phase::Publish.next(), None);
    }

    #[test]
    fn a_record_round_trips_through_json() {
        // It is stored and read back for review, so it has to survive that.
        let mut record = record();
        record.enter(Phase::Research);
        record.record_call(
            "read_page",
            &json!({ "route": "/pricing" }),
            CallOutcome::Ok,
        );
        record.record_exchange("m", &request(), "ok", None);
        record.record_decision(crate::policy::Decision {
            outcome: crate::policy::Outcome::Proposal,
            configured: Policy::Proposal,
            downgraded: None,
        });
        let text = as_text(&record);
        let back: RunRecord = serde_json::from_str(&text).expect("it reads back");
        assert_eq!(back, record);
    }

    #[test]
    fn tokens_spent_sums_every_exchange() {
        let mut record = record();
        for (input, output) in [(100u32, 20u32), (300, 40)] {
            record.record_exchange("m", &request(), "ok", Some(Usage { input, output }));
        }
        assert_eq!(record.tokens_spent(), 460);
    }

    #[test]
    fn an_exchange_with_no_usage_reported_counts_nothing() {
        // Under-counting rather than guessing: a cap the operator cannot
        // reconcile with their bill is worse than a cap that is slightly loose.
        let mut record = record();
        record.record_exchange("m", &request(), "ok", None);
        assert_eq!(record.tokens_spent(), 0);
    }

    #[test]
    fn retention_is_carried_rather_than_assumed() {
        let mut header = header();
        header.retention_days = 7;
        assert_eq!(
            RunRecord::open(RunId::new("r"), header)
                .header()
                .retention_days,
            7
        );
    }
}
