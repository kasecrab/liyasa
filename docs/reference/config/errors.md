---
title: errors
description: "Error page behaviour (§8.8)."
sidebarTitle: errors
---

# `errors`

Error page behaviour (§8.8).

Specified by CFG-71.

| Key | Type | Default | What it does |
|---|---|---|---|
| `errors.404.description` | string | — | Body of the 404 page, as Markdown. |
| `errors.404.page` | string | — | A page of your own to render instead, by path. |
| `errors.404.redirect` | boolean | `false` | Send the reader to the home page instead of showing a 404 page. Off by default, because a redirect hides the broken link that caused it. |
| `errors.404.title` | string | — | Heading on the 404 page. |

Every key above is generated from `schemas/liyasa.schema.json`, which is the single source of truth for configuration: a key that is not in the schema is [`E0103`](/errors/E0103), and a value that does not match it is [`E0102`](/errors/E0102).

See [the configuration reference](/reference/config) for the other sections.
