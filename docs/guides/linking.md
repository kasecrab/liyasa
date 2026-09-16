---
title: Linking
description: Cross-references that survive a reorganisation, link text that works out of context, and what the build checks.
---

# Linking

Links are how a documentation set stops being a pile of pages. They are also
the first thing to rot, because they encode a structure that changes.

## Link by route, not by file

Write internal links as routes, starting with `/`:

```markdown
See [the CLI reference](/reference/cli).
```

Relative links to source files (`../reference/cli.md`) also resolve, and are
converted to routes at build time, which keeps them clickable in a repository
browser. Either is fine. What matters is that both are checked: a link to a
route that does not exist is [`E0401`](/errors/E0401), and a link to a heading
anchor that does not exist is [`E0402`](/errors/E0402).

Set `build.strictLinks` to make those fail the build rather than warn. On a
site that is verified at all, they should fail.

## Link text carries the meaning

Link text is read out of context constantly: by screen readers listing the
links on a page, by search engines, by agents deciding what to fetch next.

| Instead of | Write |
|---|---|
| [click here](/reference/cli) | [the CLI reference](/reference/cli) |
| [this page](/guides/hosting) | [hosting on GitHub Pages](/guides/hosting) |
| read more | [how verification handles screenshots](/guides/verification) |

Non-descriptive link text is [`W0405`](/errors/W0405). "Here", "this", "link",
"read more", and a bare URL all match.

## Anchors

Every heading gets an anchor derived from its text. Because the anchor is
derived, editing a heading breaks every link to it, and the build says so.

When a heading has to change but the link should not, give the heading an
explicit ID:

```markdown
## Rate limits {#limits}
```

Anchors also exist for blocks that have a stable identity: accordions and steps
take an `id` prop, and components that wrap content expose the block ID as an
anchor so a reader can link to a specific step.

## Moving pages

A page's route comes from its path, so moving the file changes the URL. Add a
redirect in the same commit:

```json
{
  "redirects": {
    "rules": [
      { "source": "/guides/deploy", "destination": "/guides/hosting" }
    ]
  }
}
```

A plain array is accepted as shorthand for `redirects.rules`.

Redirects support wildcards (`/v1/*` to `/v2/:splat`) and named parameters
(`/docs/:slug`). Destinations are path-relative by default; an absolute
destination is accepted only when its host is listed in
`redirects.externalAllow`, and a parameter may never be interpolated into the
scheme or host ([`E0109`](/errors/E0109)) so that your documentation domain
cannot be turned into an open redirect.

`status` defaults to `301`, a permanent move. Set it to `302` while you are
still deciding, because search engines and browsers both cache a 301 hard.

:::warning{title="Redirects are not free on every host"}
Static hosts differ in what they will do with a redirect file. GitHub Pages
reads none, so Liyasa emits `<meta refresh>` fallback pages there. See
[hosting](/guides/hosting) for the per-host matrix.
:::

## External links

External links are checked on a schedule rather than on every build, because a
network round trip per link would make builds slow and flaky. An external link
that fails is [`W0404`](/errors/W0404), and repeated failures escalate to
drift, which is reported like any other stale content.

```sh
liyasa broken-links
```

Configure `verify.links.grace` to control how long a link may be failing before
it escalates, and `verify.links.external` to turn external checking off for a
site that is built on an isolated network.

## Linking to other documentation

When you link to another product's documentation, link to a **stable, specific**
page rather than to its home page. A link to a home page with "see their docs
for details" makes the reader do the search you should have done.

If the other site is versioned, link to the version you tested against, not to
`latest`.

## What the Markdown output does

Every page is also served as Markdown for agents. In that output, links are
rewritten to absolute URLs, because an agent that fetched a page has no base to
resolve against. A relative link that survives into the Markdown output is
[`W0406`](/errors/W0406).

This is automatic. It is worth knowing about because it is the reason
`seo.canonicalOrigin` matters even on a site you never intend to appear in a
search engine: without it, absolute URLs cannot be generated
([`W0131`](/errors/W0131)).

## Next steps

[Maintenance](/guides/maintenance) covers keeping links and everything else
true over years, and [writing for agents](/guides/writing-for-agents) covers
the Markdown output in full.
