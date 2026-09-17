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
| `social.thumbnails.appearance` | string | — | `dark` draws the card from the dark scheme's tokens; anything else uses the light scheme's. |
| `social.thumbnails.background` | string | — | Card background, as any CSS colour. Absent, the theme's page background is used. |
| `social.thumbnails.font` | string | — | CSS font stack the card's text is set in. |
| `social.thumbnails.logo` | string | — | A logo placed in the card's corner, as a path or a data URI. |

Every key above is generated from `schemas/liyasa.schema.json`, which is the single source of truth for configuration: a key that is not in the schema is [`E0103`](/errors/E0103), and a value that does not match it is [`E0102`](/errors/E0102).

See [the configuration reference](/reference/config) for the other sections.
