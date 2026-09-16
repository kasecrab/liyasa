---
title: Verification strategy
description: What Liyasa can check about your documentation, what it cannot, and how to sequence the checks so the build stays fast.
---

# Verification strategy

Documentation fails silently. That is the whole problem. A page that says the
free tier is 5 GB after it became 2 GB keeps rendering, keeps ranking, and
keeps being believed until somebody is annoyed enough to file an issue.

Verification is the set of checks that make those failures loud.

## The five kinds of check

| Kind | What it proves | Cost |
|---|---|---|
| **Code** | A sample runs and produces the documented output | Seconds to minutes; needs a sandbox |
| **Facts** | A value in prose still matches its source of truth | Milliseconds, or one request per source |
| **Links** | Internal and external targets resolve | Internal is free; external is scheduled |
| **Screenshots** | A captured image still matches the product | Slow; needs the companion runtime |
| **Prose** | Style and terminology rules hold | Fast, local |

```sh
liyasa verify                      # everything configured
liyasa verify --only facts,links   # a subset
liyasa verify --changed HEAD~1     # only pages touched since a ref
```

## Start where the lies are

Do not turn everything on at once. The order below is by value per unit of
effort, and it is worth doing the first two before the rest exist.

::::steps

:::step{title="Internal links"}
Free, immediate, and catches the most common breakage: a page that moved. Set
`build.strictLinks` so a broken internal link fails the build
([`E0401`](/errors/E0401)) rather than warning.
:::

:::step{title="Facts for numbers"}
Every price, limit, version, model identifier, and endpoint name in your prose
is a claim with an owner somewhere else. Move the ten most-repeated ones into
`facts/` first. [Fact modelling](/guides/fact-modelling) covers how.
:::

:::step{title="Code samples"}
Executing samples is the highest-value check and the most expensive to set up,
because it needs a sandbox and a runner per language. Start with the samples in
your quickstart, which are the ones every new user runs.
:::

:::step{title="Prose rules"}
Cheap and immediate. Catches terminology drift and the words that make readers
feel stupid. Run it in CI from the first day.
:::

:::step{title="External links and screenshots"}
Schedule these rather than running them per build. They are slow, they are
flaky for reasons outside your repository, and their failures are rarely urgent.
:::

::::

## Verified code samples

Mark a fence as verified and the runner executes it in a sandbox, comparing
what it prints with what the page claims:

````markdown
```python {verify="python" expect="600"}
from acme import Client
print(Client().limits().requests_per_minute)
```
````

A failing check is [`E0601`](/errors/E0601) with the diff. A language with no
configured runner is [`E0602`](/errors/E0602), and a runner that exceeds its
timeout is [`E0603`](/errors/E0603).

:::warning{title="Sandboxing is not optional"}
Executing code from a repository is executing code. Runners run in a container
with no network by default, and `verify.sandbox` decides what is permitted. The
`local` sandbox, which runs on the host, is rejected outright by the server
([`E0620`](/errors/E0620)) — it exists for a developer's own machine.
:::

## Budgets

Verification competes with deploy time. `verify.budget` bounds it:

```json
{
  "verify": {
    "budget": { "deploy": "60s", "perCheck": "10s", "total": "10m" }
  }
}
```

When the deploy budget is spent, the remaining checks are queued rather than
skipped, and that is [`W0622`](/errors/W0622) rather than a silent omission. The
distinction matters: a check you deferred is a check you still owe.

## Drift

Some checks cannot run at build time. An external service changes on its own
schedule, and a fact sourced from an HTTP endpoint is re-read according to its
`refresh` interval, not when you happen to deploy.

When a source changes and pages disagree with it, that is **drift**:
[`E0607`](/errors/E0607), reported with the fact, the old value, the new value,
and every block that depends on it. Drift is what turns "somebody should check
the docs after the release" into a list of exactly which paragraphs.

## What verification cannot do

It cannot tell you that a page is missing, that an explanation is confusing, or
that a procedure is in the wrong order. It checks claims against sources; it
does not check whether you made the right claims.

Accessibility and performance have their own commands, because they check the
site rather than the content:

```sh
liyasa test --a11y --perf --agents
```

## In CI

```sh
liyasa build --strict
liyasa verify --format sarif > verify.sarif
liyasa test --agents
```

`--strict` turns warnings into errors. `--format sarif` puts the diagnostics
where a code-hosting platform will annotate the diff with them, which is the
difference between a check people read and a check people mute.

## Next steps

[Fact modelling](/guides/fact-modelling) is the design work behind the facts
check. [Maintenance](/guides/maintenance) covers what to do with what
verification finds.
