---
title: Front matter
description: "Every key a page's YAML front matter accepts, generated from the published schema."
---

# Front matter

Front matter is the YAML block at the top of a page, between two `---` lines. It carries what the build needs to know about the page that the body does not say.

```yaml
---
title: Rate limits
description: What the API allows per minute, per plan, and what happens when you exceed it.
---
```

`title` and `description` are the two that every page should carry: they are the search result, the social preview, and the line in `llms.txt`. A page without a description is [`W0630`](/errors/W0630).

Set `content.frontmatter.strict` to turn an unrecognised key into a warning rather than letting a typo pass silently.

| Key | Type | Default | What it does |
|---|---|---|---|
| `access` | object \| null | `null` | — |
| `ai` | object \| null | `null` | — |
| `asyncapi` | any | `null` | — |
| `authors` | string[] | `[]` | — |
| `canonical` | any | `null` | — |
| `date` | any | `null` | — |
| `description` | any | `null` | — |
| `draft` | any | `null` | — |
| `facts` | object | `{}` | — |
| `graphql` | any | `null` | — |
| `groups` | string[] | `[]` | — |
| `hidden` | any | `null` | — |
| `icon` | any | `null` | — |
| `iconType` | any | `null` | — |
| `id` | object \| null | `null` | — |
| `keywords` | string[] | `[]` | — |
| `locales` | object[] | `[]` | — |
| `mode` | object \| null | `null` | — |
| `noindex` | any | `null` | — |
| `og` | object \| null | `null` | — |
| `openapi` | any | `null` | `"spec-id METHOD /path"`. |
| `personalized` | any | `null` | Declares that the page reads free-form `reader.*` fields and is rendered on demand (§6.6.4); without it, `reader.*` is `E0208`. |
| `product` | any | `null` | — |
| `regions` | object \| null | `null` | — |
| `related` | string[] | `[]` | Page IDs or routes for the related topics block. |
| `reviewed` | any | `null` | — |
| `search` | object \| null | `null` | — |
| `sidebarTitle` | any | `null` | — |
| `slug` | any | `null` | — |
| `tag` | any | `null` | — |
| `template` | any | `null` | — |
| `title` | any | `null` | — |
| `twitter` | object \| null | `null` | — |
| `updated` | any | `null` | — |
| `url` | any | `null` | External URL; the page is a navigation link only and has no body. |
| `variation` | string[] | `[]` | — |
| `verify` | any | `null` | Mirrors the `verify` schema object (§14). |
| `versions` | object[] | `[]` | — |
