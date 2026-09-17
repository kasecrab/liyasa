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
| `navbar.primary.href` | string | — | Where the button goes. For the `github` form, the repository URL whose stars are counted. |
| `navbar.primary.label` | string | — | The button's text. Unused by the `github` form, which labels itself. |
| `navbar.primary.type` | `button` \| `github` | — | `button` renders `label` and `href` as written; `github` renders the repository link with its star count, fetched at build time and cached, never from the reader's browser. |

Every key above is generated from `schemas/liyasa.schema.json`, which is the single source of truth for configuration: a key that is not in the schema is [`E0103`](/errors/E0103), and a value that does not match it is [`E0102`](/errors/E0102).

See [the configuration reference](/reference/config) for the other sections.
