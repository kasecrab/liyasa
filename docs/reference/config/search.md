---
title: search
description: "Search behaviour and the browser index's shard sizing (§8.6)."
sidebarTitle: search
---

# `search`

Search behaviour and the browser index's shard sizing (§8.6).

Specified by CFG-50..CFG-55.

| Key | Type | Default | What it does |
|---|---|---|---|
| `search.boost` | object[] | — | Rules that move results up or down the ranking. |
| `search.exclude` | string[] | — | Globs kept out of the index entirely. An excluded page is still routable; it is simply not findable. |
| `search.filters` | (`tab` \| `version` \| `locale` \| `type`)[] | — | The facets a reader may filter results by. |
| `search.maxResults` | integer | `20` | How many results one query returns. |
| `search.mode` | `keyword` \| `hybrid` | — | `keyword` searches the index alone. `hybrid` also uses the assistant's embeddings, which needs a server. |
| `search.placeholder` | string | — | Placeholder text in the search field. Localizable. |
| `search.shardSize.max` | string | — | A byte size such as `512MB`. |
| `search.shardSize.min` | string | — | A byte size such as `512MB`. |
| `search.shortcut` | string | `"mod+k"` | Keyboard shortcut that opens search, written as a chord such as `mod+k`, where `mod` is Command on macOS and Control elsewhere. |
| `search.snippets` | boolean | `true` | Whether a result carries a matching excerpt. Off, results are titles alone and the index is smaller. |

Every key above is generated from `schemas/liyasa.schema.json`, which is the single source of truth for configuration: a key that is not in the schema is [`E0103`](/errors/E0103), and a value that does not match it is [`E0102`](/errors/E0102).

See [the configuration reference](/reference/config) for the other sections.
