---
title: content
description: "Markdown, templating, and image behaviour (§8.9)."
sidebarTitle: content
---

# `content`

Markdown, templating, and image behaviour (§8.9).

Specified by CFG-80.

| Key | Type | Default | What it does |
|---|---|---|---|
| `content.codeblocks.lineNumbers` | boolean | — | — |
| `content.codeblocks.theme` | string | — | — |
| `content.codeblocks.wrap` | boolean | — | — |
| `content.customElements` | `strip` \| `keep` | — | — |
| `content.frontmatter.strict` | boolean | `false` | Unknown front matter keys become warnings. |
| `content.html` | `allow` \| `sanitize` \| `off` | — | — |
| `content.images.breakpoints` | integer[] | — | — |
| `content.images.formats` | `avif` \| `webp` \| `png` \| `jpeg`[] | — | — |
| `content.lastModified` | boolean | — | CFG-74: show the git or editor timestamp on every page. |
| `content.math` | boolean \| `katex` \| `pulldown-latex` | — | — |
| `content.related.auto` | boolean | — | — |
| `content.reviewCadence` | string | — | A duration such as `500ms`, `30s`, `180d`. |
| `content.templating.limits.depth` | integer | — | — |
| `content.templating.limits.iterations` | integer | — | — |
| `content.templating.limits.outputBytes` | string | — | A byte size such as `512MB`. |
| `content.templating.undefined` | `strict` \| `lenient` \| `chainable` | — | — |
| `content.wikilinks` | boolean | `false` | — |

Every key above is generated from `schemas/liyasa.schema.json`, which is the single source of truth for configuration: a key that is not in the schema is [`E0110`](/errors/E0110), and a value that does not match it is [`E0102`](/errors/E0102).

See [the configuration reference](/reference/config) for the other sections.
