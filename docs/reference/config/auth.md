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
| `auth.jwt.algs` | string[] | — | Signature algorithms a token may use. Anything else is refused, which is what stops an `alg: none` token. |
| `auth.jwt.aud` | string | — | The audience a token must claim, so a token minted for another service is not accepted here. |
| `auth.jwt.groupsClaim` | string | — | The claim a reader's access groups are read from, which is what page and navigation `groups` are matched against. |
| `auth.jwt.iss` | string | — | The issuer a token must claim. |
| `auth.jwt.jwksUrl` | string | — | Where the signing keys are published. |
| `auth.jwt.localeClaim` | string | — | The claim a reader's locale is read from. |
| `auth.jwt.loginUrl` | string | — | Where a reader without a token is sent to get one. |
| `auth.jwt.regionClaim` | string | — | The claim a reader's region is read from. |
| `auth.logout.redirect` | string | — | Where a reader lands after signing out. |
| `auth.managed.allowDomains` | string[] | — | Email domains that may sign in. Empty, nobody can. |
| `auth.managed.magicLinkTtl` | string | — | A duration such as `500ms`, `30s`, `180d`. |
| `auth.mode` | `public` \| `password` \| `jwt` \| `oidc` \| `managed` | — | How a reader proves who they are. `public` asks nothing of them; the rest need `liyasa serve`. |
| `auth.oidc.clientId` | string | — | This site's client id at the provider. The client secret lives in the secret store, never here. |
| `auth.oidc.groupsClaim` | string | — | The claim a reader's access groups are read from. |
| `auth.oidc.issuer` | string | — | The provider's issuer URL, from which its endpoints are discovered. |
| `auth.oidc.scopes` | string[] | — | Scopes requested at sign-in; ask for the claims the site actually reads and no more. |
| `auth.operators` | object[] | — | The operators an instance names in configuration, for the one job organization membership cannot do: an empty membership table elevates nobody, so on a fresh instance there is no one who can reach `settingsWrite` to add the first member. There is no default and no implicit operator; an absent or empty list elevates nobody, which is the safe direction. **An entry here is a credential, not a membership row.** Role sources are asked in order and the first answer wins, so a configured grant cannot be lowered or removed by the membership table: taking somebody out of the organization — the obvious correct thing to do when they leave — does not take away what this list gives them. Remove the entry as well, and treat the list as the break-glass key it is: the shortest safe life for an entry is from a fresh instance to its first real member. `liyasa validate` reports W0139 while any entry stands, so the standing grant is visible rather than remembered. Under `mode: "password"` it can name nobody at all: every reader who knows the password carries one subject, so nobody can be told apart, and a dashboard role in that mode is not available to anyone. |
| `auth.password.argon2.iterations` | integer | `3` | How many passes over that memory one hash makes. |
| `auth.password.argon2.memoryKiB` | integer | `65536` | Memory one hash computation uses, in KiB. |
| `auth.password.argon2.parallelism` | integer | `1` | How many lanes a hash computation runs in. |
| `auth.preview.protection` | `org` \| `password` \| `public` | — | `org` limits a preview to the organization, `password` to whoever has the password, `public` to anyone with the link. |
| `auth.session.cookieName` | string | — | Name of the session cookie, for a site that shares a domain with something else. |
| `auth.session.idleTimeout` | string | — | A duration such as `500ms`, `30s`, `180d`. |
| `auth.session.maxAge` | string | — | A duration such as `500ms`, `30s`, `180d`. |

Every key above is generated from `schemas/liyasa.schema.json`, which is the single source of truth for configuration: a key that is not in the schema is [`E0103`](/errors/E0103), and a value that does not match it is [`E0102`](/errors/E0102).

See [the configuration reference](/reference/config) for the other sections.
