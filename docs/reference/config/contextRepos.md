---
title: contextRepos
description: "Repositories outside this project that Liyasa may fetch and read, each limited to the paths it names. A clone is shallow and blob-filtered, so a large repository stays affordable to read; `paths` is an allow list rather than a hint, and an entry with none is refused (W0142). Up to ten per project."
sidebarTitle: contextRepos
---

# `contextRepos`

Repositories outside this project that Liyasa may fetch and read, each limited to the paths it names. A clone is shallow and blob-filtered, so a large repository stays affordable to read; `paths` is an allow list rather than a hint, and an entry with none is refused (W0142). Up to ten per project.

Specified by CFG-99.

| Type | Default |
|---|---|
| object[] | — |

Every key above is generated from `schemas/liyasa.schema.json`, which is the single source of truth for configuration: a key that is not in the schema is [`E0103`](/errors/E0103), and a value that does not match it is [`E0102`](/errors/E0102).

See [the configuration reference](/reference/config) for the other sections.
