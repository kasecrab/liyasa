---
title: The Diátaxis framework
description: Four kinds of documentation, why mixing them is the most common failure, and how to lay them out in a Liyasa project.
---

# The Diátaxis framework

Most documentation that feels bad is not badly written. It is two kinds of
document in one page, fighting each other. Diátaxis is a way of naming the
kinds so you can stop doing that.

It divides documentation along two axes: whether the reader is **studying** or
**working**, and whether they need **practical steps** or **theoretical
knowledge**. That gives four kinds.

| | Practical | Theoretical |
|---|---|---|
| **Studying** | Tutorial | Explanation |
| **Working** | How-to guide | Reference |

## The four kinds

::::accordions

:::accordion{title="Tutorial — learning by doing"}
A lesson. The reader has never used the product and wants to get a feel for it
by building something small that works.

A tutorial makes every decision for the reader. It does not offer choices, it
does not explain alternatives, and it does not aim to be complete. It promises
that if you type these things in this order, you will end up somewhere real.

The hardest discipline is leaving things out. Every "you could also…" is a
place the reader can fall off.
:::

:::accordion{title="How-to guide — solving a problem"}
A recipe. The reader already knows roughly what they are doing and has a
specific goal: *deploy to a private host*, *migrate from Mintlify*, *add a
second locale*.

A how-to guide starts from the problem, not from the feature. It may assume
knowledge, it may skip steps the reader can be trusted with, and it should
handle the realistic variations rather than one blessed path.
:::

:::accordion{title="Reference — describing the machinery"}
A map. The reader knows what they want and needs the exact name, the exact
type, the exact default.

Reference is austere on purpose: consistent structure, no narrative, no
opinion. Its virtue is that it is complete and that you can trust it. This is
the kind that should be generated wherever it can be, which is why Liyasa
generates its [configuration reference](/reference/config) and its
[error codes](/errors) from the same sources the compiler reads.
:::

:::accordion{title="Explanation — understanding why"}
A discussion. The reader wants to know why the thing is the way it is: what the
design trades off, what alternatives were rejected, what the mental model is.

Explanation is the only kind where digression is allowed, because context is
the point. It is also the kind most often missing, and its absence is why
readers cargo-cult configuration they do not understand.
:::

::::

## Why mixing them hurts

A page that is a tutorial with reference tables in the middle loses the learner
in a wall of options. A reference page with a narrative introduction is one
people stop trusting, because they cannot tell what is normative and what is
commentary. A how-to guide that pauses to explain the architecture is one that
does not get someone unblocked at 2am.

The test is simple: name the kind of the page out loud. If you cannot, or if
the honest answer is "both", split it.

## Laying it out in a project

Diátaxis is a way of thinking, not a required directory structure, but the
structure is easier to hold to when the tree reflects it:

```
docs/
├── getting-started/      tutorials: one path, no choices
├── guides/               how-to: one problem per page
├── reference/            generated wherever possible
└── concepts/             explanation: the why
```

Navigation should follow the same division. A reader who is working does not
want to scroll past explanation to reach the reference, and a reader who is
studying does not want to be dropped into a table of flags.

:::note{title="What this site does"}
[Getting started](/getting-started/quickstart) is a tutorial.
This page and its neighbours under `/guides` are how-to and explanation.
[`/reference`](/reference/cli) and [`/errors`](/errors) are reference, and both
are generated. The split is not perfect: the hosting guide carries a generated
table, because the alternative was letting prose drift from the model.
:::

## Further reading

Diátaxis is Daniele Procida's framework and is documented in full at
[diataxis.fr](https://diataxis.fr). The summary here is only enough to act on.

## Next steps

[Style](/guides/style) covers how to write within a kind, and
[navigation design](/guides/navigation) covers arranging the kinds so readers
find the right one.
