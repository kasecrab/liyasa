---
title: errors
description: "Error page behaviour (§8.8)."
sidebarTitle: errors
---

# `errors`

Error page behaviour (§8.8).

Specified by CFG-71.

| Key | Type | Default | What it does |
|---|---|---|---|
| `errors.404.description` | string | — | — |
| `errors.404.page` | string | — | — |
| `errors.404.redirect` | boolean | `false` | — |
| `errors.404.title` | string | — | — |

Every key above is generated from `schemas/liyasa.schema.json`, which is the single source of truth for configuration: a key that is not in the schema is [`E0110`](/errors/E0110), and a value that does not match it is [`E0102`](/errors/E0102).

See [the configuration reference](/reference/config) for the other sections.
