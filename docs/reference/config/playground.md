---
title: playground
description: "API playground behaviour (§13)."
sidebarTitle: playground
---

# `playground`

API playground behaviour (§13).

Specified by CFG-87.

| Key | Type | Default | What it does |
|---|---|---|---|
| `playground.display` | `interactive` \| `simple` \| `none` | — | How an endpoint's request appears: `interactive` lets a reader send it, `simple` shows it without sending, `none` shows the documentation alone. |
| `playground.languages` | string[] | — | Code samples to generate for each request, in the order they are shown; the first is the one a reader sees. |
| `playground.proxy.allow` | string[] | — | Hosts the proxy may forward to. Nothing else is forwarded, so the proxy cannot be used to reach anything else. |
| `playground.proxy.enabled` | boolean | `false` | Offer the proxy. |
| `playground.requiredOnly` | boolean | `false` | Show only the required parameters until a reader asks for the rest. |

Every key above is generated from `schemas/liyasa.schema.json`, which is the single source of truth for configuration: a key that is not in the schema is [`E0103`](/errors/E0103), and a value that does not match it is [`E0102`](/errors/E0102).

See [the configuration reference](/reference/config) for the other sections.
