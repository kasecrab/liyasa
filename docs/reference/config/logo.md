---
title: logo
description: "The site logo, per colour scheme. Leave it out and a text logo is generated from `name`."
sidebarTitle: logo
---

# `logo`

The site logo, per colour scheme. Leave it out and a text logo is generated from `name`.

Specified by CFG-02.

| Key | Type | Default | What it does |
|---|---|---|---|
| `logo.dark` | string | — | Path or URL of the logo shown in the dark scheme. Absent, the light logo is used in both. |
| `logo.href` | string | — | Click target for the logo. |
| `logo.light` | string | — | Path or URL of the logo shown in the light scheme; also the fallback when `dark` is absent. |

Every key above is generated from `schemas/liyasa.schema.json`, which is the single source of truth for configuration: a key that is not in the schema is [`E0103`](/errors/E0103), and a value that does not match it is [`E0102`](/errors/E0102).

See [the configuration reference](/reference/config) for the other sections.
