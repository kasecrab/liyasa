---
title: localization
description: "Locale fallback and visitor routing (§7.11)."
sidebarTitle: localization
---

# `localization`

Locale fallback and visitor routing (§7.11).

Specified by CFG-89.

| Key | Type | Default | What it does |
|---|---|---|---|
| `localization.fallback` | `notice` \| `hide` | — | What a reader sees when a page has no translation in their locale: the default locale's text under a notice, or nothing at all. |
| `localization.routeVisitors` | boolean | `false` | Send a reader to the locale their browser asks for. Off, a reader stays where they navigated. |

Every key above is generated from `schemas/liyasa.schema.json`, which is the single source of truth for configuration: a key that is not in the schema is [`E0103`](/errors/E0103), and a value that does not match it is [`E0102`](/errors/E0102).

See [the configuration reference](/reference/config) for the other sections.
