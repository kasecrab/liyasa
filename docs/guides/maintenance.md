---
title: Maintenance
description: "Keeping a documentation set true over years: review cadence, ownership, what to do with drift, and when to delete."
---

# Maintenance

Documentation does not decay evenly. A conceptual explanation can be correct for
a decade; a page listing supported versions is wrong within a quarter. Treating
every page as equally perishable wastes the review effort you actually have.

## Sort pages by how fast they rot

| Rot rate | Examples | What to do |
|---|---|---|
| **Fast** | Pricing, limits, supported versions, model lists, endpoint references | Make the volatile values [facts](/guides/fact-modelling) or generate the page |
| **Medium** | Procedures, screenshots, integration guides | Set a review cadence; verify what can be verified |
| **Slow** | Concepts, architecture, design rationale | Review on product change, not on a timer |

The aim is to move everything in the fast row out of prose entirely. A page that
is generated or fact-backed does not need reviewing, and review effort is the
scarce resource.

## Review cadence

`content.reviewCadence` sets a site-wide default, and front matter overrides it
per page:

```yaml
---
title: Supported models
reviewed: 2026-09-01
---
```

A page past its cadence is surfaced in the score and in the dashboard. Set the
cadence to a period you will actually honour: a 30-day cadence that everyone
ignores is worse than a 180-day one that gets done, because it trains people to
ignore the signal.

## Ownership

Every page should have an owner who is not "the docs team". Use `authors` in
front matter, and set it to the team that owns the *feature*, since they are the
ones who know when it changed.

```yaml
---
title: Rate limits
authors: [platform-team]
---
```

Ownership without a notification path is decoration. The point of naming an
owner is that drift on that page reaches them.

## Acting on drift

When a fact's source changes, [`E0607`](/errors/E0607) names the fact, the old
value, the new value, and the blocks that depend on it. Three responses, in
order of preference:

::::steps

:::step{title="The page is stale — accept the new value"}
If the page simply references the fact, nothing to do: it already renders the
new value. This is the case you engineered for.
:::

:::step{title="The prose around it is now wrong"}
A limit that went from 600 to 60 may invalidate the sentence that called it
generous. The drift report names the block; read the paragraph, not just the
number.
:::

:::step{title="The change was a mistake upstream"}
Drift can be a genuine finding about the product rather than the docs. This is
the underrated benefit: documentation verification is a second set of eyes on
the change log.
:::

::::

## Deleting

The hardest maintenance discipline is deletion. A page that is out of date and
unowned is worse than no page, because search sends people to it and agents
repeat it.

Delete when: the feature is gone, the page has no owner and no traffic, or the
content is duplicated somewhere better. Always leave a
[redirect](/guides/linking) to the nearest live page.

Archive rather than delete when the content still applies to a version people
are on. That is what version archiving is for: a frozen build, served, excluded
from the assistant unless the reader is on that version.

:::tip{title="Traffic tells you where the harm is"}
A stale page with no readers is technical debt. A stale page in the top ten by
traffic is an active source of wrong answers. Fix in traffic order, not in
discovery order.
:::

## Signals to watch

- **Drift count over time.** Rising means sources are changing faster than you
  are modelling them.
- **Pages past review cadence**, as a proportion. An absolute number is
  demoralising and uninformative.
- **Search queries with no good result.** The clearest statement of what is
  missing you will ever get.
- **Feedback on pages**, especially negative feedback clustered on one section.
- **Assistant answers rated poorly**, which usually means a page is technically
  correct and unusably structured.

## The score

```sh
liyasa score
```

Prints the documentation quality score with its sub-scores and the top actions
to raise it. Use it as a trend rather than a target: a score you optimise
directly is a score that stops measuring anything.

## Next steps

[Verification](/guides/verification) covers the checks that produce these
signals. [Style](/guides/style) covers the rules that keep prose consistent as
the set grows and the number of authors does too.
