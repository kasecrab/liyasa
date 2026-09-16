---
title: banner
description: "Site-wide banner (§8.8)."
sidebarTitle: banner
---

# `banner`

Site-wide banner (§8.8).

Specified by CFG-70.

| Key | Type | Default | What it does |
|---|---|---|---|
| `banner.by_locale` | object | — | — |
| `banner.content` | string | — | — |
| `banner.dismissible` | boolean | — | — |
| `banner.end` | string | — | — |
| `banner.id` | string | — | — |
| `banner.start` | string | — | — |

Every key above is generated from `schemas/liyasa.schema.json`, which is the single source of truth for configuration: a key that is not in the schema is [`E0110`](/errors/E0110), and a value that does not match it is [`E0102`](/errors/E0102).

See [the configuration reference](/reference/config) for the other sections.
