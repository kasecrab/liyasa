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
| `analytics.botList` | string | `"builtin"` | Which list of bot user agents is filtered out of the numbers. |
| `analytics.collector` | any | — | Static sites: URL of a collector (ANA-09). |
| `analytics.db` | string | — | Path or URL of the analytics database (§6.8). |
| `analytics.enabled` | boolean | `true` | Collect analytics at all. |
| `analytics.identityLinkage` | boolean | `true` | Link a reader's events to their identity when they are signed in. Off, every event is anonymous. |
| `analytics.rawSink` | `database` \| `files` | — | Where raw events are written before they are aggregated. |
| `analytics.retention.aggregateMonths` | integer | `13` | Months of aggregates to keep, which is what the dashboards read. |
| `analytics.retention.rawDays` | integer | `90` | Days of raw events to keep before they are dropped. |

Every key above is generated from `schemas/liyasa.schema.json`, which is the single source of truth for configuration: a key that is not in the schema is [`E0103`](/errors/E0103), and a value that does not match it is [`E0102`](/errors/E0102).

See [the configuration reference](/reference/config) for the other sections.
