---
title: pageActions
description: "Copy, view, and open-in-assistant actions (§8.8)."
sidebarTitle: pageActions
---

# `pageActions`

Copy, view, and open-in-assistant actions (§8.8).

Specified by CFG-72.

| Key | Type | Default | What it does |
|---|---|---|---|
| `pageActions.exclude` | string[] | — | Built-in actions to drop, so the rest of the default list can stay untouched. |
| `pageActions.items` | (string \| object)[] | — | The actions offered on every page, in the order they appear. |
| `pageActions.placement` | `header` \| `sidebar` \| `both` | — | Where the actions sit: above the page, in the sidebar, or both. |

Every key above is generated from `schemas/liyasa.schema.json`, which is the single source of truth for configuration: a key that is not in the schema is [`E0103`](/errors/E0103), and a value that does not match it is [`E0102`](/errors/E0102).

See [the configuration reference](/reference/config) for the other sections.
