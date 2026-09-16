---
title: Navigation design
description: Turning a directory tree into a navigation a reader can hold in their head, and the node forms Liyasa gives you to do it.
---

# Navigation design

Navigation is the only part of a documentation site that every reader uses and
almost nobody notices when it works. It fails in two directions: too flat, and
the reader cannot see structure; too deep, and they cannot find anything.

## Principles

**Group by what the reader is doing, not by how you built it.** Readers do not
know which team owns which feature. They know they are trying to deploy.

**Keep the first level short.** Between four and seven groups is what a reader
can scan without effort. If you have eleven, two of them are the same thing.

**Order by the path through the product**, not alphabetically. Installation
before configuration, configuration before deployment. Alphabetical order is
what you use when you have given up on meaning, which is fine for a reference
index and wrong for a guide list.

**Name groups with nouns, pages with what they do.** A group called "Security"
holding pages called "Configure SSO" and "Rotate keys" reads correctly. A group
called "Configuring security settings" does not.

:::tip{title="The three-click rule is a myth, the orientation rule is not"}
Readers will happily click five times if each click visibly moves them closer.
They give up after one click that leaves them unsure whether they are nearer.
Every level should narrow the subject in a way the labels make obvious.
:::

## Declaring navigation

Navigation lives under `navigation` in `liyasa.json`. The simplest form is a
list of pages, by path or by route:

```json
{
  "navigation": ["index", "getting-started/install", "getting-started/quickstart"]
}
```

The object form adds tree-wide options and lets you use the node kinds below:

```json
{
  "navigation": {
    "breadcrumbs": "path",
    "pages": [
      "index",
      { "group": "Getting started", "icon": "rocket", "expanded": true,
        "pages": ["getting-started/install", "getting-started/quickstart"] }
    ]
  }
}
```

## Node kinds

| Node | What it does |
|---|---|
| `"path/to/page"` | A page, by source path or by route |
| `{ "group": … }` | A titled, collapsible group. `root` gives the group's own landing page, `expanded` opens it by default, `tag` puts a label beside it |
| `{ "directory": … }` | Every page under a directory, in file order. Useful for generated trees |
| `{ "tab": … }` | A top-level tab, each with its own tree |
| `{ "menu": … }` / `{ "dropdown": … }` | A navbar menu of links or nested nodes |
| `{ "anchor": … }` / `{ "link": … }` | A link out of the site, with an icon |
| `{ "divider": … }` | A labelled rule between sections |
| `{ "version": … }` / `{ "language": … }` / `{ "product": … }` | A subtree bound to a dimension value |
| `{ "openapi": … }` | Operations from a spec, grouped by tag or path |
| `{ "asyncapi": … }` / `{ "graphql": … }` / `{ "sdk": … }` | The equivalents for other API kinds |

A node that names a page which does not exist is [`E0104`](/errors/E0104), so a
rename that misses the navigation fails the build rather than the reader.

## Tabs, groups, or both

Use **tabs** when a site has audiences that barely overlap: guides and API
reference, or two products under one roof. Each tab is a fresh mental context,
which is exactly what you want when the reader switches and exactly what you do
not want when they are following one path.

Use **groups** for everything else. Nested groups are fine two deep, tolerable
three deep, and a sign of trouble beyond that.

## Pages outside the tree

A page nobody navigates to is [`W0130`](/errors/W0130). That is usually right:
an orphan is usually a mistake. When it is deliberate, say so in front matter
rather than silencing the warning globally.

```yaml
---
title: Legacy migration notes
hidden: true
---
```

`hidden: true` keeps the page routable and drops it from navigation, the
sitemap, search, and `llms.txt`. Turn individual ones back on with
`search: true`, `ai: true`, or `noindex: false`.

Set `"autofill": true` to add pages that no node names to the end of their
directory's group automatically. It is a good setting for a site under rapid
change and a bad one for a site whose order carries meaning.

## Breadcrumbs

`breadcrumbs` takes `"path"` (derived from the route), `"eyebrow"` (the group
name only, shown above the title), or `"none"`. Use `path` for deep reference
trees, `eyebrow` for shallow guide sets.

## Dimensions in navigation

When a site has versions, locales, products, or regions, bind subtrees to them
rather than duplicating the tree:

```json
{ "language": "de", "pages": ["de/index", "de/erste-schritte"] }
```

A node bound to a dimension value that is not declared in `liyasa.json` is
[`E0133`](/errors/E0133).
[Regions and localization](/guides/regions-and-localization) covers the rest.

## Next steps

[Linking](/guides/linking) covers cross-references, which carry as much of the
reader's path as navigation does, and
[SEO](/guides/seo) covers how the same structure reaches people who never see
your navigation at all.
