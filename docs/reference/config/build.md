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
| `build.basePath` | string | `""` | URL prefix the site is served under, such as `/docs`. It prefixes the links, assets, Markdown twins, and agent surfaces in the output; it does not change where the files are written. It is the only place the prefix belongs: leave it out of `seo.canonicalOrigin`. |
| `build.budget.template` | string | — | A duration such as `500ms`, `30s`, `180d`. |
| `build.budget.templateIncremental` | string | — | A duration such as `500ms`, `30s`, `180d`. |
| `build.budget.total` | string | — | A duration such as `500ms`, `30s`, `180d`. |
| `build.cacheSize` | string | — | A byte size such as `512MB`. |
| `build.downloads.pdf` | `inline` \| `attachment` | — | `inline` opens a PDF in the browser; `attachment` downloads it. |
| `build.downloads.zip` | `inline` \| `attachment` | — | `inline` opens an archive in the browser; `attachment` downloads it. |
| `build.drafts` | boolean | `false` | Include pages marked `draft: true`. Off, they are built by `dev` and left out of `build`. |
| `build.env` | string[] | — | The environment variables a template may read. Nothing outside this list is visible to `env()`, and each value is part of the build's fingerprint, so changing one rebuilds the pages that read it. |
| `build.hashing` | `filename` \| `query` \| `none` | `"filename"` | How an asset's URL carries its content hash: in the file name, as a query parameter, or not at all. `none` makes a build's file names stable at the cost of cacheability. |
| `build.images.eager` | boolean | `false` | Generate every image derivative during the build rather than on first request. Slower builds, no first-request cost. |
| `build.maxVariants` | integer | `10000` | How many variants the whole build may produce before it refuses. |
| `build.maxVariantsPerPage` | integer | `16` | How many variants one page may expand into before the build refuses. A variant is one combination of the dimensions that page actually reads, so a page that reads none has exactly one. |
| `build.minify` | boolean | `true` | Minify the HTML, CSS, and JavaScript the build emits. |
| `build.output` | string | `"dist"` | Directory the built site is written to, relative to the project root. |
| `build.prefetch` | boolean | `true` | Prefetch a page when a reader's pointer rests on a link to it. |
| `build.strictLinks` | boolean | `true` | Whether a link to a page or file that does not exist fails the build. Off, it is a warning and the link is left as written. |
| `build.strictVerification` | boolean | `true` | Whether a failed verification check fails the build. |
| `build.variantDiscoveryIterations` | integer | `4` | How many passes the build makes over a page to discover what it reads. A page whose variables depend on other variables needs more than one. |

Every key above is generated from `schemas/liyasa.schema.json`, which is the single source of truth for configuration: a key that is not in the schema is [`E0103`](/errors/E0103), and a value that does not match it is [`E0102`](/errors/E0102).

See [the configuration reference](/reference/config) for the other sections.
