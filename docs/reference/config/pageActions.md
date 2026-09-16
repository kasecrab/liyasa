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
| `pageActions.exclude` | string[] | — | — |
| `pageActions.items` | (string \| object)[] | — | — |
| `pageActions.placement` | `header` \| `sidebar` \| `both` | — | — |

Every key above is generated from `schemas/liyasa.schema.json`, which is the single source of truth for configuration: a key that is not in the schema is [`E0110`](/errors/E0110), and a value that does not match it is [`E0102`](/errors/E0102).

See [the configuration reference](/reference/config) for the other sections.
