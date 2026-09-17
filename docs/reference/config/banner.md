---
title: banner
description: "Site-wide banner (§8.8)."
sidebarTitle: banner
---

# `banner`

Site-wide banner (§8.8).

Specified by CFG-70.

| Key | Type | Default | What it does |
|---|---|---|---|
| `banner.by_locale` | object | — | Per-locale text, keyed by locale code, used instead of `content` for a reader in that locale. |
| `banner.content` | string | — | The banner's text, as Markdown. It is rendered at build time, so a banner costs the reader no script. |
| `banner.dismissible` | boolean | — | Whether a reader may close the banner. A dismissal is remembered against `id`. |
| `banner.end` | string | — | When the banner stops showing, as `YYYY-MM-DD` or an RFC 3339 timestamp. An expired banner is absent from the output rather than hidden by a script. |
| `banner.id` | string | — | What a dismissal is remembered against. Change it to show a closed banner again; absent, the content is its own identity. |
| `banner.start` | string | — | When the banner begins showing, as `YYYY-MM-DD` or an RFC 3339 timestamp. Compared against the build clock, not the reader's, so an unstarted banner is simply absent from the output. |

Every key above is generated from `schemas/liyasa.schema.json`, which is the single source of truth for configuration: a key that is not in the schema is [`E0103`](/errors/E0103), and a value that does not match it is [`E0102`](/errors/E0102).

See [the configuration reference](/reference/config) for the other sections.
