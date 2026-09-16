---
title: network
description: "Outbound network allow lists, consumed by `liyasa-net` (§30.2.3)."
sidebarTitle: network
---

# `network`

Outbound network allow lists, consumed by `liyasa-net` (§30.2.3).

Specified by CFG-86.

| Key | Type | Default | What it does |
|---|---|---|---|
| `network.allowHosts.agentFetch` | string[] | — | — |
| `network.allowHosts.embeds` | string[] | — | — |
| `network.allowHosts.factSources` | string[] | — | — |
| `network.allowHosts.specRefs` | string[] | — | — |
| `network.allowInsecureHosts` | string[] | — | — |
| `network.allowPrivate` | string[] | — | — |
| `network.denyHosts` | string[] | — | — |
| `network.egressProxy` | any | — | Cloud only. |

Every key above is generated from `schemas/liyasa.schema.json`, which is the single source of truth for configuration: a key that is not in the schema is [`E0110`](/errors/E0110), and a value that does not match it is [`E0102`](/errors/E0102).

See [the configuration reference](/reference/config) for the other sections.
