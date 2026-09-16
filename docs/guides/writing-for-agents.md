---
title: Writing for agents
description: Your documentation is an API surface for language models. What they need, what breaks them, and what Liyasa emits for them.
---

# Writing for agents

A growing share of your readers are not people. They are coding assistants,
support bots, and research agents fetching pages to answer somebody's question.
They are also the readers least able to recover from a badly structured page:
a person who lands mid-table scrolls up, and an agent that retrieved a chunk
mid-table answers from it.

This is not a separate deliverable. Nearly everything that helps an agent helps
a person in a hurry.

## What Liyasa emits

::::columns{cols=2}

:::column
**A Markdown twin of every page.** `/guides/linking` is also served at
`/guides/linking.md`, and where the host supports content negotiation, a
request with `Accept: text/markdown` gets the same thing. Links in that output
are rewritten to absolute URLs.
:::

:::column
**An index at `/llms.txt`.** One line per indexable page: title, URL, and
description. `agents.llms.full` additionally emits `/llms-full.txt`, the whole
site as one document, for agents that would rather fetch once.
:::

::::

Beyond those: a sitemap, feeds, a Model Context Protocol endpoint for agents
that can speak it, and a skill file describing what the site covers.

```sh
liyasa test --agents
```

That runs the agent-readiness checks against the built output and scores them.
The checks are the ones in the public specification, and the score is
comparable across sites rather than being a number we invented.

## What agents need from your prose

**Self-contained sections.** Retrieval returns a chunk, not a page. A section
that begins "This is why it fails" is useless on its own. Name the subject in
the first sentence of every section.

**Descriptive headings.** They are the chunk boundaries and the titles in
retrieval results. See [style](/guides/style).

**Front matter descriptions on every page.** The description is the line in
`llms.txt`, which is often all an agent sees before deciding whether to fetch.
A missing description is [`W0630`](/errors/W0630).

**Explicit versions and dates.** "The current limit" cannot be resolved by a
model reading a cached copy. "As of 2.4, the limit is 600 per minute" can.

**Stated prerequisites.** A page that assumes the reader just read the previous
one produces agents that skip steps.

**No meaning carried only by layout.** A two-column comparison where the
distinction lives in the visual arrangement flattens into nonsense. If the
columns mean something, say what they mean.

:::tip{title="Tabs are flattened, so label them"}
An installation block with tabs for npm, pnpm, and yarn becomes a linear list in
the Markdown output. The tab titles become the labels that keep the commands
apart, so `title="npm"` is load-bearing, not decoration.
:::

## Controlling what agents see

The `visibility` component splits the two audiences when they genuinely need
different content:

```markdown
:::visibility{humans=false}
The endpoint accepts ISO 8601 timestamps with an offset. Naive timestamps are
rejected with HTTP 422.
:::
```

Use it sparingly. Two divergent versions of the truth is the problem this whole
product exists to prevent. Legitimate uses are narrow: extra precision an agent
can act on but a person would find tedious, and visual descriptions of things an
agent cannot see.

`.liyasa-aiignore` excludes pages from AI indexing while leaving them served and
searchable, for content that is accurate but useless out of context.

## Size and shape budgets

Agents have fetch buffers, and a page that exceeds them is truncated silently,
which is worse than a failure. Liyasa checks the shape of what it serves:

| Check | What it catches |
|---|---|
| [`W0720`](/errors/W0720) / [`E0721`](/errors/E0721) | A served response above 1 MB, or above 10 MB |
| [`E0307`](/errors/E0307) / [`W0308`](/errors/W0308) | A page above 100,000 or 50,000 Markdown characters |
| [`W0321`](/errors/W0321) | Machine-generated bulk elements dominating an oversized page |
| [`W0409`](/errors/W0409) | `llms.txt` not covering every indexable page |

A page that trips the size checks usually wants splitting along a boundary a
reader would recognise anyway.

## The honest limits

An agent cannot tell whether your documentation is true. It can only tell
whether it is well formed. That is precisely why the rest of this product
exists: [verification](/guides/verification) is what makes the pages an agent
confidently repeats actually correct.

## Next steps

[Verification](/guides/verification) and
[fact modelling](/guides/fact-modelling) cover truth.
[Linking](/guides/linking) covers the absolute-URL rewriting in the Markdown
output.
