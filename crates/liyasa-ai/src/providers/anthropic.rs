//! The Anthropic Messages wire format.
//!
//! The recommended default for the assistant and agent roles (§6.7). There is
//! no embedding endpoint, which the compatibility table says and
//! [`super::embedding_model`] enforces.

use liyasa_core::ai::{AiError, ChatEvent, ChatModel, ChatRequest, Message, Part, Role as MsgRole};
use liyasa_core::net::{BoxFut, BoxStream, HttpRequest, Method};
use serde_json::{Value, json};

use super::openai::{error_from_json, futures_iter};
use super::{Endpoint, stream};
use crate::prompt;

/// The API version header. Anthropic dates its API rather than numbering it,
/// and omitting the header is an error rather than a default.
pub const API_VERSION: &str = "2023-06-01";

pub struct Chat {
    model: String,
    endpoint: Endpoint,
}

impl Chat {
    pub fn new(model: String, endpoint: Endpoint) -> Self {
        Self { model, endpoint }
    }
}

pub fn chat_body(model: &str, req: &ChatRequest) -> Value {
    let mut body = json!({
        "model": model,
        "max_tokens": req.budget.max_tokens,
        "stream": true,
        "messages": prompt::messages(req).iter().map(wire_message).collect::<Vec<_>>(),
    });
    if !req.system.is_empty() {
        // A top-level field here, not a message; operator text only.
        body["system"] = Value::String(req.system.clone());
    }

    let trust = prompt::effective_trust(req);
    let tools = prompt::tools_for(&req.tools, trust);
    if !tools.is_empty() {
        body["tools"] = Value::Array(
            tools
                .iter()
                .map(|tool| {
                    json!({
                        "name": tool.name,
                        "description": tool.description,
                        "input_schema": tool.input_schema,
                    })
                })
                .collect(),
        );
    }
    body
}

fn wire_message(message: &Message) -> Value {
    let role = match message.role {
        // This format has no `tool` role: a result is a user-turn content block.
        MsgRole::Assistant => "assistant",
        _ => "user",
    };
    let content: Vec<Value> = message
        .content
        .iter()
        .filter_map(|part| match part {
            Part::Text(text) => Some(json!({ "type": "text", "text": text })),
            Part::ToolCall { id, name, input } => Some(json!({
                "type": "tool_use", "id": id, "name": name, "input": input,
            })),
            Part::ToolResult { id, output, .. } => Some(json!({
                "type": "tool_result",
                "tool_use_id": id,
                "content": output.to_string(),
            })),
            Part::Image { mime, data } => Some(json!({
                "type": "image",
                "source": {
                    "type": "base64",
                    "media_type": mime,
                    "data": super::openai::base64_for_tests(data),
                },
            })),
            _ => None,
        })
        .collect();
    json!({ "role": role, "content": content })
}

/// The events one buffered SSE body describes (RFC 1803).
pub fn chat_events(body: &str) -> Result<Vec<ChatEvent>, AiError> {
    if !stream::looks_like_sse(body) {
        return Err(error_from_json(body, 200));
    }
    let mut out = Vec::new();
    // A `tool_use` block's input arrives as `partial_json` deltas against the
    // block's index, so the arguments are only parseable at `content_block_stop`.
    let mut open: Option<(usize, String, String, String)> = None;

    for payload in stream::parse_sse(body) {
        let Ok(event) = serde_json::from_str::<Value>(payload) else {
            continue;
        };
        match event.get("type").and_then(Value::as_str) {
            Some("content_block_start") => {
                let at = event.get("index").and_then(Value::as_u64).unwrap_or(0) as usize;
                if let Some(block) = event.get("content_block")
                    && block.get("type").and_then(Value::as_str) == Some("tool_use")
                {
                    open = Some((
                        at,
                        block
                            .get("id")
                            .and_then(Value::as_str)
                            .unwrap_or_default()
                            .to_owned(),
                        block
                            .get("name")
                            .and_then(Value::as_str)
                            .unwrap_or_default()
                            .to_owned(),
                        String::new(),
                    ));
                }
            }
            Some("content_block_delta") => {
                let delta = event.get("delta");
                if let Some(text) = delta
                    .and_then(|d| d.get("text"))
                    .and_then(Value::as_str)
                    .filter(|t| !t.is_empty())
                {
                    out.push(ChatEvent::Token(text.to_owned()));
                }
                if let Some(chunk) = delta
                    .and_then(|d| d.get("partial_json"))
                    .and_then(Value::as_str)
                    && let Some(call) = open.as_mut()
                {
                    call.3.push_str(chunk);
                }
            }
            Some("content_block_stop") => {
                if let Some((_, id, name, arguments)) = open.take()
                    && !name.is_empty()
                {
                    out.push(ChatEvent::ToolCall {
                        id,
                        name,
                        input: serde_json::from_str(&arguments).unwrap_or_else(|_| json!({})),
                    });
                }
            }
            Some("message_start") => {
                if let Some(usage) = event.get("message").and_then(|m| m.get("usage")) {
                    out.push(usage_event(usage));
                }
            }
            Some("message_delta") => {
                if let Some(usage) = event.get("usage") {
                    out.push(usage_event(usage));
                }
            }
            Some("error") => {
                let message = event
                    .get("error")
                    .and_then(|e| e.get("message"))
                    .and_then(Value::as_str)
                    .unwrap_or("provider error")
                    .to_owned();
                return Err(AiError::Provider {
                    status: 200,
                    message,
                });
            }
            _ => {}
        }
    }
    out.push(ChatEvent::Done);
    Ok(out)
}

fn usage_event(usage: &Value) -> ChatEvent {
    ChatEvent::Usage {
        input: usage
            .get("input_tokens")
            .and_then(Value::as_u64)
            .unwrap_or(0) as u32,
        output: usage
            .get("output_tokens")
            .and_then(Value::as_u64)
            .unwrap_or(0) as u32,
    }
}

impl ChatModel for Chat {
    fn id(&self) -> &str {
        &self.model
    }

    fn complete<'a>(
        &'a self,
        req: ChatRequest,
    ) -> BoxFut<'a, Result<BoxStream<'a, ChatEvent>, AiError>> {
        Box::pin(async move {
            let body = chat_body(&self.model, &req);
            let url = format!("{}/v1/messages", self.endpoint.base_url);
            let url = url::Url::parse(&url).map_err(|e| AiError::Provider {
                status: 0,
                message: format!("`{url}` is not a URL: {e}"),
            })?;
            let mut headers = vec![
                ("content-type".to_owned(), "application/json".to_owned()),
                ("anthropic-version".to_owned(), API_VERSION.to_owned()),
            ];
            if let Some(key) = self.endpoint.key.as_ref() {
                // `x-api-key`, not `Authorization`.
                headers.push(("x-api-key".to_owned(), key.as_str().to_owned()));
            }
            let response = self
                .endpoint
                .http
                .fetch(
                    HttpRequest {
                        method: Method::POST,
                        url,
                        headers,
                        body: Some(liyasa_core::vfs::Bytes::from(body.to_string())),
                    },
                    &self.endpoint.policy,
                )
                .await?;
            let text = String::from_utf8_lossy(&response.body).into_owned();
            if response.status >= 400 {
                return Err(error_from_json(&text, response.status));
            }
            Ok(Box::pin(futures_iter(chat_events(&text)?)) as BoxStream<'a, ChatEvent>)
        })
    }
}
