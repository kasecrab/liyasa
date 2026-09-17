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
| `agents.llms.custom` | string | — | A file of your own to publish as `llms.txt` instead of the generated index. |
| `agents.llms.full` | boolean | — | Also publish `llms-full.txt`, the whole site as one document, for an agent that would rather read everything than navigate. |
| `agents.llms.fullMaxBytes` | string | — | A byte size such as `512MB`. |
| `agents.llms.split` | boolean | — | Split `llms-full.txt` into several files when one would be larger than the size limit. |
| `agents.markdown.includeOpenApiSchema` | boolean | — | Include the schema of an endpoint's request and response in the Markdown twin of its page. |
| `agents.markdown.instructions` | string | — | A line of guidance prepended to every Markdown twin, for whatever an agent should know before reading the site. |
| `agents.mcp.description` | string | — | What the server says it is for, which is how an agent decides to use it. |
| `agents.mcp.discoveryVersion` | string | — | — |
| `agents.mcp.enabled` | boolean | — | Publish the MCP server. |
| `agents.mcp.external` | any[] | — | Other MCP servers to advertise alongside this one. A trust-plane key: an untrusted build reads it from the deploy branch, never from the branch being built. |
| `agents.mcp.name` | string | — | The server's name as an agent sees it. |
| `agents.skill.enabled` | boolean | — | Publish the skill. |
| `agents.skill.files` | string[] | — | The files that make up the skill. |
| `agents.specVersion` | string | — | The agent-readiness specification version this site targets (SPEC-03). |

Every key above is generated from `schemas/liyasa.schema.json`, which is the single source of truth for configuration: a key that is not in the schema is [`E0103`](/errors/E0103), and a value that does not match it is [`E0102`](/errors/E0102).

See [the configuration reference](/reference/config) for the other sections.
