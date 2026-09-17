//! The OpenAI wire format, and every endpoint that speaks it.
//!
//! Azure OpenAI, OpenRouter, Groq, Together, vLLM and Ollama all answer this
//! shape, so they share one adapter and differ only in `baseUrl` (§6.7).

use liyasa_core::ai::{
    AiError, ChatEvent, ChatModel, ChatRequest, EmbeddingModel, Message, Part, Role as MsgRole,
};
use liyasa_core::net::{BoxFut, BoxStream, HttpRequest, Method};
use serde_json::{Value, json};

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

/// The request body. Separated from the call so a test reads what would be
/// sent rather than a description of it.
pub fn chat_body(model: &str, req: &ChatRequest) -> Value {
    let mut messages = Vec::new();
    if !req.system.is_empty() {
        // Operator text only; §30.2.2 item 2.
        messages.push(json!({ "role": "system", "content": req.system }));
    }
    for message in prompt::messages(req) {
        messages.extend(wire_message(&message));
    }

    let mut body = json!({
        "model": model,
        "messages": messages,
        "max_tokens": req.budget.max_tokens,
        "stream": true,
        "stream_options": { "include_usage": true },
    });

    let trust = prompt::effective_trust(req);
    let tools = prompt::tools_for(&req.tools, trust);
    if !tools.is_empty() {
        body["tools"] = Value::Array(
            tools
                .iter()
                .map(|tool| {
                    json!({
                        "type": "function",
                        "function": {
                            "name": tool.name,
                            "description": tool.description,
                            "parameters": tool.input_schema,
                        }
                    })
                })
                .collect(),
        );
    }
    if let Some(schema) = &req.output_schema {
        body["response_format"] = json!({
            "type": "json_schema",
            "json_schema": { "name": "answer", "strict": true, "schema": schema },
        });
    }
    body
}

/// One `Message` as the one or more wire messages it needs.
///
/// A tool result is its own wire message in this format, so a message holding
/// both text and results expands rather than losing one of them.
fn wire_message(message: &Message) -> Vec<Value> {
    let role = match message.role {
        MsgRole::User => "user",
        MsgRole::Assistant => "assistant",
        MsgRole::Tool => "tool",
    };
    let mut out = Vec::new();
    let mut content = Vec::new();
    let mut calls = Vec::new();

    for part in &message.content {
        match part {
            Part::Text(text) => content.push(json!({ "type": "text", "text": text })),
            Part::Image { mime, data } => content.push(json!({
                "type": "image_url",
                "image_url": { "url": format!("data:{mime};base64,{}", base64(data)) },
            })),
            Part::ToolCall { id, name, input } => calls.push(json!({
                "id": id,
                "type": "function",
                "function": { "name": name, "arguments": input.to_string() },
            })),
            Part::ToolResult { id, output, .. } => out.push(json!({
                "role": "tool",
                "tool_call_id": id,
                "content": output.to_string(),
            })),
            _ => {}
        }
    }

    if !content.is_empty() || !calls.is_empty() {
        let mut message = json!({ "role": role });
        if !content.is_empty() {
            message["content"] = Value::Array(content);
        }
        if !calls.is_empty() {
            message["tool_calls"] = Value::Array(calls);
        }
        out.insert(0, message);
    }
    out
}

/// The events one buffered SSE body describes (RFC 1803).
pub fn chat_events(body: &str) -> Result<Vec<ChatEvent>, AiError> {
    if !stream::looks_like_sse(body) {
        return Err(error_from_json(body, 200));
    }
    let mut out = Vec::new();
    // A tool call arrives as deltas keyed by index; the arguments are a string
    // built up across events and are only valid JSON once complete.
    let mut calls: Vec<(String, String, String)> = Vec::new();

    for payload in stream::parse_sse(body) {
        let Ok(event) = serde_json::from_str::<Value>(payload) else {
            continue;
        };
        if let Some(usage) = event.get("usage").filter(|u| !u.is_null()) {
            out.push(ChatEvent::Usage {
                input: usage
                    .get("prompt_tokens")
                    .and_then(Value::as_u64)
                    .unwrap_or(0) as u32,
                output: usage
                    .get("completion_tokens")
                    .and_then(Value::as_u64)
                    .unwrap_or(0) as u32,
            });
        }
        let Some(delta) = event
            .get("choices")
            .and_then(|c| c.get(0))
            .and_then(|c| c.get("delta"))
        else {
            continue;
        };
        if let Some(text) = delta.get("content").and_then(Value::as_str)
            && !text.is_empty()
        {
            out.push(ChatEvent::Token(text.to_owned()));
        }
        for call in delta
            .get("tool_calls")
            .and_then(Value::as_array)
            .into_iter()
            .flatten()
        {
            let at = call.get("index").and_then(Value::as_u64).unwrap_or(0) as usize;
            if calls.len() <= at {
                calls.resize(at + 1, (String::new(), String::new(), String::new()));
            }
            if let Some(id) = call.get("id").and_then(Value::as_str) {
                calls[at].0 = id.to_owned();
            }
            if let Some(name) = call
                .get("function")
                .and_then(|f| f.get("name"))
                .and_then(Value::as_str)
            {
                calls[at].1 = name.to_owned();
            }
            if let Some(chunk) = call
                .get("function")
                .and_then(|f| f.get("arguments"))
                .and_then(Value::as_str)
            {
                calls[at].2.push_str(chunk);
            }
        }
    }

    for (id, name, arguments) in calls {
        if name.is_empty() {
            continue;
        }
        // Arguments that did not parse are passed as `{}` rather than dropped:
        // the loop must see the call so it can refuse it, not silently lose it.
        let input = serde_json::from_str(&arguments).unwrap_or_else(|_| json!({}));
        out.push(ChatEvent::ToolCall { id, name, input });
    }
    out.push(ChatEvent::Done);
    Ok(out)
}

/// A provider's error body, whatever shape it arrived in.
pub fn error_from_json(body: &str, status: u16) -> AiError {
    let message = serde_json::from_str::<Value>(body)
        .ok()
        .and_then(|v| {
            v.get("error")
                .and_then(|e| e.get("message"))
                .or_else(|| v.get("message"))
                .and_then(Value::as_str)
                .map(str::to_owned)
        })
        .unwrap_or_else(|| body.chars().take(200).collect());
    match status {
        429 => AiError::RateLimited { retry_after: None },
        _ => AiError::Provider { status, message },
    }
}

pub(crate) fn base64_for_tests(data: &[u8]) -> String {
    base64(data)
}

fn base64(data: &[u8]) -> String {
    const ALPHABET: &[u8; 64] = b"ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789+/";
    let mut out = String::with_capacity(data.len().div_ceil(3) * 4);
    for group in data.chunks(3) {
        let b = [
            group[0],
            group.get(1).copied().unwrap_or(0),
            group.get(2).copied().unwrap_or(0),
        ];
        let n = (u32::from(b[0]) << 16) | (u32::from(b[1]) << 8) | u32::from(b[2]);
        for i in 0..4 {
            if i <= group.len() {
                out.push(ALPHABET[((n >> (18 - i * 6)) & 0x3f) as usize] as char);
            } else {
                out.push('=');
            }
        }
    }
    out
}

fn headers(endpoint: &Endpoint) -> Vec<(String, String)> {
    let mut out = vec![("content-type".to_owned(), "application/json".to_owned())];
    if let Some(key) = endpoint.key.as_ref() {
        out.push((
            "authorization".to_owned(),
            format!("Bearer {}", key.as_str()),
        ));
    }
    out
}

fn post<'a>(
    endpoint: &'a Endpoint,
    path: &str,
    body: Value,
) -> BoxFut<'a, Result<(u16, String), AiError>> {
    let url = format!("{}{path}", endpoint.base_url);
    let headers = headers(endpoint);
    Box::pin(async move {
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
                    headers,
                    body: Some(liyasa_core::vfs::Bytes::from(body.to_string())),
                },
                &endpoint.policy,
            )
            .await?;
        Ok((
            response.status,
            String::from_utf8_lossy(&response.body).into_owned(),
        ))
    })
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
            let (status, text) = post(&self.endpoint, "/chat/completions", body).await?;
            if status >= 400 {
                return Err(error_from_json(&text, status));
            }
            let events = chat_events(&text)?;
            Ok(Box::pin(futures_iter(events)) as BoxStream<'a, ChatEvent>)
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
    json!({ "model": model, "input": inputs })
}

/// The vectors, in the order the provider indexed them.
///
/// `data` is not guaranteed to arrive in request order, so it is sorted by
/// `index`. A provider that returned them shuffled would otherwise pair every
/// chunk with the wrong vector, and nothing downstream could tell.
pub fn parse_embeddings(body: &str, expected: usize) -> Result<Vec<Vec<f32>>, AiError> {
    let value: Value = serde_json::from_str(body).map_err(|e| AiError::Provider {
        status: 200,
        message: format!("the embedding response is not JSON: {e}"),
    })?;
    let Some(data) = value.get("data").and_then(Value::as_array) else {
        return Err(error_from_json(body, 200));
    };
    let mut rows: Vec<(usize, Vec<f32>)> = Vec::with_capacity(data.len());
    for (n, entry) in data.iter().enumerate() {
        let at = entry
            .get("index")
            .and_then(Value::as_u64)
            .unwrap_or(n as u64) as usize;
        let vector = entry
            .get("embedding")
            .and_then(Value::as_array)
            .map(|v| {
                v.iter()
                    .filter_map(Value::as_f64)
                    .map(|f| f as f32)
                    .collect::<Vec<f32>>()
            })
            .unwrap_or_default();
        rows.push((at, vector));
    }
    rows.sort_by_key(|(at, _)| *at);
    if rows.len() != expected {
        return Err(AiError::Provider {
            status: 200,
            message: format!(
                "asked for {expected} embeddings and received {}",
                rows.len()
            ),
        });
    }
    Ok(rows.into_iter().map(|(_, v)| v).collect())
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
            let body = embedding_body(&self.model, inputs);
            let (status, text) = post(&self.endpoint, "/embeddings", body).await?;
            if status >= 400 {
                return Err(error_from_json(&text, status));
            }
            let vectors = parse_embeddings(&text, inputs.len())?;
            // A model whose width is not what the index records would write
            // rows nothing can query. Caught here rather than at the database.
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

/// A finished list as a stream. The events are already all present (RFC 1803).
pub(crate) fn futures_iter<T: Send + Unpin + 'static>(
    items: Vec<T>,
) -> impl futures_core::Stream<Item = T> + Send {
    struct Iter<T>(std::vec::IntoIter<T>);
    impl<T: Unpin> futures_core::Stream for Iter<T> {
        type Item = T;
        fn poll_next(
            mut self: std::pin::Pin<&mut Self>,
            _cx: &mut std::task::Context<'_>,
        ) -> std::task::Poll<Option<T>> {
            std::task::Poll::Ready(self.0.next())
        }
    }
    Iter(items.into_iter())
}
