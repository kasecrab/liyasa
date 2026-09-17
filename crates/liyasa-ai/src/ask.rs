//! The assistant as an MCP tool (AST-32).
//!
//! WP-19 owns the MCP server and mounts this; what lives here is the tool's
//! shape and the one call behind it, so the answer an agent gets is the answer
//! a reader gets — same retrieval, same entitlement filter, same citations.
//!
//! An agent asking is still an `anonymous` caller. Being a program rather than
//! a person does not raise trust, and an agent relaying a third party's text is
//! the injection path §30.2.2 exists for.

use liyasa_core::ai::{AiError, ToolSpec, TrustLevel};
use serde::{Deserialize, Serialize};
use serde_json::json;

use crate::assistant::run::Plan;
use crate::assistant::{Answer, Thread, ask};
use crate::privacy::Caller;

pub const TOOL: &str = "ask";

/// The MCP tool declaration.
pub fn spec() -> ToolSpec {
    ToolSpec {
        name: TOOL.to_owned(),
        description: "Ask this documentation site a question. Returns an answer with citations \
                      that deep-link to the sections it used."
            .to_owned(),
        input_schema: json!({
            "type": "object",
            "properties": {
                "question": { "type": "string" },
                "version": { "type": "string" },
                "locale": { "type": "string" },
                "thread": {
                    "type": "string",
                    "description": "Continue an earlier conversation."
                }
            },
            "required": ["question"],
            "additionalProperties": false
        }),
        min_trust: TrustLevel::External,
    }
}

/// What the tool returns (AST-32: "answer plus citations").
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct AskResult {
    pub answer: String,
    pub citations: Vec<Citation>,
    pub confidence: f32,
    /// Present when the assistant declined; an agent should not retry into it.
    #[serde(skip_serializing_if = "Vec::is_empty")]
    pub deflection: Vec<String>,
    pub thread: String,
    /// Always `agent` from this entry point (AST-40's breakdown).
    pub caller: Caller,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Citation {
    pub href: String,
    pub title: String,
}

impl AskResult {
    pub fn of(answer: &Answer, thread: &str) -> Self {
        Self {
            answer: answer.text.clone(),
            citations: answer
                .citations
                .iter()
                .map(|c| Citation {
                    href: c.href.clone(),
                    title: c.title.clone(),
                })
                .collect(),
            confidence: answer.confidence,
            deflection: answer
                .deflection
                .iter()
                .map(|target| target.value.clone())
                .collect(),
            thread: thread.to_owned(),
            caller: Caller::Agent,
        }
    }
}

/// One `ask` call.
pub async fn call(plan: &Plan<'_>, thread: &Thread, question: &str) -> Result<AskResult, AiError> {
    let outcome = ask(plan, thread, question).await?;
    Ok(AskResult::of(&outcome.answer, &thread.id))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_tool_takes_a_question_and_nothing_undeclared() {
        let spec = spec();
        assert_eq!(spec.name, "ask");
        assert_eq!(spec.input_schema["required"], json!(["question"]));
        assert_eq!(spec.input_schema["additionalProperties"], json!(false));
    }

    #[test]
    fn an_agent_is_not_more_trusted_than_a_reader() {
        assert_eq!(spec().min_trust, TrustLevel::External);
    }

    #[test]
    fn a_declined_answer_carries_its_deflection_rather_than_looking_confident() {
        use crate::assistant::answer::DeflectionKind;
        use crate::assistant::{Citation as AnswerCitation, DeflectionTarget};

        let answer = Answer {
            text: "I could not find this.".to_owned(),
            citations: vec![AnswerCitation {
                href: "/a#b".to_owned(),
                title: "A".to_owned(),
                breadcrumb: Vec::new(),
            }],
            confidence: 0.0,
            follow_ups: Vec::new(),
            deflection: vec![DeflectionTarget {
                kind: DeflectionKind::Email,
                value: "support@example.com".to_owned(),
            }],
            trust: TrustLevel::Anonymous,
        };
        let result = AskResult::of(&answer, "th_1");
        assert_eq!(result.confidence, 0.0);
        assert_eq!(result.deflection, ["support@example.com"]);
        assert_eq!(result.caller, Caller::Agent);
        assert_eq!(result.citations[0].href, "/a#b");
    }
}
