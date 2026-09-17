---
title: feeds
description: "Changelog and update feeds."
sidebarTitle: feeds
---

# `feeds`

Changelog and update feeds.

Specified by CFG-89.

| Key | Type | Default | What it does |
|---|---|---|---|
| `feeds.changelog` | boolean | `true` | Publish the changelog as RSS, Atom, and JSON feeds. |
| `feeds.updates` | boolean | `false` | Publish a feed of every page that changed, not only the changelog. |

Every key above is generated from `schemas/liyasa.schema.json`, which is the single source of truth for configuration: a key that is not in the schema is [`E0103`](/errors/E0103), and a value that does not match it is [`E0102`](/errors/E0102).

See [the configuration reference](/reference/config) for the other sections.
