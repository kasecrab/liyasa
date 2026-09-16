---
title: Regions and localization
description: Planning a site that is published in several languages and gates content by where the reader is, without doubling the maintenance.
---

# Regions and localization

Language and availability are independent axes, and conflating them is the
usual mistake. A German reader in the United States wants German prose and
United States availability. A site that ties the two together shows them the
wrong half of both.

Liyasa keeps them separate: **locales** decide what language a page is written
in, **regions** decide which content applies where.

## Locales

Declare the languages the site is published in. The default is served
un-prefixed; the others live under `/<code>/`.

```json
{
  "locales": [
    { "code": "en", "default": true, "label": "English" },
    { "code": "de", "label": "Deutsch" }
  ],
  "localization": { "fallback": "notice" }
}
```

Content for a locale lives in a directory mirroring the default tree
(`de/getting-started/install.md`) or side by side as `install.de.md`. Bind it in
navigation with a `language` node:

```json
{ "language": "de", "pages": ["de/index", "de/erste-schritte"] }
```

What you get: routes under `/de/`, `hreflang` alternates, a per-locale sitemap
and `llms.txt`, a language switcher, and the theme's own interface strings in
that language.

### Missing translations

`localization.fallback` decides what happens when a page exists in the default
locale and not in another:

| Value | Behaviour |
|---|---|
| `notice` | Serve the default-locale page with a "not yet translated" notice |
| `hide` | Do not serve the page in that locale at all |

`notice` is right for a documentation set being translated progressively.
`hide` is right when serving untranslated content would be worse than serving
nothing, which is rare in documentation and common in regulated industries.

:::tip{title="Translate in dependency order"}
Translate the pages a reader hits first, in the order they hit them: home,
install, quickstart, then the most-trafficked guides. A half-translated site
where the entry path is complete reads as in progress. One where random deep
pages are translated reads as broken.
:::

### Keeping translations in sync

The translate automation detects source pages that changed since their
translation and proposes updated translations with a diff of the source change.
The proposal is a diff to review, not a commit: machine translation of technical
prose is a good first draft and a poor final one.

## Regions

Regions are off by default. Turn them on when availability genuinely differs by
where the reader is, not because your company has offices in several countries.

```json
{
  "regions": {
    "enabled": true,
    "list": ["us", "eu", "in"],
    "default": "us",
    "detection": ["choice"],
    "availability": "facts/availability.json"
  }
}
```

### Detection

| Mode | How the region is decided | Needs |
|---|---|---|
| `choice` | A switcher the reader operates | Nothing; works on a static host |
| `header` | An edge header such as `CF-IPCountry` | A server, and the request must come from a trusted proxy |
| `auth` | The reader's authenticated profile | A server and authentication |

Header detection is honoured **only** when the request arrives from an address
in `server.trustedProxies`, and is otherwise ignored, so a reader cannot forge
their region by setting a header. Liyasa performs no IP geolocation itself and
stores no IP addresses.

For a static site, `choice` is the only mode that works, and it is usually
enough.

### Gating content

Four levels, from coarse to fine:

```yaml
---
title: Single sign-on
regions: [us, eu]
---
```

```markdown
:::region{only="us,ca"}
Payments settle through our United States entity.
:::
```

Navigation nodes take `regions` too, and the finest level is fact-driven:

```markdown
{% if region_available("sso") %}
Single sign-on is available on your plan.
{% endif %}
```

Prefer the fact-driven form. It keeps the availability matrix in one file that
the product can generate, rather than scattered across front matter that nobody
updates when a feature launches somewhere new. See
[fact modelling](/guides/fact-modelling).

### What agents and search see

Markdown routes accept `?region=` and default to the **union** of all regions,
with per-block labels ("Available in: US, CA"). An agent is never handed a
silently reduced page, because an agent that does not know content was withheld
will confidently tell someone the feature does not exist.

Search results are filtered by the reader's region in the interface. Region-gated
pages are indexable only in their default variant, and hidden blocks are absent
from the HTML of other variants rather than hidden with CSS.

## Cost

Both axes multiply the pages that get built and the surface that needs
reviewing. Two locales and three regions is not six times the work, because most
content is shared, but it is not one times the work either.

Before turning either on, decide who owns the translations and who owns the
availability matrix. A locale with no owner becomes a stale copy of last year's
documentation, which is worse than no translation at all.

## Next steps

[Fact modelling](/guides/fact-modelling) covers the availability matrix, and
[SEO](/guides/seo) covers the canonical and `hreflang` rules that keep the
duplication from hurting you.
