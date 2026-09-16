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
| `ai.agent.contextRepos` | any[] | — | — |
| `ai.agent.limits.maxFilesChanged` | integer | — | — |
| `ai.agent.limits.maxLinesChanged` | integer | — | — |
| `ai.assistant.enabled` | boolean | — | — |
| `ai.assistant.instructions` | string | — | — |
| `ai.includePersonalized` | `neutral` \| `exclude` | — | — |
| `ai.instructions` | string | — | Operator text prepended to assistant and agent system prompts. |
| `ai.models.agent` | string | — | `provider:model`. |
| `ai.models.assistant` | string | — | `provider:model`. |
| `ai.models.embeddings` | string | — | `provider:model`. |
| `ai.models.rerank` | string | — | `provider:model`. |
| `ai.models.translate` | string | — | `provider:model`. |
| `ai.providers.anthropic.baseUrl` | string | — | — |
| `ai.providers.bedrock.region` | string | — | — |
| `ai.providers.google.baseUrl` | string | — | — |
| `ai.providers.ollama.baseUrl` | string | — | — |
| `ai.providers.openai.baseUrl` | string | — | — |
| `ai.reindex.autoApproveCents` | integer | `500` | — |
| `ai.respectNoindex` | boolean | `true` | — |

Every key above is generated from `schemas/liyasa.schema.json`, which is the single source of truth for configuration: a key that is not in the schema is [`E0110`](/errors/E0110), and a value that does not match it is [`E0102`](/errors/E0102).

See [the configuration reference](/reference/config) for the other sections.
