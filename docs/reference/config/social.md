---
title: social
description: "Open Graph thumbnail generation (§8.8)."
sidebarTitle: social
---

# `social`

Open Graph thumbnail generation (§8.8).

Specified by CFG-73.

| Key | Type | Default | What it does |
|---|---|---|---|
| `social.thumbnails.appearance` | string | — | — |
| `social.thumbnails.background` | string | — | — |
| `social.thumbnails.font` | string | — | — |
| `social.thumbnails.logo` | string | — | — |

Every key above is generated from `schemas/liyasa.schema.json`, which is the single source of truth for configuration: a key that is not in the schema is [`E0110`](/errors/E0110), and a value that does not match it is [`E0102`](/errors/E0102).

See [the configuration reference](/reference/config) for the other sections.
