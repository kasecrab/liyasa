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
| `network.allowHosts.agentFetch` | string[] | — | Hosts the agent may fetch while it works. |
| `network.allowHosts.embeds` | string[] | — | Hosts a page may embed content from. |
| `network.allowHosts.factSources` | string[] | — | Hosts a fact may read its value from. |
| `network.allowHosts.specRefs` | string[] | — | Hosts an OpenAPI or AsyncAPI document may pull a reference from. |
| `network.allowInsecureHosts` | string[] | — | Hosts that may be reached over plain HTTP. |
| `network.allowPrivate` | string[] | — | Hosts or CIDRs that may be reached even though they are private addresses. Empty, a fetch to a private address is refused, which is what stops a config from reaching into the network the build runs on. |
| `network.denyHosts` | string[] | — | Hosts nothing may reach, whatever the allow lists say. |
| `network.egressProxy` | any | — | Cloud only. |

Every key above is generated from `schemas/liyasa.schema.json`, which is the single source of truth for configuration: a key that is not in the schema is [`E0103`](/errors/E0103), and a value that does not match it is [`E0102`](/errors/E0102).

See [the configuration reference](/reference/config) for the other sections.
