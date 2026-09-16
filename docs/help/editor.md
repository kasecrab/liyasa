---
title: Editor
description: The web editor, editing without git, the language server, and what to check when an edit does not appear.
---

# Editor

There are three ways to edit a Liyasa site: a text editor and git, the web
editor, and an editor-only workspace with no git at all. They write the same
files.

## In a text editor

Two things make this much better and are worth setting up once:

::::steps

:::step{title="Point `$schema` at the published schema"}
```json
{ "$schema": "https://kasecrab.github.io/liyasa/schema/v1/liyasa.schema.json" }
```

That gives completion and inline validation for every configuration key, in any
editor that speaks JSON Schema. `liyasa schema config` prints a local copy for
an offline setup.
:::

:::step{title="Run the language server"}
```sh
liyasa lsp
```

It provides completion for component names and props, go-to-definition for
snippets and links, and diagnostics as you type rather than at build time.
:::

::::

Front matter has a schema too: `liyasa schema frontmatter`. Setting
`content.frontmatter.strict` turns an unrecognised key into a warning, which
catches the typos a schema-aware editor would otherwise be the only defence
against.

## The web editor

A visual editor over the same Markdown. What it writes is what you would have
typed, so a page edited in one place and then the other does not churn.

::::accordions

:::accordion{title="A component does not appear in the editor"}
Every built-in component has an editor block. A user-defined component under
`components/` needs a valid prop schema in its front matter before the editor
can offer a form for it; an invalid one is [`E0352`](/errors/E0352) and a file
that is not a component definition at all is [`E0356`](/errors/E0356).
:::

:::accordion{title="An edit does not appear on the site"}
Saving in the editor commits to a branch. The site updates when that branch is
deployed, which for a production site usually means a merge.

Check the deployment rather than the editor: an edit that is committed and not
deployed looks identical to an edit that did not save.
:::

:::accordion{title="The editor shows a conflict"}
Two people edited the same page, or the same person edited it in the editor and
in git. The editor's history is per revision, so both versions exist; the
conflict is asking which is the parent of the next one.
:::

::::

## Editing without git

An editor-only workspace keeps full history with restore of any revision, and
can be exported to a git repository at any time:

```sh
liyasa workspace export ./acme-docs
```

That produces a real repository with one commit per workspace revision, authors
and timestamps preserved, and the files exactly as the build reads them. Teams
that start without git and connect it later lose nothing by having waited.

## Previewing while you edit

```sh
liyasa dev
```

The dev server watches the project, rebuilds what changed, and shows errors as
an overlay rather than in a log you have to find. A single page edit is visible
before you have moved your hand back to the browser.

`--groups` and `--region` mock a reader, which is the only sane way to work on
gated content:

```sh
liyasa dev --groups beta --region eu
```

## Formatting

```sh
liyasa format
liyasa format --check
```

Canonical formatting for Markdown, front matter, and configuration. Running
`--check` in CI keeps diffs about content rather than about whitespace.

`--directives` converts the tag form (`<Note>`) to the directive form
(`:::note`), which is what an import from another tool usually leaves behind.

## Getting help

[Style](/guides/style) covers what to write, and
[build errors](/help/build-errors) covers the diagnostics an edit can produce.
