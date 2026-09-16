---
title: Liyasa
description: Documentation that stays true. Built from Markdown in your repository, checked against the product on every build, and served to humans and agents alike.
---

# Liyasa

Liyasa turns a directory of Markdown files into a documentation site, and then
keeps that site honest. Every claim a page makes about your product can be tied
to a source of truth, and the build tells you when the two disagree.

It is one static binary. No Node, no browser, no runtime service.

::::cards{cols=2}

:::card{title="Install" href="/getting-started/install" icon="download"}
One binary, a container image, or a build from source.
:::

:::card{title="Quickstart" href="/getting-started/quickstart" icon="rocket"}
A site on screen in about a minute.
:::

:::card{title="Verification" href="/guides/verification" icon="shield-check"}
What makes documentation stale, and what to do about it.
:::

:::card{title="Error codes" href="/errors" icon="alert-circle"}
Every diagnostic Liyasa can print, with the fix.
:::

::::

## Why another documentation tool

Documentation goes wrong in a way code does not. A function that stops
compiling fails loudly; a page that says the free tier is 5 GB after it became
2 GB fails silently, and keeps failing until a reader is annoyed enough to file
an issue.

Liyasa treats that as the central problem rather than an afterthought:

::::steps

:::step{title="Write plain Markdown"}
Pages are `.md` files with YAML front matter. Components are directives, so a
page is still readable and still diffable when Liyasa is not involved.
:::

:::step{title="Name your sources of truth"}
A price, a limit, a model identifier, or a version becomes a *fact* with a
source: a JSON file, a repository, an HTTP endpoint, an OpenAPI document.
:::

:::step{title="Let the build check them"}
Code samples are executed, facts are compared against their sources, links are
resolved, and a mismatch is a diagnostic with a code, a page, and a line.
:::

::::

## Built for two readers

A documentation site is now read by people and by agents, and the two want
different things from the same page.

::::columns{cols=2}

:::column
**People** get a site that renders on the server, ships no hydration payload,
and stays usable with JavaScript turned off. Search runs in the browser. The
default theme is designed rather than assembled.
:::

:::column
**Agents** get every page as Markdown at the same URL with a `.md` suffix, an
`llms.txt` index, a Model Context Protocol endpoint, and content negotiation
where the host supports it. No scraping HTML to find the prose.
:::

::::

:::note{title="This site is the reference implementation"}
The pages you are reading are built by the Liyasa in this repository, from the
Markdown under `docs/`. Its error-code pages, configuration reference, and
component gallery are generated from the same sources the compiler reads, so
they cannot drift from the product.
:::

## Where to go next

- [Install](/getting-started/install) and [quickstart](/getting-started/quickstart)
  if you want to see it work.
- [The Diátaxis framework](/guides/diataxis) if you are planning a documentation
  set rather than a page.
- [Writing for agents](/guides/writing-for-agents) if your readers are mostly
  machines.
- [CLI reference](/reference/cli) for every command and flag.
