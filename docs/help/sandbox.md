---
title: Sandbox setup
description: Container runtimes, code runners, the companion browser, and what to do when verification cannot run.
---

# Sandbox setup

Verifying code samples means executing them, and executing code from a
repository needs isolation. This article covers getting that working and what
each failure means.

## What needs what

| Feature | Needs |
|---|---|
| Executing code samples | A container runtime (Docker or Podman) |
| Maths with KaTeX, Mermaid pre-render, PDF export | The companion runtime |
| Accessibility tests in a browser, Lighthouse | The companion runtime |
| Screenshot sources | The companion runtime |
| Everything else | Nothing |

```sh
liyasa doctor
```

That reports which of these are present. Nothing in the list is required to
build a site.

## The container runtime

[`E0004`](/errors/E0004) means a sandbox was required and none was available.
Docker and Podman are both supported; Podman is usually the easier one to run
rootless.

::::steps

:::step{title="Check the daemon is running and reachable"}
```sh
docker info
podman info
```

"Cannot connect to the Docker daemon" is a daemon that is not running or a
socket your user cannot reach. On Linux that is usually group membership, and
it takes a new login session to take effect.
:::

:::step{title="Check the image can be pulled"}
Runners use pinned images. On an isolated network, mirror them into a registry
you can reach and set `verify.runners.registry`.
:::

:::step{title="Check the runner is configured for the language"}
[`E0602`](/errors/E0602) means no runner is configured for a sample's language.
Fences are only verified when you ask them to be, so an unconfigured language
is a configuration gap rather than a silent skip.

```json
{
  "verify": {
    "runners": {
      "python": { "image": "python:3.13-slim" }
    }
  }
}
```
:::

::::

## Runner failures

::::accordions

:::accordion{title="E0601 — a check failed"}
The sample ran and produced something other than what the page claims. The
diagnostic carries the diff.

This is the check working. Before changing the expectation, check whether the
page or the product is the thing that is wrong.
:::

:::accordion{title="E0603 — a runner timed out"}
The default per-check timeout is deliberately short. A sample that needs longer
usually needs a network, which runners do not have by default.

Raise `verify.budget.perCheck` for a genuinely slow sample, and prefer making
the sample smaller.
:::

:::accordion{title="E0620 — the local sandbox was rejected"}
`verify.sandbox: "local"` runs code on the host with no isolation. It exists for
a developer's own machine and is refused by the server outright.

If a build on a server needs it, the answer is a runner image, not a policy
exception.
:::

:::accordion{title="E0621 — a command source is not allowed"}
A `command` fact source runs a command, so the server requires it to be in an
allow list with a matching hash. A command that changed since it was allowed is
refused until it is allowed again, which is the point.
:::

::::

## The companion runtime

```sh
liyasa companion install
```

Downloads a pinned headless browser into the user cache directory, the way
Playwright does. [`E0003`](/errors/E0003) is a feature that needed it finding it
absent.

Without it, the graceful paths are:

- Maths renders through the pure-Rust path rather than KaTeX
- Mermaid diagrams render in the browser from the fence source
- `liyasa export --pdf` explains why it cannot run
- `liyasa test --a11y` runs static checks only, and `--perf` is unavailable
- Screenshot verification is skipped rather than failing

On an isolated network, the download will not work. Mirror the browser build and
point the cache at it, or accept the degraded paths, which are the ones most
sites use anyway.

## Isolated networks

Several things reach out by default. On a network where they cannot:

```json
{
  "verify": {
    "links": { "external": false },
    "sources": { "refresh": "0s" }
  }
}
```

Fact sources of kind `url` will fail with [`E0604`](/errors/E0604) rather than
silently serving stale values. That is the right failure: a fact whose source
cannot be read is not verified, and pretending otherwise is the problem this
product exists to prevent.

## Getting help

[Verification](/guides/verification) covers the strategy, and
[fact modelling](/guides/fact-modelling) covers the source kinds each runner
serves.
