---
title: Importing
description: Moving a Mintlify, Docusaurus, or MDX project into Liyasa, and working through what the migration report asks of you.
---

# Importing

An import converts a documentation project from another tool into a Liyasa one.
It is designed to be read before it is applied, and to tell you the truth about
what it could not convert rather than quietly dropping it.

:::info{title="Not reachable from the command line yet"}
`liyasa import <mintlify|docusaurus|mdx> <path>`, with `--dry-run`, `--force`,
and `--directives`, is specified and not yet built. Today the importer is a
library: a caller builds a plan and applies it. The diagnostics below are the
same either way, and so is everything this page asks you to do about them.
:::

## The shape of an import

::::steps

:::step{title="A plan is built"}
The importer reads the source project and produces a **plan**: every file it
would write, and a report of everything that needs a person. Building a plan
writes nothing.
:::

:::step{title="You read the report"}
The plan carries a migration report, also written to `migration-report.md` when
the plan is applied. It is the worklist, and it is the point of the whole
exercise.
:::

:::step{title="The plan is applied"}
Applying writes the converted project. If any single file it would write is
already there, nothing at all is written ([`E1103`](/errors/E1103)) — a refused
import never leaves a half-converted project behind.
:::

::::

Because the plan is the product, the dry run is the default rather than a mode
you have to remember to ask for.

## What stops an import

These are errors: the import does not finish.

| Code | Cause |
|---|---|
| [`E1101`](/errors/E1101) | The path is not a project this importer recognises, usually a repository root rather than the documentation directory |
| [`E1102`](/errors/E1102) | A source file could not be read, or is not UTF-8 |
| [`E1103`](/errors/E1103) | The destination already holds a file the import would write |
| [`E1104`](/errors/E1104) | The source configuration is not valid JSON or YAML |
| [`E1105`](/errors/E1105) | A JavaScript configuration reaches outside itself and cannot be evaluated |

[`E1102`](/errors/E1102) is the one to watch: it skips the file and lets the
rest of the import finish, so a migration that reported it is incomplete in a
way the converted project will not show you.

## What needs your attention afterwards

These are warnings. The import finished, and each names something a person has
to decide.

| Code | What it asks |
|---|---|
| [`W1110`](/errors/W1110) | Write the markup for a component that has no Liyasa equivalent. Once per component, not once per page |
| [`W1111`](/errors/W1111) | Rewrite a JavaScript expression that was carried across verbatim |
| [`W1112`](/errors/W1112) | Rehome an `import` or `export` that was dropped from a page |
| [`W1113`](/errors/W1113) | Set a Liyasa option by hand where a source configuration key had no counterpart |
| [`W1114`](/errors/W1114) | Remove or restore a navigation entry that names a page which does not exist |
| [`W1115`](/errors/W1115) | Put the generated redirect on the host you are migrating away from |
| [`W1116`](/errors/W1116) | Fix a converted page that does not parse |

## Working through the list

The order that wastes least time:

1. **[`W1116`](/errors/W1116) first.** Those pages do not build, so nothing else
   you do is verifiable until they do.
2. **[`W1110`](/errors/W1110) next**, because one stub can clear dozens of
   pages at once.
3. **[`W1113`](/errors/W1113) and [`W1114`](/errors/W1114)**, which are
   configuration and navigation and affect the whole site.
4. **[`W1111`](/errors/W1111) and [`W1112`](/errors/W1112)** page by page.
5. **[`W1115`](/errors/W1115) last**, and not in the repository at all — it is
   work on the host you are leaving.

Then build, and treat the result as the real test:

```sh
liyasa build --strict
liyasa validate
```

:::tip{title="Fix the source too, where it is the source that is wrong"}
Several of these — an unclosed fence, a navigation entry pointing at a deleted
page — were already broken before the migration and were tolerated by a more
lenient pipeline. If you are keeping the old site running during a transition,
the same fix is worth making there.
:::

## Getting help

[Build errors](/help/build-errors) covers what the converted project reports
once it starts building, and [linking](/guides/linking) covers redirects, which
is what most route changes come down to.
