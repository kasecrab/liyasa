---
title: security
description: "Content Security Policy, frame ancestors, and upload rules (§30.2)."
sidebarTitle: security
---

# `security`

Content Security Policy, frame ancestors, and upload rules (§30.2).

Specified by CFG-85.

| Key | Type | Default | What it does |
|---|---|---|---|
| `security.csp.extraFrameSrc` | string[] | — | — |
| `security.csp.extraImgSrc` | string[] | — | — |
| `security.csp.extraScriptSrc` | string[] | — | — |
| `security.csp.imgHostsAllow` | string[] | — | — |
| `security.csp.reportUri` | string | — | — |
| `security.frameAncestors` | string[] | — | — |
| `security.styleAttribute` | `allowlist` \| `off` | — | — |
| `security.txt` | boolean \| object | — | RFC 9116 `security.txt` fields, emitted at `/.well-known/security.txt`. |
| `security.uploads.allowTypes` | string[] | — | — |
| `security.uploads.svg` | `sanitize` \| `attachment` | — | — |

Every key above is generated from `schemas/liyasa.schema.json`, which is the single source of truth for configuration: a key that is not in the schema is [`E0110`](/errors/E0110), and a value that does not match it is [`E0102`](/errors/E0102).

See [the configuration reference](/reference/config) for the other sections.
