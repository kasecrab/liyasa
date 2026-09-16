---
title: Configuration
description: The keys in liyasa.json that matter first, and what each one changes.
---

# Configuration

Everything about this site lives in `liyasa.json`. It is validated on every
build, so a typo is a diagnostic with a line number rather than a page that
renders wrong.

## The keys to set first

| Key | What it does | Set it when |
|---|---|---|
| `name` | The site title, in the tab and the header | Now |
| `seo.canonicalOrigin` | The absolute base for every generated URL | Before you deploy |
| `theme.preset` | The colour and type system | Now |
| `navigation` | The sidebar tree | As you add pages |
| `openapi` | Specifications that become reference pages | When you have one |

:::warning
`seo.canonicalOrigin` is the one people forget. Without it Liyasa cannot write
`llms.txt`, the sitemap, or any absolute link, and agents reading your docs
lose the base URL.
:::

## Checking your work

```bash
liyasa validate
```

`validate` reports configuration, content, link, and specification problems
together, with a code frame for each. Add `--format json` in continuous
integration, or `--format sarif` to get them as code-scanning annotations.

## Variables

Values under `variables` in `liyasa.json` are available in every page as
`vars.*`. This site's support address is {{ vars.support }}, written once and
used everywhere.

:::note
Changing a variable rebuilds every page that reads it, and only those pages.
:::
