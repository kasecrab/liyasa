---
title: navbar
description: "Top navigation bar (§8.3)."
sidebarTitle: navbar
---

# `navbar`

Top navigation bar (§8.3).

Specified by CFG-20..CFG-22.

| Key | Type | Default | What it does |
|---|---|---|---|
| `navbar.items` | any[] | — | CFG-22: dropdowns and switcher placement. |
| `navbar.links` | object[] | — | CFG-20 |
| `navbar.primary.href` | string | — | — |
| `navbar.primary.label` | string | — | — |
| `navbar.primary.type` | `button` \| `github` | — | — |

Every key above is generated from `schemas/liyasa.schema.json`, which is the single source of truth for configuration: a key that is not in the schema is [`E0110`](/errors/E0110), and a value that does not match it is [`E0102`](/errors/E0102).

See [the configuration reference](/reference/config) for the other sections.
