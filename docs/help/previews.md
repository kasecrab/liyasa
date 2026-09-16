---
title: Previews
description: Per-branch preview deployments, why a preview can differ from production, and how to tell which difference is real.
---

# Previews

A preview is a build of a branch, served at its own URL, so a change can be read
before it is merged. Whether you get them automatically depends on the host, not
on Liyasa.

## Where previews come from

| Host | Previews |
|---|---|
| Cloudflare Pages, Netlify, Vercel | Per pull request, automatically |
| The Liyasa server | `liyasa deploy --env preview` |
| GitHub Pages | Not natively; build the branch and upload the artifact |

```sh
liyasa deploy --env preview --message "rewrite the limits page"
```

The command prints the URL and the deployment identifier.

## A preview differs from production

Most reported preview bugs are a configuration difference rather than a build
difference. In order of likelihood:

::::accordions

:::accordion{title="A different environment overlay"}
`--env` selects an overlay that is merged into `liyasa.json`. A preview built
with `--env preview` and a production build with `--env production` are
different configurations, which is the point, but it means a difference you see
may be intentional.

```sh
liyasa build --env preview
```
:::

:::accordion{title="Drafts are included"}
Preview builds often set `--drafts`, which includes pages with `draft: true`. A
page that exists in the preview and not in production is usually this.
:::

:::accordion{title="A different base path"}
A preview served at the root of a generated hostname and a production site
served under a path need different `build.basePath` values. Symptoms: the page
loads, the stylesheet does not, links go one level too high. See
[domains](/help/domains).
:::

:::accordion{title="Environment variables"}
`env()` reads only variables allow-listed in `build.env`, and a variable whose
value differs between environments invalidates the pages that read it
([`W0718`](/errors/W0718)). A preview runner that does not set one gets
[`E0211`](/errors/E0211) or a different value.
:::

:::accordion{title="Verification did not run"}
Previews commonly skip verification for speed. A fact that is stale will render
the cached value in the preview and the refreshed one in production. If the
numbers differ, that is the finding, not a bug.
:::

::::

## A preview does not appear

- **The build failed.** A failed build changes nothing that is served, so the
  preview URL either 404s or shows the previous build. Read the build log
  before the deployment log.
- **The queue is full.** [`E0809`](/errors/E0809) means the build queue is at its
  depth cap and the job was rejected. Per-project concurrency defaults to one,
  and a newer push to the same branch supersedes a queued older one, so rapid
  pushes cancel each other by design.
- **Preview builds are disabled for forks.** Most hosts refuse to build a fork's
  branch with repository secrets available, which is correct. The preview will
  not exist until the branch is in the repository.

## Reproducing a preview locally

Preferable to debugging through a deployment:

```sh
liyasa build --env preview --drafts --base-path /preview/my-branch
liyasa serve --offline
```

Then compare against a production build of the same commit. A difference that
survives into two local builds is a real difference; one that does not was
environmental.

## Previews and search engines

Preview deployments should not be indexed, or they compete with the real site
for the same content. Hosts that generate preview URLs usually send
`X-Robots-Tag: noindex` for them; the Liyasa server does the same for
`--env preview`.

If you serve previews yourself, set it. A preview that ranks above production is
a slow and confusing problem to diagnose.

## Getting help

[Domains](/help/domains) covers deployments and base paths, and
[hosting](/guides/hosting) covers what each static host does with a build.
