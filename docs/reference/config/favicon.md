---
title: favicon
description: "The site favicon, per colour scheme. Absent, one is generated from the logo."
sidebarTitle: favicon
---

# `favicon`

The site favicon, per colour scheme. Absent, one is generated from the logo.

Specified by CFG-02.

| Key | Type | Default | What it does |
|---|---|---|---|
| `favicon.dark` | string | — | Path or URL of the favicon a browser in the dark scheme asks for, for the browsers that ask for one. |
| `favicon.href` | string | — | Click target for the logo. |
| `favicon.light` | string | — | Path or URL of the favicon a browser in the light scheme asks for; also the fallback when `dark` is absent. |

Every key above is generated from `schemas/liyasa.schema.json`, which is the single source of truth for configuration: a key that is not in the schema is [`E0103`](/errors/E0103), and a value that does not match it is [`E0102`](/errors/E0102).

See [the configuration reference](/reference/config) for the other sections.
