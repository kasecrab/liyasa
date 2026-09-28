//! The stdio transport behind `liyasa mcp --dist dist/` (MCP-04).
//!
//! The point of MCP-04 is that an OSS user with no server still gets MCP,
//! locally and in CI. So this reads a static build directly and runs the same
//! [`protocol::dispatch`] the HTTP transport runs — the tools, the schemas
//! and the answers are identical, and the only differences are the two MCP-04
//! names: `ask` has no model to call and says so, and `report_issue` has
//! nowhere to file and refuses cleanly. Both stay in `tools/list` with their
//! reason attached, because omitting them would tell a model the SITE cannot
//! do these things when it is this transport that cannot.
//!
//! ## What a local run may read
//!
//! The same pages an anonymous reader is served, and no more (RFC 1902).
//! Whoever runs this holds `dist/` and could read any file in it with `cat`,
//! so the filter withholds nothing from *them* — but the caller here is an
//! agent, and AUTH-10's rule is that every surface filters identically. A
//! `--dist` server that answered with restricted pages would be the one
//! surface where "the assistant sees what the reader sees" is untrue, and it
//! would be the surface pointed at a model.
//!
//! ## Framing
//!
//! One JSON message per line, as the specification's stdio transport
//! defines it: no embedded newlines out, and a blank line in is skipped
//! rather than treated as a parse error, because shells and wrappers insert
//! them.
//!
//! ## Who supplies the streams
//!
//! [`run`] takes them rather than opening `stdin` and `stdout` itself. The
//! subcommand lives in `liyasa-cli` (WP-09), tokio's `io-std` feature is what
//! `tokio::io::stdin` needs, and `liyasa-cli` is where that row belongs —
//! `liyasa-server`'s manifest is shared and append-only, so widening ITS
//! tokio features to serve a binary in another crate is the kind of edit that
//! conflicts for everyone. Taking the streams as arguments also means a test
//! can drive the transport with a `Cursor` instead of spawning a process and
//! measuring the harness.

use std::path::Path;
use std::sync::Arc;

use serde_json::Value;
use tokio::io::{AsyncBufRead, AsyncBufReadExt, AsyncWrite, AsyncWriteExt, BufReader};

use super::bundle_reader::BundleReader;
use super::jsonrpc::Incoming;
use super::protocol::{self, Server};
use super::reader::Scope;
use super::tools::Host;
use crate::routes::bundle::Bundle;

/// Opens `dist` as an MCP corpus.
///
/// `site_config` is the site's `liyasa.json`, for the server's name and
/// description; `Value::Null` is accepted and gives the defaults, so a caller
/// that cannot find the config still gets a working server rather than none.
pub fn open(dist: &Path, site_config: &Value) -> std::io::Result<Arc<BundleReader>> {
    let bundle = Bundle::open(dist)?;
    let info = super::site_info(site_config);
    Ok(Arc::new(BundleReader::new(Arc::new(bundle), info)))
}

/// Serves until `input` ends, which is how a client says it is finished.
pub async fn run<I, O>(reader: Arc<BundleReader>, input: I, output: O) -> std::io::Result<()>
where
    I: AsyncBufRead + Unpin,
    O: AsyncWrite + Unpin,
{
    let mut lines = BufReader::new(input).lines();
    let mut output = output;
    while let Some(line) = lines.next_line().await? {
        // A shell or a wrapper inserts blank lines; treating one as a parse
        // error would answer a message nobody sent.
        if line.trim().is_empty() {
            continue;
        }
        if let Some(answer) = answer(reader.clone(), &line).await {
            output.write_all(answer.as_bytes()).await?;
            output.write_all(b"\n").await?;
            // Flushed per message, not per batch: a client blocks on the
            // response to the request it just sent, so a buffered answer is
            // a hang rather than a delay.
            output.flush().await?;
        }
    }
    Ok(())
}

/// One message in, one line of JSON out — or `None` for a notification.
///
/// Separate from [`serve`] so the transport can be tested without a process:
/// a test that had to spawn `liyasa mcp` would be measuring the harness.
pub async fn answer(reader: Arc<BundleReader>, line: &str) -> Option<String> {
    let server = Server {
        host: Host {
            reader: reader.as_ref(),
            // MCP-04: no vectors in `dist/`, and no server to file against.
            assistant: None,
            issues: None,
        },
        scope: Scope::anonymous(),
    };
    let incoming = super::jsonrpc::parse(line);
    let response = match &incoming {
        Incoming::Notify(_) => return None,
        Incoming::Refuse(response) => (**response).clone(),
        Incoming::Call(request) => protocol::dispatch(&server, request).await?,
    };
    // A message with a newline in it would frame as two, and the second one
    // would not parse. `to_string` never emits one, and this says why it
    // must not be swapped for `to_string_pretty`.
    Some(serde_json::to_string(&response).unwrap_or_else(|error| {
        format!(
            r#"{{"jsonrpc":"2.0","id":null,"error":{{"code":{},"message":{}}}}}"#,
            super::jsonrpc::INTERNAL_ERROR,
            Value::String(error.to_string())
        )
    }))
}
