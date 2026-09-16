---
title: Quickstart
description: Create a site, run the dev server, add a verified fact, and build for production.
---

# Quickstart

This walks from an empty directory to a built site with one verified claim in
it. It assumes `liyasa` is [installed](/getting-started/install).

::::steps

:::step{title="Create a project"}
`liyasa new` writes a project you can build immediately: a `liyasa.json` with
the schema reference in place, a navigation tree, and a few pages that
demonstrate front matter, components, and code samples.

```sh
liyasa new acme-docs
cd acme-docs
```

Add `--yes` to skip the questions and take the defaults.
:::

:::step{title="Run the dev server"}
```sh
liyasa dev
```

The server watches the project and rebuilds what changed. The build memoizes
every query it makes, so editing one page re-renders that page and the pages
whose content depends on it, not the site.
:::

:::step{title="Write a page"}
Create `guides/limits.md`. Front matter carries the title and description;
the body is Markdown.

```markdown
---
title: Rate limits
description: What the API allows per minute, and what happens when you exceed it.
---

# Rate limits

The API allows 600 requests per minute on the Pro plan.
```

The page appears at `/guides/limits` as soon as you save it. Its route comes
from where the file sits, never from its title, which is what makes redirects
mechanical when you move it.
:::

:::step{title="Turn the number into a fact"}
The sentence above is exactly the kind that goes stale. Give the number a
source of truth instead of typing it.

Write `facts/limits.json`:

```json
{ "pro": { "requests_per_minute": 600 } }
```

Declare where it comes from in `facts/sources.toml`:

```toml
[[source]]
id = "limits"
kind = "file"
path = "facts/limits.json"
```

Then reference it in the page:

```markdown
The API allows {{ facts.limits.pro.requests_per_minute }} requests per minute
on the Pro plan.
```

Now the page cannot disagree with `facts/limits.json`, and when that file is
generated from the service's own configuration, the page cannot disagree with
the service. [Fact modelling](/guides/fact-modelling) covers the other source
kinds: a repository, an HTTP endpoint, an OpenAPI document, a command.
:::

:::step{title="Check the site"}
```sh
liyasa validate
liyasa verify
```

`validate` checks configuration, front matter, components, links, and
navigation. `verify` runs the checks that reach outside the repository:
executing code samples, refreshing fact sources, resolving external links.
Both print diagnostics with a code, a file, and a line.
:::

:::step{title="Build for production"}
```sh
liyasa build
```

`dist/` now holds the HTML, the Markdown twin of every page, the search index,
`llms.txt`, a sitemap, redirects, and the header files static hosts read. It is
a directory of files: any static host serves it, and
[the hosting guide](/guides/hosting) covers what each host can do with the
header and redirect files.
:::

::::

## What to read next

::::cards{cols=2}

:::card{title="Project layout" href="/getting-started/project-layout" icon="folder"}
What each directory is for and which ones are never routable.
:::

:::card{title="Diátaxis" href="/guides/diataxis" icon="compass"}
How to decide what pages a documentation set needs.
:::

:::card{title="Verification" href="/guides/verification" icon="shield-check"}
The full picture: code samples, facts, links, screenshots, prose.
:::

:::card{title="CLI reference" href="/reference/cli" icon="terminal"}
Every command and every flag.
:::

::::
