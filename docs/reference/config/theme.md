---
title: theme
description: "Presets, colours, fonts, icons, appearance, layout, and overrides (§8.2)."
sidebarTitle: theme
---

# `theme`

Presets, colours, fonts, icons, appearance, layout, and overrides (§8.2).

Specified by CFG-03..CFG-10.

| Key | Type | Default | What it does |
|---|---|---|---|
| `theme.appearance.background.color` | string | — | — |
| `theme.appearance.background.decoration` | `none` \| `grid` \| `gradient` \| `windows` | — | — |
| `theme.appearance.background.image` | string | — | — |
| `theme.appearance.default` | `system` \| `light` \| `dark` | — | — |
| `theme.appearance.strict` | boolean | `false` | Hides the appearance toggle. |
| `theme.codeTheme.dark` | string | — | — |
| `theme.codeTheme.light` | string | — | — |
| `theme.colors.accent` | string | — | — |
| `theme.colors.background.dark` | string | — | — |
| `theme.colors.background.light` | string | — | — |
| `theme.colors.border` | string | — | — |
| `theme.colors.danger` | string | — | — |
| `theme.colors.dark` | string | — | — |
| `theme.colors.light` | string | — | — |
| `theme.colors.muted` | string | — | — |
| `theme.colors.primary` | string | — | — |
| `theme.colors.success` | string | — | — |
| `theme.colors.text` | string | — | — |
| `theme.colors.warning` | string | — | — |
| `theme.css` | string \| string[] | — | — |
| `theme.fonts.body.family` | string | — | — |
| `theme.fonts.body.format` | string | — | — |
| `theme.fonts.body.source` | string | — | A Google Fonts family, downloaded and self-hosted at build time, or a local file path. |
| `theme.fonts.body.weight` | any | — | — |
| `theme.fonts.heading.family` | string | — | — |
| `theme.fonts.heading.format` | string | — | — |
| `theme.fonts.heading.source` | string | — | A Google Fonts family, downloaded and self-hosted at build time, or a local file path. |
| `theme.fonts.heading.weight` | any | — | — |
| `theme.fonts.mono.family` | string | — | — |
| `theme.fonts.mono.format` | string | — | — |
| `theme.fonts.mono.source` | string | — | A Google Fonts family, downloaded and self-hosted at build time, or a local file path. |
| `theme.fonts.mono.weight` | any | — | — |
| `theme.fonts.subset` | boolean | `false` | Accepted and ignored with W0716 until the fontations subsetter exists. |
| `theme.icons.defaultType` | string | — | — |
| `theme.icons.library` | `lucide` \| `phosphor` \| `tabler` \| `fontawesome` | — | — |
| `theme.js` | string \| string[] | — | — |
| `theme.layout.contentWidth` | string | — | — |
| `theme.layout.density` | `comfortable` \| `compact` | — | — |
| `theme.layout.radius` | string | — | — |
| `theme.layout.shadows` | boolean | — | — |
| `theme.layout.sidebarWidth` | string | — | — |
| `theme.layout.tocWidth` | string | — | — |
| `theme.overrides` | string | — | Directory of theme partial overrides (§10). |
| `theme.preset` | `aurora` \| `atlas` \| `meadow` \| `slate` \| `ember` \| `harbor` \| `quill` \| `signal` \| `lumen` | `"aurora"` | — |

Every key above is generated from `schemas/liyasa.schema.json`, which is the single source of truth for configuration: a key that is not in the schema is [`E0110`](/errors/E0110), and a value that does not match it is [`E0102`](/errors/E0102).

See [the configuration reference](/reference/config) for the other sections.
