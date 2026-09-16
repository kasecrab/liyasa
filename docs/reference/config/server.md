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
| `server.builds.concurrencyPerProject` | integer | `1` | — |
| `server.builds.cpu` | integer | `4` | — |
| `server.builds.memory` | string | — | A byte size such as `512MB`. |
| `server.builds.queue` | integer | `100` | — |
| `server.drainTimeout` | string | — | A duration such as `500ms`, `30s`, `180d`. |
| `server.dynamic.concurrency` | integer \| `cores` | — | — |
| `server.dynamic.queue` | integer | `256` | — |
| `server.dynamic.queueTimeout` | string | — | A duration such as `500ms`, `30s`, `180d`. |
| `server.dynamic.subjectTtl` | string | — | A duration such as `500ms`, `30s`, `180d`. |
| `server.jobs.leaseSeconds` | integer | `60` | — |
| `server.offline` | boolean | `false` | — |
| `server.rateLimits.agentPages` | integer | — | — |
| `server.rateLimits.assistant` | integer | — | — |
| `server.rateLimits.auth` | integer | — | — |
| `server.rateLimits.feedback` | integer | — | — |
| `server.rateLimits.mcp` | integer | — | — |
| `server.rateLimits.pages` | integer | — | — |
| `server.rateLimits.proxy` | integer | — | — |
| `server.rateLimits.rest` | integer | — | — |
| `server.rateLimits.search` | integer | — | — |
| `server.trustedProxies` | string[] | — | — |
| `server.variantCache.bytes` | string | — | A byte size such as `512MB`. |
| `server.variantCache.entries` | integer | `10000` | — |

Every key above is generated from `schemas/liyasa.schema.json`, which is the single source of truth for configuration: a key that is not in the schema is [`E0110`](/errors/E0110), and a value that does not match it is [`E0102`](/errors/E0102).

See [the configuration reference](/reference/config) for the other sections.
