---
title: regions
description: "Region gating (§19.6)."
sidebarTitle: regions
---

# `regions`

Region gating (§19.6).

Specified by CFG-100.

| Key | Type | Default | What it does |
|---|---|---|---|
| `regions.availability` | string | — | — |
| `regions.default` | string | — | — |
| `regions.detection` | `auth` \| `header` \| `choice`[] | — | — |
| `regions.enabled` | boolean | `false` | — |
| `regions.header` | string | — | — |
| `regions.list` | string[] | — | — |

Every key above is generated from `schemas/liyasa.schema.json`, which is the single source of truth for configuration: a key that is not in the schema is [`E0110`](/errors/E0110), and a value that does not match it is [`E0102`](/errors/E0102).

See [the configuration reference](/reference/config) for the other sections.
