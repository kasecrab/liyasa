//! The MCP server (MCP-01..05).
//!
//! A documentation site is read by agents at least as often as by people, and
//! an agent that has to scrape HTML gets a worse answer than one that can ask.
//! This subtree is the ask: the same corpus the reader sees, the same
//! entitlement filter, over JSON-RPC.
//!
//! ## Where it answers
//!
//! `/mcp`, and `/_liyasa/mcp` as an alias (RFC 1900). `/mcp` is canonical
//! because that is what MCP-01 says and what every generated `llms.txt`
//! already publishes to agents; the alias exists because `routes::pool_for`
//! charges the `Mcp` rate-limit bucket on the `/_liyasa/` spelling. Serving
//! only one of the two would leave either the advertised URL a 404 or agent
//! traffic charged to the human page pool.
//!
//! ## Layers
//!
//! - [`jsonrpc`] — the JSON-RPC 2.0 envelope. Knows nothing about MCP.
//! - [`protocol`] — what a method name means. Knows nothing about HTTP.
//! - [`tools`], [`resources`], [`prompts`] — the surface, as declarations.
//! - [`reader`] — the one trait a host supplies to answer them.
//!
//! The split is what lets the same surface serve both transports: `liyasa
//! serve` mounts it over Streamable HTTP, and `liyasa mcp --dist` (MCP-04)
//! runs the same dispatcher over stdio against a static build.

pub mod bundle_reader;
pub mod jsonrpc;
pub mod markdown;
pub mod openapi;
pub mod reader;
pub mod tools;
