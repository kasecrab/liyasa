---
title: integrations
description: "Third-party scripts and consent (§26.8)."
sidebarTitle: integrations
---

# `integrations`

Third-party scripts and consent (§26.8).

Specified by CFG-81.

| Key | Type | Default | What it does |
|---|---|---|---|
| `integrations.cookieConsent` | string \| any | — | — |
| `integrations.telemetry` | boolean | `false` | Liyasa's own anonymous CLI telemetry; off by default in OSS. |

Every key above is generated from `schemas/liyasa.schema.json`, which is the single source of truth for configuration: a key that is not in the schema is [`E0110`](/errors/E0110), and a value that does not match it is [`E0102`](/errors/E0102).

See [the configuration reference](/reference/config) for the other sections.
