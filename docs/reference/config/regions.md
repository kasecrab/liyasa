---
title: regions
description: "Region gating (§19.6)."
sidebarTitle: regions
---

# `regions`

Region gating (§19.6).

Specified by CFG-100.

| Key | Type | Default | What it does |
|---|---|---|---|
| `regions.availability` | string | — | A facts file saying which features exist in which region, so a page can say so without hard-coding it. |
| `regions.default` | string | — | The region a reader is served when none can be determined. |
| `regions.detection` | (`auth` \| `header` \| `choice`)[] | — | How a reader's region is decided, in order: from their identity, from a request header, or from a choice they made. |
| `regions.enabled` | boolean | `false` | Serve region-specific content at all. |
| `regions.header` | string | — | The request header a region is read from, such as `CF-IPCountry`. It is only trusted from a peer in `server.trustedProxies`. |
| `regions.list` | string[] | — | The regions this site has content for. |

Every key above is generated from `schemas/liyasa.schema.json`, which is the single source of truth for configuration: a key that is not in the schema is [`E0103`](/errors/E0103), and a value that does not match it is [`E0102`](/errors/E0102).

See [the configuration reference](/reference/config) for the other sections.
