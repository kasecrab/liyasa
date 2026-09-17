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
| `theme.appearance.background.color` | string | — | A flat colour drawn behind the page, under any image. |
| `theme.appearance.background.decoration` | `none` \| `grid` \| `gradient` \| `windows` | — | A generated pattern drawn behind the page: a grid, a gradient, or window chrome. `none` leaves the background flat. |
| `theme.appearance.background.image` | string | — | Path or URL of an image drawn behind the page. |
| `theme.appearance.default` | `system` \| `light` \| `dark` | — | The scheme a first-time reader sees. `system` follows the operating system and is applied before first paint, so there is no flash. |
| `theme.appearance.strict` | boolean | `false` | Hides the appearance toggle. |
| `theme.codeTheme.dark` | string | — | Bundled Shiki theme for the dark scheme, or `css-variables` to drive the colours yourself. |
| `theme.codeTheme.light` | string | — | Bundled Shiki theme for the light scheme, or `css-variables` to drive the colours yourself. |
| `theme.colors.accent` | string | — | A second accent for highlights that should not read as the primary action. |
| `theme.colors.background.dark` | string | — | Page background in the dark scheme. |
| `theme.colors.background.light` | string | — | Page background in the light scheme. |
| `theme.colors.border` | string | — | Rules and outlines: table borders, card edges, the line under a heading. |
| `theme.colors.danger` | string | — | The status colour for a destructive or error callout. |
| `theme.colors.dark` | string | — | The primary to use in the dark scheme, as written, instead of adapting `primary` to it. |
| `theme.colors.light` | string | — | The primary to use in the light scheme, as written, instead of adapting `primary` to it. |
| `theme.colors.muted` | string | — | Secondary text: captions, metadata, the parts of the chrome that should recede. |
| `theme.colors.primary` | string | — | The accent a reader acts on: buttons, links, focus rings. Adapted per scheme unless `light` and `dark` set their own, and reported with E0107 when no label colour on it clears WCAG AA. |
| `theme.colors.success` | string | — | The status colour for a successful or affirmative callout. |
| `theme.colors.text` | string | — | Body text on the page background, and the label the contrast check considers for a fill. |
| `theme.colors.warning` | string | — | The status colour for a cautionary callout. |
| `theme.css` | string \| string[] | — | Custom stylesheet or stylesheets, loaded after the theme's own so they win on equal specificity. |
| `theme.fonts.body.family` | string | — | CSS font family name, and the Google Fonts family to fetch when `source` says so. |
| `theme.fonts.body.format` | string | — | Font file format, such as `woff2`, when it cannot be inferred from the source. |
| `theme.fonts.body.source` | string | — | A Google Fonts family, downloaded and self-hosted at build time, or a local file path. |
| `theme.fonts.body.weight` | any | — | Weight or weight range to ship, as a CSS value such as `400` or `400 700`. Shipping fewer weights ships fewer bytes. |
| `theme.fonts.heading.family` | string | — | CSS font family name, and the Google Fonts family to fetch when `source` says so. |
| `theme.fonts.heading.format` | string | — | Font file format, such as `woff2`, when it cannot be inferred from the source. |
| `theme.fonts.heading.source` | string | — | A Google Fonts family, downloaded and self-hosted at build time, or a local file path. |
| `theme.fonts.heading.weight` | any | — | Weight or weight range to ship, as a CSS value such as `600` or `600 700`. Shipping fewer weights ships fewer bytes. |
| `theme.fonts.mono.family` | string | — | CSS font family name, and the Google Fonts family to fetch when `source` says so. |
| `theme.fonts.mono.format` | string | — | Font file format, such as `woff2`, when it cannot be inferred from the source. |
| `theme.fonts.mono.source` | string | — | A Google Fonts family, downloaded and self-hosted at build time, or a local file path. |
| `theme.fonts.mono.weight` | any | — | Weight or weight range to ship, as a CSS value such as `400`. Shipping fewer weights ships fewer bytes. |
| `theme.fonts.subset` | boolean | `false` | Accepted and ignored with W0716 until the fontations subsetter exists. |
| `theme.icons.defaultType` | string | — | The Font Awesome style a name with no style prefix takes, such as `solid` or `regular`. |
| `theme.icons.library` | `lucide` \| `phosphor` \| `tabler` \| `fontawesome` | — | The icon set a bare icon name is looked up in. Lucide, Phosphor, and Tabler ship as they are; Font Awesome Free ships with the attribution its licence requires. |
| `theme.js` | string \| string[] | — | Custom script or scripts, deferred after the theme runtime. |
| `theme.layout.contentWidth` | string | — | Maximum width of the prose column, as a CSS length. Wider fits more code; narrower reads better. |
| `theme.layout.density` | `comfortable` \| `compact` | — | How much air the chrome gets. `compact` tightens spacing and type scale for a dense reference site. |
| `theme.layout.radius` | string | — | Corner radius for cards, buttons, and inputs, as a CSS length. `0` squares everything off. |
| `theme.layout.shadows` | boolean | — | Whether elevated surfaces cast a shadow. Off, they are separated by borders alone. |
| `theme.layout.sidebarWidth` | string | — | Width of the navigation sidebar, as a CSS length. |
| `theme.layout.tocWidth` | string | — | Width of the on-page table of contents, as a CSS length. |
| `theme.overrides` | string | — | Directory of theme partial overrides (§10). |
| `theme.preset` | `aurora` \| `atlas` \| `meadow` \| `slate` \| `ember` \| `harbor` \| `quill` \| `signal` \| `lumen` | `"aurora"` | Layout preset: a complete set of tokens and partial overrides the rest of `theme` adjusts. Changing it changes the whole look, not one value. |

Every key above is generated from `schemas/liyasa.schema.json`, which is the single source of truth for configuration: a key that is not in the schema is [`E0103`](/errors/E0103), and a value that does not match it is [`E0102`](/errors/E0102).

See [the configuration reference](/reference/config) for the other sections.
