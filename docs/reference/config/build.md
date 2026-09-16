---
title: build
description: "Build outputs, budgets, caps, and determinism inputs (§6.6)."
sidebarTitle: build
---

# `build`

Build outputs, budgets, caps, and determinism inputs (§6.6).

Specified by CFG-83.

| Key | Type | Default | What it does |
|---|---|---|---|
| `build.basePath` | string | `""` | — |
| `build.budget.template` | string | — | A duration such as `500ms`, `30s`, `180d`. |
| `build.budget.templateIncremental` | string | — | A duration such as `500ms`, `30s`, `180d`. |
| `build.budget.total` | string | — | A duration such as `500ms`, `30s`, `180d`. |
| `build.cacheSize` | string | — | A byte size such as `512MB`. |
| `build.downloads.pdf` | `inline` \| `attachment` | — | — |
| `build.downloads.zip` | `inline` \| `attachment` | — | — |
| `build.drafts` | boolean | `false` | — |
| `build.env` | string[] | — | — |
| `build.hashing` | `filename` \| `query` \| `none` | `"filename"` | — |
| `build.images.eager` | boolean | `false` | — |
| `build.maxVariants` | integer | `10000` | — |
| `build.maxVariantsPerPage` | integer | `16` | — |
| `build.minify` | boolean | `true` | — |
| `build.output` | string | `"dist"` | — |
| `build.prefetch` | boolean | `true` | — |
| `build.strictLinks` | boolean | `true` | — |
| `build.strictVerification` | boolean | `true` | — |
| `build.variantDiscoveryIterations` | integer | `4` | — |

Every key above is generated from `schemas/liyasa.schema.json`, which is the single source of truth for configuration: a key that is not in the schema is [`E0110`](/errors/E0110), and a value that does not match it is [`E0102`](/errors/E0102).

See [the configuration reference](/reference/config) for the other sections.
