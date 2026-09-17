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
| `verify.drift.autoResolve` | boolean | — | Close a drift finding automatically once the page and its source agree again. |
| `verify.drift.batchSize` | integer | — | How many pages one drift pass examines. |
| `verify.enabled` | boolean | — | Whether verification runs at all for this site. |
| `verify.facts` | object | — | Accepted by the schema and read by nothing. RFC 1302 records that neither VER-76 nor the schema says what it should contain. |
| `verify.http.target` | string | — | Base URL HTTP checks run against. |
| `verify.links.external` | boolean | — | Check links that leave the site. Off, only internal links are checked, which needs no network. |
| `verify.links.grace` | string | — | A duration such as `500ms`, `30s`, `180d`. |
| `verify.policy` | any | — | What each kind of finding costs, keyed by kind (`code`, `facts`, `links`, `screenshots`) and valued by severity. A kind set to `error` fails the build when `build.strictVerification` is on. |
| `verify.prose` | any | — | Accepted by the schema and read by nothing. RFC 1302 records that neither VER-76 nor the schema says what it should contain. |
| `verify.report` | any | — | Accepted by the schema and read by nothing. RFC 1302 records that neither VER-76 nor the schema says what it should contain. |
| `verify.runners` | object | — | The runners checks execute in, keyed by name. |
| `verify.sandbox` | any | — | Accepted by the schema and read by nothing; `runners.sandbox` is the key that carries the sandbox (RFC 1302). |
| `verify.schedule` | any | — | When verification runs on its own, as a cron expression, for the checks that are too slow to run on every build. |
| `verify.screenshots` | any | — | Screenshot comparison settings, including how much a screenshot may differ before it is a finding. |
| `verify.sources.commands.allow` | string[] | — | The command allow list. A fact that runs anything else is refused rather than executed. |

Every key above is generated from `schemas/liyasa.schema.json`, which is the single source of truth for configuration: a key that is not in the schema is [`E0103`](/errors/E0103), and a value that does not match it is [`E0102`](/errors/E0102).

See [the configuration reference](/reference/config) for the other sections.
