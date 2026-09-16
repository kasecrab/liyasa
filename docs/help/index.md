---
title: Help center
description: Troubleshooting organised by what went wrong, with the error codes that point here.
---

# Help center

[The error reference](/errors) is organised by code. This section is organised
by what you were doing when it failed.

Every diagnostic Liyasa prints carries the URL of its own page, so a code in a
terminal is a link. Each of those pages points back here, to the article that
covers what usually causes codes in its range.

::::cards{cols=2}

:::card{title="Build errors" href="/help/build-errors" icon="hammer"}
The build failed, or produced something you did not expect.
:::

:::card{title="Domains" href="/help/domains" icon="globe"}
A custom domain, a certificate, or a deployment that will not go live.
:::

:::card{title="Authentication" href="/help/auth" icon="lock"}
Private sites, single sign-on, reader groups, and tokens.
:::

:::card{title="Previews" href="/help/previews" icon="eye"}
A preview deployment that differs from production, or does not appear.
:::

:::card{title="Editor" href="/help/editor" icon="edit"}
The web editor, the language server, and editing without git.
:::

:::card{title="Sandbox setup" href="/help/sandbox" icon="box"}
Code verification, runners, containers, and the companion runtime.
:::

::::

## Before anything else

::::steps

:::step{title="Read the code, not just the message"}
Every failure has a code. The code tells you which part of Liyasa is
complaining, which narrows the cause faster than the message does. `E01xx` is
configuration, `E02xx` is templating, `E04xx` is links, `E07xx` is the build.
:::

:::step{title="Run `liyasa doctor`"}
It reports the version, the cache, the companion runtime, container
availability, and whether the configured sources are reachable. A surprising
number of problems are a missing optional dependency rather than a bug.

```sh
liyasa doctor
```
:::

:::step{title="Try a clean build"}
The cache is safe to delete and is occasionally the problem. A corrupted cache
is [`W0702`](/errors/W0702) and is rebuilt automatically, but forcing it costs
one build.

```sh
liyasa build --clean
```
:::

:::step{title="Get the structured output"}
`--json` prints one diagnostic object per line, with the code, the file, the
span, and the help URL. It is easier to read in bulk than the terminal
rendering, and it is what to paste into an issue.

```sh
liyasa build --json
```
:::

::::

## Reporting a problem

An issue that gets fixed quickly has: the version from `liyasa --version`, the
platform, the `--json` diagnostic, and the smallest project that reproduces it.
`liyasa doctor` output covers the first two.

If the problem is that Liyasa accepted something it should have rejected, say
what you expected to be diagnosed. Missing diagnostics are bugs here, not
omissions.
