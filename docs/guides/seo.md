---
title: SEO
description: What actually moves documentation in search, what Liyasa emits for you, and what to configure.
---

# SEO

Documentation search traffic behaves differently from marketing search traffic.
People arrive with an error message, a function name, or a specific task, and
they arrive on deep pages rather than on your home page. Optimising for that is
mostly a matter of structure and honesty, not of keywords.

## What matters most

**Titles and descriptions that answer a question.** The title is the search
result; the description is usually the snippet. Write both as though the reader
has not seen your product's name before.

**One page per question.** Three thin pages on the same subject compete with
each other and none of them ranks. One complete page beats them all.

**Stable URLs.** Every URL change costs accumulated ranking. Routes come from
file paths, so moving a file is an SEO decision — add a permanent
[redirect](/guides/linking) in the same commit.

**Internal links with descriptive text.** They are how search engines discover
depth and how they infer what a page is about.

**Speed.** Liyasa renders on the server and ships no hydration payload, which
covers most of this. Media is the part left to you; see
[media](/guides/media).

## What Liyasa emits

::::columns{cols=2}

:::column
- `sitemap.xml`, covering exactly the pages that are indexable
- `robots.txt`, including per-crawler rules
- Canonical URLs on every page
- Open Graph and Twitter card metadata
- JSON-LD for the organization and for articles
- `hreflang` alternates for every locale
- RSS, Atom, and JSON feeds for changelogs
:::

:::column
```json
{
  "seo": {
    "canonicalOrigin": "https://docs.acme.com",
    "indexing": "navigable",
    "trailingSlash": false,
    "organization": { "name": "Acme", "url": "https://acme.com" }
  }
}
```
:::

::::

`seo.canonicalOrigin` is the one key you must set. Without it absolute URLs
cannot be generated ([`W0131`](/errors/W0131)), which affects canonical tags,
feeds, `llms.txt`, and the Markdown output, not just search.

`indexing: "navigable"` indexes only pages reachable from navigation, which is
usually what you want: it keeps orphans and internal pages out of search
results without you having to mark each one.

## Per-page control

```yaml
---
title: Rate limits
description: What the API allows per minute, per plan, and what happens when you exceed it.
keywords: [rate limit, 429, quota, throttling]
noindex: false
canonical: https://docs.acme.com/guides/limits
og:
  image: /assets/og/limits.png
---
```

Set `canonical` explicitly when the same content is genuinely served in more
than one place, for example the default version of a versioned page. Liyasa
handles the ordinary versioned case for you; the override is for the unusual
ones.

## Versions, locales, and duplication

Versioned and localized sites generate near-duplicate pages, which search
engines handle badly unless you tell them what is going on.

- The **default version** is served un-prefixed and is the canonical one. Older
  versions point their canonical at themselves but are typically excluded from
  indexing, so that a search result never lands a reader on last year's page.
- **Locales** carry `hreflang` alternates and per-locale sitemaps, which is how
  a search engine learns that the German page is a translation and not
  duplicated content.
- **Region-gated pages** are indexable only in their default variant, and
  content hidden in a variant is absent from the HTML rather than hidden with
  CSS, so a crawler cannot index content a reader would not see.

## Crawlers, including the new ones

```json
{
  "seo": {
    "crawlers": {
      "gptbot": { "allow": true },
      "claudebot": { "allow": true },
      "internal-preview": { "disallow": true, "paths": ["/preview/*"] }
    }
  }
}
```

Whether to admit AI crawlers is a policy decision, not a technical one. For
product documentation the usual answer is yes, because the alternative is that
models answer from a stale scrape or from a competitor's description of your
product. If you admit them, [writing for agents](/guides/writing-for-agents)
covers making that content correct rather than merely available.

## What not to bother with

Keyword density, meta keyword stuffing, and doorway pages for every synonym of
your feature. Documentation ranks on being the authoritative, complete, fast
answer, and there is no shortcut that beats being that.

## Next steps

[Writing for agents](/guides/writing-for-agents) covers the machine readers,
and [regions and localization](/guides/regions-and-localization) covers the
`hreflang` and canonical rules for multi-locale sites.
