---
title: locales
description: "Languages this site is published in (§7.11)."
sidebarTitle: locales
---

# `locales`

Languages this site is published in (§7.11).

Specified by CFG-01.

| Type | Default |
|---|---|
| (string \| object)[] | — |

Every key above is generated from `schemas/liyasa.schema.json`, which is the single source of truth for configuration: a key that is not in the schema is [`E0103`](/errors/E0103), and a value that does not match it is [`E0102`](/errors/E0102).

See [the configuration reference](/reference/config) for the other sections.
