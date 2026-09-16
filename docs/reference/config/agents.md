---
title: agents
description: "Agent surfaces: llms.txt, Markdown output, skills, MCP (§24, §25)."
sidebarTitle: agents
---

# `agents`

Agent surfaces: llms.txt, Markdown output, skills, MCP (§24, §25).

Specified by CFG-82.

| Key | Type | Default | What it does |
|---|---|---|---|
| `agents.llms.custom` | string | — | — |
| `agents.llms.full` | boolean | — | — |
| `agents.llms.fullMaxBytes` | string | — | A byte size such as `512MB`. |
| `agents.llms.split` | boolean | — | — |
| `agents.markdown.includeOpenApiSchema` | boolean | — | — |
| `agents.markdown.instructions` | string | — | — |
| `agents.mcp.description` | string | — | — |
| `agents.mcp.discoveryVersion` | string | — | — |
| `agents.mcp.enabled` | boolean | — | — |
| `agents.mcp.external` | any[] | — | — |
| `agents.mcp.name` | string | — | — |
| `agents.skill.enabled` | boolean | — | — |
| `agents.skill.files` | string[] | — | — |
| `agents.specVersion` | string | — | The agent-readiness specification version this site targets (SPEC-03). |

Every key above is generated from `schemas/liyasa.schema.json`, which is the single source of truth for configuration: a key that is not in the schema is [`E0110`](/errors/E0110), and a value that does not match it is [`E0102`](/errors/E0102).

See [the configuration reference](/reference/config) for the other sections.
