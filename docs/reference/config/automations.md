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
| `automations.definitions` | string | `"automations/"` | Directory the automation definitions are read from. A trust-plane path: an untrusted build reads it from the deploy branch. |
| `automations.perSourceHourly` | integer | `1` | How many runs a single source may cause in an hour, so one noisy source cannot use the whole day's budget. |
| `automations.untrustedCostCentsPerDay` | integer | `500` | How much untrusted runs may cost a day, in cents. |
| `automations.untrustedRunsPerDay` | integer | `20` | How many automation runs a day an untrusted source may cause. |
| `automations.untrustedTokensPerDay` | integer | `500000` | How many model tokens a day untrusted runs may spend in total. |

Every key above is generated from `schemas/liyasa.schema.json`, which is the single source of truth for configuration: a key that is not in the schema is [`E0103`](/errors/E0103), and a value that does not match it is [`E0102`](/errors/E0102).

See [the configuration reference](/reference/config) for the other sections.
