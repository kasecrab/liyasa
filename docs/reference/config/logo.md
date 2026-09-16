---
title: logo
description: "The `logo` setting."
sidebarTitle: logo
---

# `logo`

The `logo` setting.

Specified by CFG-02.

| Key | Type | Default | What it does |
|---|---|---|---|
| `logo.dark` | string | — | — |
| `logo.href` | string | — | Click target for the logo. |
| `logo.light` | string | — | — |

Every key above is generated from `schemas/liyasa.schema.json`, which is the single source of truth for configuration: a key that is not in the schema is [`E0110`](/errors/E0110), and a value that does not match it is [`E0102`](/errors/E0102).

See [the configuration reference](/reference/config) for the other sections.
