---
title: public
description: "`false` requires an access mode under `auth` and a server; rejected with E0120 before 0.5."
sidebarTitle: public
---

# `public`

`false` requires an access mode under `auth` and a server; rejected with E0120 before 0.5.

Specified by CFG-01.

| Type | Default |
|---|---|
| boolean | `true` |

Every key above is generated from `schemas/liyasa.schema.json`, which is the single source of truth for configuration: a key that is not in the schema is [`E0103`](/errors/E0103), and a value that does not match it is [`E0102`](/errors/E0102).

See [the configuration reference](/reference/config) for the other sections.
