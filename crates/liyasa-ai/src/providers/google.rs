//! The Gemini `generateContent` wire format.
//!
//! The key travels in `x-goog-api-key`, never as a `?key=` query parameter:
//! §30.2.3 says logging never records query strings, which is a reason to keep
//! secrets out of them and not a guarantee that a proxy or an access log
//! elsewhere will do the same.

use liyasa_core::ai::{
    AiError, ChatEvent, ChatModel, ChatRequest, EmbeddingModel, Message, Part, Role as MsgRole,
};
use liyasa_core::net::{BoxFut, BoxStream, HttpRequest, Method};
use serde_json::{Value, json};

use super::openai::{error_from_json, futures_iter};
use super::{Endpoint, stream};
use crate::prompt;

pub struct Chat {
    model: String,
    endpoint: Endpoint,
}

impl Chat {
    pub fn new(model: String, endpoint: Endpoint) -> Self {
        Self { model, endpoint }
    }
}

pub fn chat_body(req: &ChatRequest) -> Value {
    let mut body = json!({
        "contents": prompt::messages(req).iter().map(wire_message).collect::<Vec<_>>(),
        "generationConfig": { "maxOutputTokens": req.budget.max_tokens },
    });
    if !req.system.is_empty() {
        body["systemInstruction"] = json!({ "parts": [{ "text": req.system }] });
    }
    let trust = prompt::effective_trust(req);
    let tools = prompt::tools_for(&req.tools, trust);
    if !tools.is_empty() {
        body["tools"] = json!([{
            "functionDeclarations": tools.iter().map(|tool| json!({
                "name": tool.name,
                "description": tool.description,
                "parameters": tool.input_schema,
            })).collect::<Vec<_>>()
        }]);
    }
    if let Some(schema) = &req.output_schema {
        body["generationConfig"]["responseMimeType"] = Value::String("application/json".to_owned());
        body["generationConfig"]["responseSchema"] = schema.clone();
    }
    body
}

fn wire_message(message: &Message) -> Value {
    let role = match message.role {
        MsgRole::Assistant => "model",
        _ => "user",
    };
    let parts: Vec<Value> = message
        .content
        .iter()
        .filter_map(|part| match part {
            Part::Text(text) => Some(json!({ "text": text })),
            Part::ToolCall { name, input, .. } => Some(json!({
                "functionCall": { "name": name, "args": input }
            })),
            Part::ToolResult { id, output, .. } => Some(json!({
                "functionResponse": { "name": id, "response": { "result": output } }
            })),
            Part::Image { mime, data } => Some(json!({
                "inlineData": { "mimeType": mime, "data": super::openai::base64_for_tests(data) }
            })),
            _ => None,
        })
        .collect();
    json!({ "role": role, "parts": parts })
}

/// The events one buffered SSE body describes (RFC 1803).
pub fn chat_events(body: &str) -> Result<Vec<ChatEvent>, AiError> {
    if !stream::looks_like_sse(body) {
        return Err(error_from_json(body, 200));
    }
    let mut out = Vec::new();
    let mut calls = 0usize;
    for payload in stream::parse_sse(body) {
        let Ok(event) = serde_json::from_str::<Value>(payload) else {
            continue;
        };
        if let Some(usage) = event.get("usageMetadata") {
            out.push(ChatEvent::Usage {
                input: usage
                    .get("promptTokenCount")
                    .and_then(Value::as_u64)
                    .unwrap_or(0) as u32,
                output: usage
                    .get("candidatesTokenCount")
                    .and_then(Value::as_u64)
                    .unwrap_or(0) as u32,
            });
        }
        for part in event
            .get("candidates")
            .and_then(Value::as_array)
            .into_iter()
            .flatten()
            .filter_map(|c| c.get("content"))
            .filter_map(|c| c.get("parts"))
            .filter_map(Value::as_array)
            .flatten()
        {
            if let Some(text) = part
                .get("text")
                .and_then(Value::as_str)
                .filter(|t| !t.is_empty())
            {
                out.push(ChatEvent::Token(text.to_owned()));
            }
            if let Some(call) = part.get("functionCall") {
                let name = call
                    .get("name")
                    .and_then(Value::as_str)
                    .unwrap_or_default()
                    .to_owned();
                if name.is_empty() {
                    continue;
                }
                calls += 1;
                out.push(ChatEvent::ToolCall {
                    // This format frames no call id, so one is minted from the
                    // ordinal. A result must come back under the same name.
                    id: format!("call_{calls}"),
                    name,
                    input: call.get("args").cloned().unwrap_or_else(|| json!({})),
                });
            }
        }
    }
    out.push(ChatEvent::Done);
    Ok(out)
}

fn headers(endpoint: &Endpoint) -> Vec<(String, String)> {
    let mut out = vec![("content-type".to_owned(), "application/json".to_owned())];
    if let Some(key) = endpoint.key.as_ref() {
        out.push(("x-goog-api-key".to_owned(), key.as_str().to_owned()));
    }
    out
}

async fn post(endpoint: &Endpoint, path: String, body: Value) -> Result<(u16, String), AiError> {
    let url = format!("{}{path}", endpoint.base_url);
    let url = url::Url::parse(&url).map_err(|e| AiError::Provider {
        status: 0,
        message: format!("`{url}` is not a URL: {e}"),
    })?;
    let response = endpoint
        .http
        .fetch(
            HttpRequest {
                method: Method::POST,
                url,
                headers: headers(endpoint),
                body: Some(liyasa_core::vfs::Bytes::from(body.to_string())),
            },
            &endpoint.policy,
        )
        .await?;
    Ok((
        response.status,
        String::from_utf8_lossy(&response.body).into_owned(),
    ))
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
            let path = format!("/models/{}:streamGenerateContent?alt=sse", self.model);
            let (status, text) = post(&self.endpoint, path, chat_body(&req)).await?;
            if status >= 400 {
                return Err(error_from_json(&text, status));
            }
            Ok(Box::pin(futures_iter(chat_events(&text)?)) as BoxStream<'a, ChatEvent>)
        })
    }
}

pub struct Embeddings {
    model: String,
    dims: usize,
    endpoint: Endpoint,
}

impl Embeddings {
    pub fn new(model: String, dims: usize, endpoint: Endpoint) -> Self {
        Self {
            model,
            dims,
            endpoint,
        }
    }
}

pub fn embedding_body(model: &str, inputs: &[String]) -> Value {
    json!({
        "requests": inputs.iter().map(|text| json!({
            "model": format!("models/{model}"),
            "content": { "parts": [{ "text": text }] },
        })).collect::<Vec<_>>()
    })
}

pub fn parse_embeddings(body: &str, expected: usize) -> Result<Vec<Vec<f32>>, AiError> {
    let value: Value = serde_json::from_str(body).map_err(|e| AiError::Provider {
        status: 200,
        message: format!("the embedding response is not JSON: {e}"),
    })?;
    let Some(rows) = value.get("embeddings").and_then(Value::as_array) else {
        return Err(error_from_json(body, 200));
    };
    let out: Vec<Vec<f32>> = rows
        .iter()
        .map(|row| {
            row.get("values")
                .and_then(Value::as_array)
                .map(|v| {
                    v.iter()
                        .filter_map(Value::as_f64)
                        .map(|f| f as f32)
                        .collect()
                })
                .unwrap_or_default()
        })
        .collect();
    if out.len() != expected {
        return Err(AiError::Provider {
            status: 200,
            message: format!("asked for {expected} embeddings and received {}", out.len()),
        });
    }
    Ok(out)
}

impl EmbeddingModel for Embeddings {
    fn id(&self) -> &str {
        &self.model
    }

    fn dims(&self) -> usize {
        self.dims
    }

    fn embed<'a>(&'a self, inputs: &'a [String]) -> BoxFut<'a, Result<Vec<Vec<f32>>, AiError>> {
        Box::pin(async move {
            if inputs.is_empty() {
                return Ok(Vec::new());
            }
            let path = format!("/models/{}:batchEmbedContents", self.model);
            let (status, text) =
                post(&self.endpoint, path, embedding_body(&self.model, inputs)).await?;
            if status >= 400 {
                return Err(error_from_json(&text, status));
            }
            let vectors = parse_embeddings(&text, inputs.len())?;
            if let Some(found) = vectors.iter().map(Vec::len).find(|len| *len != self.dims) {
                return Err(AiError::Provider {
                    status: 200,
                    message: format!(
                        "`{}` returned {found}-dimensional vectors; this index is {}",
                        self.model, self.dims
                    ),
                });
            }
            Ok(vectors)
        })
    }
}
