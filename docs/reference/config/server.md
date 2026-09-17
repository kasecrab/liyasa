---
title: server
description: "Server-mode settings that are not secrets; only meaningful to `liyasa serve`."
sidebarTitle: server
---

# `server`

Server-mode settings that are not secrets; only meaningful to `liyasa serve`.

Specified by CFG-84.

| Key | Type | Default | What it does |
|---|---|---|---|
| `server.builds.concurrencyPerProject` | integer | `1` | How many builds of one project run at once. Above one, two builds of the same project compete for the same cache. |
| `server.builds.cpu` | integer | `4` | CPU cores one build may use. |
| `server.builds.memory` | string | — | A byte size such as `512MB`. |
| `server.builds.queue` | integer | `100` | How many builds may wait before one is refused. |
| `server.drainTimeout` | string | — | A duration such as `500ms`, `30s`, `180d`. |
| `server.dynamic.concurrency` | integer \| `cores` | — | How many on-demand renders run at once. |
| `server.dynamic.queue` | integer | `256` | How many renders may wait before further requests are refused rather than queued behind a long line. |
| `server.dynamic.queueTimeout` | string | — | A duration such as `500ms`, `30s`, `180d`. |
| `server.dynamic.subjectTtl` | string | — | A duration such as `500ms`, `30s`, `180d`. |
| `server.jobs.leaseSeconds` | integer | `60` | How long a worker holds a job before another may take it, which is what stops a crashed worker from stranding work. |
| `server.offline` | boolean | `false` | Run with no outbound network at all: no model calls, no spec fetches, no link checking. |
| `server.rateLimits.agentPages` | integer | — | Requests for the Markdown twins and the agent surfaces, which are cheap to serve and meant to be read in bulk. |
| `server.rateLimits.assistant` | integer | — | Assistant questions, which cost a model call each. |
| `server.rateLimits.auth` | integer | — | Sign-in attempts, which is the limit that matters for guessing passwords. |
| `server.rateLimits.feedback` | integer | — | Feedback submissions. |
| `server.rateLimits.mcp` | integer | — | Requests to the MCP server. |
| `server.rateLimits.pages` | integer | — | Page requests. |
| `server.rateLimits.proxy` | integer | — | Requests through the playground's proxy. |
| `server.rateLimits.rest` | integer | — | Requests to the REST API. |
| `server.rateLimits.search` | integer | — | Search queries. |
| `server.trustedProxies` | string[] | — | CIDRs whose forwarded headers are believed. Until a peer is listed here, `X-Forwarded-For`, `X-Real-IP`, `CF-Connecting-IP`, and the region headers are ignored, because anyone can send them. |
| `server.variantCache.bytes` | string | — | A byte size such as `512MB`. |
| `server.variantCache.entries` | integer | `10000` | How many variants the cache holds before it evicts the least recently used. |

Every key above is generated from `schemas/liyasa.schema.json`, which is the single source of truth for configuration: a key that is not in the schema is [`E0103`](/errors/E0103), and a value that does not match it is [`E0102`](/errors/E0102).

See [the configuration reference](/reference/config) for the other sections.
