---
title: favicon
description: "The `favicon` setting."
sidebarTitle: favicon
---

# `favicon`

The `favicon` setting.

Specified by CFG-02.

| Key | Type | Default | What it does |
|---|---|---|---|
| `favicon.dark` | string | — | — |
| `favicon.href` | string | — | Click target for the logo. |
| `favicon.light` | string | — | — |

Every key above is generated from `schemas/liyasa.schema.json`, which is the single source of truth for configuration: a key that is not in the schema is [`E0110`](/errors/E0110), and a value that does not match it is [`E0102`](/errors/E0102).

See [the configuration reference](/reference/config) for the other sections.
