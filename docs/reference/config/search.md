---
title: search
description: "Search behaviour and the browser index's shard sizing (§8.6)."
sidebarTitle: search
---

# `search`

Search behaviour and the browser index's shard sizing (§8.6).

Specified by CFG-50..CFG-55.

| Key | Type | Default | What it does |
|---|---|---|---|
| `search.boost` | object[] | — | — |
| `search.exclude` | string[] | — | — |
| `search.filters` | (`tab` \| `version` \| `locale` \| `type`)[] | — | — |
| `search.maxResults` | integer | `20` | — |
| `search.mode` | `keyword` \| `hybrid` | — | — |
| `search.placeholder` | string | — | — |
| `search.shardSize.max` | string | — | A byte size such as `512MB`. |
| `search.shardSize.min` | string | — | A byte size such as `512MB`. |
| `search.shortcut` | string | `"mod+k"` | — |
| `search.snippets` | boolean | `true` | — |

Every key above is generated from `schemas/liyasa.schema.json`, which is the single source of truth for configuration: a key that is not in the schema is [`E0110`](/errors/E0110), and a value that does not match it is [`E0102`](/errors/E0102).

See [the configuration reference](/reference/config) for the other sections.
