---
title: playground
description: "API playground behaviour (§13)."
sidebarTitle: playground
---

# `playground`

API playground behaviour (§13).

Specified by CFG-87.

| Key | Type | Default | What it does |
|---|---|---|---|
| `playground.display` | `interactive` \| `simple` \| `none` | — | — |
| `playground.languages` | string[] | — | — |
| `playground.proxy.allow` | string[] | — | — |
| `playground.proxy.enabled` | boolean | `false` | — |
| `playground.requiredOnly` | boolean | `false` | — |

Every key above is generated from `schemas/liyasa.schema.json`, which is the single source of truth for configuration: a key that is not in the schema is [`E0110`](/errors/E0110), and a value that does not match it is [`E0102`](/errors/E0102).

See [the configuration reference](/reference/config) for the other sections.
