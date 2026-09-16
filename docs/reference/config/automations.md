---
title: automations
description: "Automation definitions and untrusted-run budgets."
sidebarTitle: automations
---

# `automations`

Automation definitions and untrusted-run budgets.

Specified by CFG-99.

| Key | Type | Default | What it does |
|---|---|---|---|
| `automations.definitions` | string | `"automations/"` | — |
| `automations.perSourceHourly` | integer | `1` | — |
| `automations.untrustedCostCentsPerDay` | integer | `500` | — |
| `automations.untrustedRunsPerDay` | integer | `20` | — |
| `automations.untrustedTokensPerDay` | integer | `500000` | — |

Every key above is generated from `schemas/liyasa.schema.json`, which is the single source of truth for configuration: a key that is not in the schema is [`E0110`](/errors/E0110), and a value that does not match it is [`E0102`](/errors/E0102).

See [the configuration reference](/reference/config) for the other sections.
