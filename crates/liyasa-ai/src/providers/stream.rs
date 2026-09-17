//! Server-Sent Events, and the one place this crate admits it is not streaming.
//!
//! Every provider in §6.7 frames a streaming completion as SSE: `data:` lines
//! carrying JSON, terminated by `data: [DONE]` or an explicit final event.
//! The adapters ask for that format and parse it, so tool-call framing and the
//! per-token usage numbers survive.
//!
//! What they do not get is incremental delivery. `HttpClient::fetch` returns
//! the whole body when the request finishes (§30.2.3, and there is no chunked
//! variant), so the parse runs over a complete body and time to first token
//! equals time to last token. RFC 1803 records why, and what changes when
//! `HttpClient` grows a streaming method.

/// One `data:` payload, in order, with `[DONE]` dropped.
// TODO(rfc-1803): fed incrementally once `HttpClient` can stream.
pub fn parse_sse(body: &str) -> Vec<&str> {
    let mut out = Vec::new();
    for line in body.lines() {
        let line = line.trim_end_matches('\r');
        let Some(payload) = line.strip_prefix("data:") else {
            continue;
        };
        let payload = payload.trim();
        if payload.is_empty() || payload == "[DONE]" {
            continue;
        }
        out.push(payload);
    }
    out
}

/// Whether the body is SSE at all. A provider that answered with a plain JSON
/// error rather than a stream is the common case, and treating that as a
/// stream with no events would report an empty answer instead of the error.
pub fn looks_like_sse(body: &str) -> bool {
    body.lines()
        .any(|line| line.trim_start().starts_with("data:"))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn events_come_back_in_order_without_the_terminator() {
        let body = "data: {\"a\":1}\n\ndata: {\"a\":2}\n\ndata: [DONE]\n\n";
        assert_eq!(parse_sse(body), ["{\"a\":1}", "{\"a\":2}"]);
    }

    #[test]
    fn comments_and_event_lines_are_ignored() {
        let body = ": ping\nevent: message\ndata: {\"a\":1}\n\n";
        assert_eq!(parse_sse(body), ["{\"a\":1}"]);
    }

    #[test]
    fn carriage_returns_survive_the_split() {
        let body = "data: {\"a\":1}\r\n\r\n";
        assert_eq!(parse_sse(body), ["{\"a\":1}"]);
    }

    #[test]
    fn a_json_error_body_is_not_mistaken_for_an_empty_stream() {
        assert!(!looks_like_sse("{\"error\":{\"message\":\"no\"}}"));
        assert!(looks_like_sse("data: {}\n"));
    }
}
