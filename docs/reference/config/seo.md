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
| `seo.canonicalOrigin` | string | — | The production origin, as scheme, host, and port: `https://docs.example.com`. Every absolute URL in `llms.txt`, the Markdown twins, the feeds, and the sitemap is built from it, and the build warns when it is missing. It does not carry the path the site is served under — that is `build.basePath`, and writing it in both doubles it on every URL (W0136). |
| `seo.crawlers` | object | — | Per-bot directives, keyed by user agent (`GPTBot`, `ClaudeBot`, `Google-Extended`, `PerplexityBot`, `CCBot`, `Bytespider`, and the rest). They become user-agent groups in `robots.txt` and a section of `llms.txt`. The default allows everyone: agents reading the docs is the point. |
| `seo.indexing` | `navigable` \| `all` | — | `navigable` makes only pages reachable from the navigation indexable; `all` indexes every page the build produces. |
| `seo.metatags` | object | — | Meta tags added to every page, keyed by name. A page's own front matter overrides one of them. |
| `seo.organization` | any | — | A schema.org Organization, emitted as JSON-LD on every page. Pages also emit TechArticle and BreadcrumbList without being asked. |
| `seo.robots` | any | — | Extra rules appended to the generated `robots.txt`. |
| `seo.sitemap` | boolean \| any | — | Whether a sitemap is written, and how. |
| `seo.trailingSlash` | boolean | `false` | Whether a route ends in a slash. The server redirects the other form with a 301, so only one of the two is canonical. |

Every key above is generated from `schemas/liyasa.schema.json`, which is the single source of truth for configuration: a key that is not in the schema is [`E0103`](/errors/E0103), and a value that does not match it is [`E0102`](/errors/E0102).

See [the configuration reference](/reference/config) for the other sections.
