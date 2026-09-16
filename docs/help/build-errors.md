---
title: Build errors
description: The failures that stop a build, what usually causes each one, and what to try first.
---

# Build errors

Codes in `E01xx` through `E07xx` come from the build. This article covers the
ones people actually hit, grouped by what you were doing when it happened.

## Configuration

::::accordions

:::accordion{title="E0101 — liyasa.json is not valid JSON"}
A trailing comma or an unquoted key. JSON has neither. The message carries the
line and column.

Point `$schema` at the published schema and your editor will catch these as you
type:

```json
{ "$schema": "https://kasecrab.github.io/liyasa/schema/v1/liyasa.schema.json" }
```
:::

:::accordion{title="E0102, E0103, E0110 — a key the schema does not recognise"}
[`E0110`](/errors/E0110) means the key is not in the schema at all, usually a
typo or a key from a different tool. [`E0102`](/errors/E0102) means the key
exists but the value has the wrong shape.

`schemas/liyasa.schema.json` is the single source of truth, and
[the configuration reference](/reference/config) is generated from it, so if a
key is not documented there, it does not exist.
:::

:::accordion{title="E0104 — navigation names a page that does not exist"}
Almost always a page that was renamed or moved without updating `navigation`.
The message names the path it looked for.

Note that navigation nodes take a **source path or route**, not a file name:
`guides/install`, not `guides/install.md`.
:::

:::accordion{title="E0105, E0106, E0109 — duplicate routes and bad redirects"}
Two files producing the same route is [`E0105`](/errors/E0105); the usual cause
is a `slug` in front matter colliding with a real file.

[`E0109`](/errors/E0109) is a redirect whose destination is absolute and whose
host is not in `redirects.externalAllow`, or one that interpolates a parameter
into the scheme or host. The second rule is deliberate: it is what stops a
redirect rule turning your documentation domain into an open redirect.
:::

:::accordion{title="E0107, E0132 — colours"}
[`E0132`](/errors/E0132) is a colour value Liyasa cannot parse.
[`E0107`](/errors/E0107) is a colour that parses but fails the contrast check
against its background in one of the two schemes.

The fix for the second is a different colour, not a way to turn the check off.
See [accessibility](/guides/accessibility).
:::

::::

## Templating

::::accordions

:::accordion{title="E0201 — undefined template variable"}
A `{{ name }}` that nothing in the context provides. The context is site
variables, `snippets/vars.json`, facts, dimension values, page front matter,
and `env` values that are allow-listed in `build.env`.

`content.templating.undefined` decides whether this is an error or a visible
marker in dev output. Keep it strict in CI.
:::

:::accordion{title="E0204, E0705 — a template budget was exceeded"}
Time, iterations, output size, or recursion depth. Nearly always an accidental
infinite loop or a recursive include.

The limits are under `content.templating.limits`. Raising them is occasionally
right and usually a way of making a build slow rather than failing.
:::

:::accordion{title="E0205, E0206 — includes and snippets"}
[`E0205`](/errors/E0205) is a snippet that does not exist; check the path is
relative to the project root and that the file is under `snippets/`.
[`E0206`](/errors/E0206) is a cycle: two snippets including each other.
:::

:::accordion{title="E0208 — reader.* on a page that is not personalized"}
Reading reader context makes a page depend on who is asking, which means it
cannot be a single static file. Add `personalized: true` to the page's front
matter, and understand that the page is then rendered per request rather than
built once.
:::

::::

## Content and components

::::accordions

:::accordion{title="E0310, E0311 — a directive that is not closed"}
A container opened with `:::` and never closed, or a `:::` with nothing open.

The usual cause is nesting: a container inside another needs **more colons on
the outer one**.

```markdown
::::cards{cols=2}

:::card{title="Inner"}
The outer fence has four colons, the inner three.
:::

::::
```
:::

:::accordion{title="E0313, E0314, E0315, W0316 — props and component names"}
[`E0313`](/errors/E0313) is an unknown component, and the message suggests the
closest registered name. The other three are prop problems: missing required,
wrong type, unknown.

Props are typed. A number is written unquoted (`cols=2`), a string is quoted, a
boolean is `open=true`, and a list is `only=[us,eu]`. Every component's props are listed in
[the component reference](/reference/gallery), generated from the same
schema the build validates against.
:::

:::accordion{title="E0305 — image without alt text"}
Not a style warning. An image with no alt text is unusable for readers using a
screen reader, and it is an error rather than a warning for that reason. For a
genuinely decorative image, write `alt=""`.
:::

:::accordion{title="E0307, W0308, W0720, E0721 — the page is too big"}
Markdown above 100,000 characters, or a served response above 10 MB, breaks
agent fetch buffers and is slow for everyone.

A page this size nearly always has a natural split point a reader would
recognise. If it is genuinely one generated table, consider whether the data
belongs in a downloadable file with a summary on the page.
:::

::::

## Links and assets

::::accordions

:::accordion{title="E0401, E0402 — a link to nothing"}
A route that does not exist, or a heading anchor that does not exist. Both are
usually a page or a heading that was renamed.

Anchors come from heading text. To rename a heading without breaking links to
it, give it an explicit ID: `## Rate limits {#limits}`.
:::

:::accordion{title="E0403 — missing image or asset"}
The path is resolved from the project root for absolute paths and from the page
for relative ones. Files must be under `assets/` (or another directory you have
configured) to be copied to `dist/`.
:::

:::accordion{title="W0404 — an external link is unreachable"}
Checked on a schedule rather than on every build, so this is often a transient
failure of somebody else's site. `verify.links.grace` sets how long a link may
be failing before it escalates to drift.
:::

::::

## The build itself

::::accordions

:::accordion{title="E0701 — build failed (aggregate)"}
The umbrella code. The real causes are the diagnostics printed above it. On a
server, it also covers a build killed for exceeding its memory limit.
:::

:::accordion{title="E0706, W0707 — the build is not reproducible"}
[`W0707`](/errors/W0707) means no build clock was supplied, so the wall clock
was used and two builds of the same commit will differ. Supply a commit
timestamp in CI.

[`E0706`](/errors/E0706) is `--check-determinism` finding an actual difference
between two builds of the same input, which is a bug worth reporting.
:::

:::accordion{title="W0702 — cache corrupted, rebuilt"}
Recovered automatically. If it recurs, something is writing to `.liyasa/`
concurrently: two builds in the same directory, or a file watcher.
:::

:::accordion{title="W0710, E0711, E0712 — too many variants"}
A page that reads dimension values is built once per combination. A page that
crosses the per-page cap is marked dynamic ([`W0710`](/errors/W0710)); a site
that crosses the total cap fails ([`E0711`](/errors/E0711)).

Usually the fix is to narrow which dimensions a page actually reads, rather than
to raise the cap.
:::

::::

## Still stuck

Get the structured output and read the whole set rather than the first line:

```sh
liyasa build --json --strict
```

One root cause often produces several diagnostics. The first one in file order
is usually the one to fix.
