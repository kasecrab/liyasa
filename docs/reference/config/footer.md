---
title: footer
description: "Footer socials, link columns, branding, and legal line (§8.5)."
sidebarTitle: footer
---

# `footer`

Footer socials, link columns, branding, and legal line (§8.5).

Specified by CFG-40..CFG-43.

| Key | Type | Default | What it does |
|---|---|---|---|
| `footer.branding` | boolean | `true` | `true` shows "Built with Liyasa". |
| `footer.links` | object[] | — | — |
| `footer.socials` | object | — | — |
| `footer.text` | string | — | Legal line; Markdown. |

Every key above is generated from `schemas/liyasa.schema.json`, which is the single source of truth for configuration: a key that is not in the schema is [`E0110`](/errors/E0110), and a value that does not match it is [`E0102`](/errors/E0102).

See [the configuration reference](/reference/config) for the other sections.
