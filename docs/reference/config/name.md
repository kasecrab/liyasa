---
title: name
description: "Site name. The only required key."
sidebarTitle: name
---

# `name`

Site name. The only required key.

Specified by CFG-01.

| Type | Default |
|---|---|
| string | — |

Every key above is generated from `schemas/liyasa.schema.json`, which is the single source of truth for configuration: a key that is not in the schema is [`E0110`](/errors/E0110), and a value that does not match it is [`E0102`](/errors/E0102).

See [the configuration reference](/reference/config) for the other sections.
