---
title: api
description: "Manual API pages (API-20)."
sidebarTitle: api
---

# `api`

Manual API pages (API-20).

Specified by CFG-88.

| Key | Type | Default | What it does |
|---|---|---|---|
| `api.auth.in` | `header` \| `query` \| `cookie` | — | — |
| `api.auth.method` | `bearer` \| `basic` \| `apiKey` \| `none` | — | — |
| `api.auth.name` | string | — | — |
| `api.baseUrl` | string | — | — |

Every key above is generated from `schemas/liyasa.schema.json`, which is the single source of truth for configuration: a key that is not in the schema is [`E0110`](/errors/E0110), and a value that does not match it is [`E0102`](/errors/E0102).

See [the configuration reference](/reference/config) for the other sections.
