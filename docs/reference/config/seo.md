---
title: seo
description: "Meta tags, JSON-LD, indexing, sitemap, robots, and crawler directives (§8.7)."
sidebarTitle: seo
---

# `seo`

Meta tags, JSON-LD, indexing, sitemap, robots, and crawler directives (§8.7).

Specified by CFG-60..CFG-65.

| Key | Type | Default | What it does |
|---|---|---|---|
| `seo.canonicalOrigin` | string | — | Production origin used for absolute URLs in `llms.txt`, feeds, and the Markdown directive. |
| `seo.crawlers` | object | — | — |
| `seo.indexing` | `navigable` \| `all` | — | — |
| `seo.metatags` | object | — | — |
| `seo.organization` | any | — | — |
| `seo.robots` | any | — | — |
| `seo.sitemap` | boolean \| any | — | — |
| `seo.trailingSlash` | boolean | `false` | — |

Every key above is generated from `schemas/liyasa.schema.json`, which is the single source of truth for configuration: a key that is not in the schema is [`E0110`](/errors/E0110), and a value that does not match it is [`E0102`](/errors/E0102).

See [the configuration reference](/reference/config) for the other sections.
