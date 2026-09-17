---
title: ai
description: "Model routing and assistant, agent, and reindex settings."
sidebarTitle: ai
---

# `ai`

Model routing and assistant, agent, and reindex settings.

Specified by CFG-97.

| Key | Type | Default | What it does |
|---|---|---|---|
| `ai.agent.contextRepos` | any[] | — | Repositories the agent may read for context while it works. |
| `ai.agent.limits.maxFilesChanged` | integer | — | Most files one proposal may touch. |
| `ai.agent.limits.maxLinesChanged` | integer | — | Most lines one proposal may change. |
| `ai.assistant.enabled` | boolean | — | Offer the assistant to readers. |
| `ai.assistant.instructions` | string | — | Standing instructions the assistant follows on this site, such as what to recommend and what to refuse. |
| `ai.includePersonalized` | `neutral` \| `exclude` | — | What the assistant does with content written for one reader: `neutral` answers from the version nobody is personalized into, `exclude` leaves that content out of its index entirely. |
| `ai.instructions` | string | — | Operator text prepended to assistant and agent system prompts. |
| `ai.models.agent` | string | — | `provider:model`. |
| `ai.models.assistant` | string | — | `provider:model`. |
| `ai.models.embeddings` | string | — | `provider:model`. |
| `ai.models.rerank` | string | — | `provider:model`. |
| `ai.models.translate` | string | — | `provider:model`. |
| `ai.providers.anthropic.baseUrl` | string | — | Where to reach the API, for a proxy or a compatible service. |
| `ai.providers.bedrock.region` | string | — | The AWS region to call Bedrock in. |
| `ai.providers.google.baseUrl` | string | — | Where to reach the API, for a proxy or a compatible service. |
| `ai.providers.ollama.baseUrl` | string | — | Where the Ollama instance is listening. |
| `ai.providers.openai.baseUrl` | string | — | Where to reach the API, for a proxy or a compatible service. |
| `ai.reindex.autoApproveCents` | integer | `500` | How much a reindex may cost, in cents, before someone has to approve it. |
| `ai.respectNoindex` | boolean | `true` | Keep pages marked `noindex` out of the assistant's index too, not only out of search engines'. |

Every key above is generated from `schemas/liyasa.schema.json`, which is the single source of truth for configuration: a key that is not in the schema is [`E0103`](/errors/E0103), and a value that does not match it is [`E0102`](/errors/E0102).

See [the configuration reference](/reference/config) for the other sections.
