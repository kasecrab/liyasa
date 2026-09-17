---
title: editor
description: "Editor settings (§15)."
sidebarTitle: editor
---

# `editor`

Editor settings (§15).

Specified by ED-01.

| Key | Type | Default | What it does |
|---|---|---|---|
| `editor.branch` | string | — | The branch the editor commits to. |
| `editor.enabled` | boolean | — | Whether the web editor is available for this site. |
| `editor.preview.maxBytes` | string | — | A byte size such as `512MB`. |
| `editor.preview.maxIterations` | integer | — | How many template iterations a preview render may run before it is cut short. |
| `editor.review` | any | — | — |

Every key above is generated from `schemas/liyasa.schema.json`, which is the single source of truth for configuration: a key that is not in the schema is [`E0103`](/errors/E0103), and a value that does not match it is [`E0102`](/errors/E0102).

See [the configuration reference](/reference/config) for the other sections.
