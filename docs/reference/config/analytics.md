---
title: analytics
description: "Collection, retention, and bot classification (§26)."
sidebarTitle: analytics
---

# `analytics`

Collection, retention, and bot classification (§26).

Specified by CFG-98.

| Key | Type | Default | What it does |
|---|---|---|---|
| `analytics.botList` | string | `"builtin"` | — |
| `analytics.collector` | any | — | Static sites: URL of a collector (ANA-09). |
| `analytics.db` | string | — | Path or URL of the analytics database (§6.8). |
| `analytics.enabled` | boolean | `true` | — |
| `analytics.identityLinkage` | boolean | `true` | — |
| `analytics.rawSink` | `database` \| `files` | — | — |
| `analytics.retention.aggregateMonths` | integer | `13` | — |
| `analytics.retention.rawDays` | integer | `90` | — |

Every key above is generated from `schemas/liyasa.schema.json`, which is the single source of truth for configuration: a key that is not in the schema is [`E0110`](/errors/E0110), and a value that does not match it is [`E0102`](/errors/E0102).

See [the configuration reference](/reference/config) for the other sections.
