---
title: api
description: "Manual API pages (API-20)."
sidebarTitle: api
---

# `api`

Manual API pages (API-20).

Specified by CFG-88.

| Key | Type | Default | What it does |
|---|---|---|---|
| `api.auth.in` | `header` \| `query` \| `cookie` | — | Where the API key is sent: in a header, in the query string, or in a cookie. |
| `api.auth.method` | `bearer` \| `basic` \| `apiKey` \| `none` | — | The scheme: a bearer token, HTTP basic, an API key, or nothing. |
| `api.auth.name` | string | — | The header, query parameter, or cookie the API key is sent as. |
| `api.baseUrl` | string | — | The base URL a manual API page's requests are sent to. |

Every key above is generated from `schemas/liyasa.schema.json`, which is the single source of truth for configuration: a key that is not in the schema is [`E0103`](/errors/E0103), and a value that does not match it is [`E0102`](/errors/E0102).

See [the configuration reference](/reference/config) for the other sections.
