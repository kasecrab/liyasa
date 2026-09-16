---
title: auth
description: "Authentication modes and session policy (§19)."
sidebarTitle: auth
---

# `auth`

Authentication modes and session policy (§19).

Specified by CFG-96.

| Key | Type | Default | What it does |
|---|---|---|---|
| `auth.jwt.algs` | string[] | — | — |
| `auth.jwt.aud` | string | — | — |
| `auth.jwt.groupsClaim` | string | — | — |
| `auth.jwt.iss` | string | — | — |
| `auth.jwt.jwksUrl` | string | — | — |
| `auth.jwt.localeClaim` | string | — | — |
| `auth.jwt.loginUrl` | string | — | — |
| `auth.jwt.regionClaim` | string | — | — |
| `auth.logout.redirect` | string | — | — |
| `auth.managed.allowDomains` | string[] | — | — |
| `auth.managed.magicLinkTtl` | string | — | A duration such as `500ms`, `30s`, `180d`. |
| `auth.mode` | `public` \| `password` \| `jwt` \| `oidc` \| `managed` | — | — |
| `auth.oidc.clientId` | string | — | — |
| `auth.oidc.groupsClaim` | string | — | — |
| `auth.oidc.issuer` | string | — | — |
| `auth.oidc.scopes` | string[] | — | — |
| `auth.password.argon2.iterations` | integer | `3` | — |
| `auth.password.argon2.memoryKiB` | integer | `65536` | — |
| `auth.password.argon2.parallelism` | integer | `1` | — |
| `auth.preview.protection` | `org` \| `password` \| `public` | — | — |
| `auth.session.cookieName` | string | — | — |
| `auth.session.idleTimeout` | string | — | A duration such as `500ms`, `30s`, `180d`. |
| `auth.session.maxAge` | string | — | A duration such as `500ms`, `30s`, `180d`. |

Every key above is generated from `schemas/liyasa.schema.json`, which is the single source of truth for configuration: a key that is not in the schema is [`E0110`](/errors/E0110), and a value that does not match it is [`E0102`](/errors/E0102).

See [the configuration reference](/reference/config) for the other sections.
