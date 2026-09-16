---
title: verify
description: "Verification engine settings (§14)."
sidebarTitle: verify
---

# `verify`

Verification engine settings (§14).

Specified by CFG-89, VER-76.

| Key | Type | Default | What it does |
|---|---|---|---|
| `verify.budget.deploy` | string | — | A duration such as `500ms`, `30s`, `180d`. |
| `verify.budget.perCheck` | string | — | A duration such as `500ms`, `30s`, `180d`. |
| `verify.budget.total` | string | — | A duration such as `500ms`, `30s`, `180d`. |
| `verify.drift.autoResolve` | boolean | — | — |
| `verify.drift.batchSize` | integer | — | — |
| `verify.enabled` | boolean | — | — |
| `verify.facts` | object | — | — |
| `verify.http.target` | string | — | Base URL HTTP checks run against. |
| `verify.links.external` | boolean | — | — |
| `verify.links.grace` | string | — | A duration such as `500ms`, `30s`, `180d`. |
| `verify.policy` | any | — | — |
| `verify.prose` | any | — | — |
| `verify.report` | any | — | — |
| `verify.runners` | object | — | — |
| `verify.sandbox` | any | — | — |
| `verify.schedule` | any | — | — |
| `verify.screenshots` | any | — | — |
| `verify.sources.commands.allow` | string[] | — | — |

Every key above is generated from `schemas/liyasa.schema.json`, which is the single source of truth for configuration: a key that is not in the schema is [`E0110`](/errors/E0110), and a value that does not match it is [`E0102`](/errors/E0102).

See [the configuration reference](/reference/config) for the other sections.
