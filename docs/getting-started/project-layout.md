---
title: Project layout
description: What each directory in a Liyasa project is for, which files become routes, and which never do.
---

# Project layout

A Liyasa project is a directory with a `liyasa.json` in it. Everything else is
convention, and the conventions exist so that a file's location decides its
behaviour without any configuration.

```
my-docs/
├── liyasa.json              site configuration (required)
├── index.md                 the home page
├── getting-started/
│   ├── quickstart.md
│   └── install.md
├── api-reference/           generated from openapi/ by the navigation config
├── openapi/
│   └── api.yaml
├── snippets/                reusable Markdown fragments and variables
│   ├── auth-note.md
│   └── vars.json
├── components/              your own components (minijinja templates)
│   └── pricing-card.jinja
├── facts/                   sources of truth for verification
│   ├── pricing.json
│   └── sources.toml
├── assets/                  images, video, downloads, served at /assets/...
├── theme/                   optional overrides: tokens.css, custom.css, partials/
├── .liyasaignore            files excluded from the build and from AI indexing
└── .liyasa/                 build cache (add to .gitignore)
```

If you would rather keep the documentation in a subdirectory of a larger
repository, set `"root": "docs"` in `liyasa.json`. A monorepo may hold several
projects, each with its own `liyasa.json`.

## Routes come from paths

A page's URL is derived from where its file sits, not from its title.

| File | Route |
|---|---|
| `index.md` | `/` |
| `getting-started/install.md` | `/getting-started/install` |
| `guides/index.md` | `/guides` |
| a page with `slug: setup` in `getting-started/` | `/getting-started/setup` |

`seo.trailingSlash` decides whether emitted links carry a trailing slash. It
applies to every link the build writes, so the choice is made once.

## What is never a route

These are content the build reads rather than pages it serves, so nothing under
them is routable, indexed, or listed in `llms.txt`:

- `snippets/`, `components/`, `facts/`, `theme/`, and `assets/`
- any file or directory whose name starts with `_`
- anything matched by `.liyasaignore`, which uses gitignore syntax

A second file, `.liyasa-aiignore`, excludes pages from AI indexing only: they
are still served and still in search, but not in `llms.txt` and not in the
assistant's index.

:::tip{title="Hidden is not the same as ignored"}
`hidden: true` in a page's front matter keeps the page routable but takes it out
of navigation, the sitemap, search, and `llms.txt`. Each of those can be turned
back on individually with `search: true`, `ai: true`, or `noindex: false`. Use
it for pages you link to directly but do not want people to stumble into.
:::

## The cache

`.liyasa/` holds the build cache: fingerprints, the dependency graph, rendered
artifacts, and the verification index. It is safe to delete and it belongs in
`.gitignore`. `liyasa build --clean` starts from an empty one.

## Where things go

::::columns{cols=2}

:::column
**Shared prose** goes in `snippets/`. A snippet is included with
`{% snippet "name" %}` and may declare typed props in its front matter.
Changing a snippet invalidates exactly the pages that include it.
:::

:::column
**Shared values** go in `snippets/vars.json` or the `variables` key of
`liyasa.json`, and are read as `vars.<key>`. Values that describe the product
rather than the site belong in `facts/` instead, because those get verified.
:::

::::

## Next steps

[Navigation design](/guides/navigation) covers turning this tree into a
navigation config, and [the configuration reference](/reference/config) lists
every key.
