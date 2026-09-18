---
title: Hosting
description: A Liyasa build is a directory of files. What each static host does with the header and redirect files, and what to configure where.
---

# Hosting

`liyasa build` writes `dist/`: HTML, the Markdown twin of every page, the search
index, `llms.txt`, a sitemap, redirects, and header files. Any static host will
serve it. The hosts differ in what they do with the last two.

```sh
liyasa build
liyasa deploy --gh-pages        # or upload dist/ however you like
```

## What the build writes for hosts

| File | Read by |
|---|---|
| `_headers` | Cloudflare Pages, Netlify |
| `_redirects` | Cloudflare Pages, Netlify |
| `vercel.json` | Vercel |
| `.nojekyll` | GitHub Pages |
| `<meta refresh>` pages | Any host that reads no redirect file |

`.nojekyll` is written on every build rather than for one host, because it is
inert everywhere else and a `dist/` is uploaded by hand as often as by a
configured deploy. Without it GitHub Pages runs Jekyll over the upload, and
Jekyll publishes nothing whose name begins with `_` or `.` — which is the
theme, the image variants, and the agent surfaces.

Set `build.basePath` when the site is served under a path rather than at the
root of a domain, for example `/liyasa/docs` on GitHub Pages. Every emitted
link, header rule, and redirect is rewritten to sit under it.

## Host capability matrix

The table below is generated from the host models the build uses when it emits
those files, and it is held to those models by the test suite. It is the same
table `liyasa test --agents` grades against.

:::warning{title="Modelled, not measured"}
These rows describe documented host behaviour as Liyasa models it, not results
observed on live accounts with each provider. Where a host changes its
behaviour, the model is what needs updating. Treat a `yes` as "this is what the
host documents", and confirm anything you are depending on.
:::

{% snippet "host-matrix" %}
## Choosing

::::columns{cols=2}

:::column
**GitHub Pages** is the simplest thing that works, and the right default for an
open-source project. The build writes `.nojekyll` for it, which is the one thing
it needs and does not ask for. Beyond that it reads no header or redirect file,
so redirects become `<meta refresh>` pages and security headers have to be set
elsewhere or gone without. Use `build.basePath` when publishing under a
repository path.
:::

:::column
**Cloudflare Pages, Netlify, and Vercel** read the files the build emits, so
redirects, security headers, the content security policy, and immutable asset
caching all work with no configuration beyond uploading `dist/`. This is the
path of least resistance for a production documentation site.
:::

::::

::::columns{cols=2}

:::column
**S3 with CloudFront** needs the header and redirect behaviour configured in
CloudFront rather than in the bundle. It is the right answer when the
documentation has to live inside an existing AWS account and the wrong answer
when it does not.
:::

:::column
**Your own web server** gives you everything, at the cost of configuring it. The
hosting reference in the repository carries server blocks for the common ones.
Content negotiation for `Accept: text/markdown` is only possible here and on the
Liyasa server.
:::

::::

## Content negotiation

No static host negotiates on `Accept`. Every host therefore scores `partial` on
that check, and the documented alternative is the `.md` route, which every host
serves and which agents can construct from a URL without a round trip.

If negotiation matters to you, run `liyasa serve`, which serves the same bundle
with negotiation, ETags, and on-demand rendering for personalized pages.

## Security headers

The build emits a content security policy derived from what the site actually
loads, plus the usual security headers. On hosts that read `_headers` or
`vercel.json` this is automatic. Elsewhere, copy the values from
`dist/_headers` into your host's own configuration; the format differs but the
values do not.

A CSP source in `security.csp` that is not a valid source expression is
[`E0722`](/errors/E0722). A new remote image or media host appearing in content
and being added to the policy is [`W0719`](/errors/W0719), which is worth
watching: it means a page started loading from somewhere new.

## Previews

Per-branch preview deployments are a property of the host, not of the build. On
Cloudflare Pages, Netlify, and Vercel they come for free. On GitHub Pages they
do not; the usual arrangement is a workflow that builds the branch and uploads
the artifact for download, or a second repository for previews.

See [previews](/help/previews) when a preview shows something the production
site does not.

## Next steps

[SEO](/guides/seo) covers the canonical origin, which matters more once the site
has a real domain, and [domains](/help/domains) covers custom domain setup and
what to check when one does not resolve.
