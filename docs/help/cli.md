---
title: Running the CLI
description: Project roots, configuration paths, the lock file, updates, and the failures that stop a command before it does any work.
---

# Running the CLI

Codes in the `E00xx` range come from the command-line tool itself, before or
around the work you asked for. They are about finding the project, reading its
files, and knowing what this build of Liyasa can do.

Two of them are about the optional runtimes and live in
[sandbox setup](/help/sandbox): [`E0003`](/errors/E0003) for the companion
runtime and [`E0004`](/errors/E0004) for a container sandbox.

## Finding the project

::::accordions

:::accordion{title="E0001 — project root not found"}
Every command looks for `liyasa.json` in the current directory and then in each
parent. If none has one, there is no project.

Run the command from inside the project, or point at it:

```sh
liyasa build --project ../acme-docs
```
:::

:::accordion{title="E0014 — --config names a file that does not exist"}
`--config` takes a path to a configuration file, not a directory and not a
project name. A relative path is resolved from the directory you are in, which
is the usual surprise when it is run from a script.

Drop the flag and run from inside the project when you only meant to say which
project.
:::

:::accordion{title="E0002 — cannot read a file"}
A permission problem or a file that is not valid UTF-8. Liyasa reads text as
UTF-8 with no fallback, so a file saved in another encoding reports here rather
than rendering as mojibake.
:::

::::

## Commands that need a built site

`liyasa test`, `liyasa export`, `liyasa search`, and `liyasa deploy` read what a
previous build wrote. They do not build for you, because silently building would
hide which step was slow and which step failed.

[`E0011`](/errors/E0011) is the output directory not being there, and
[`E0016`](/errors/E0016) is a built site with no search index in it.

```sh
liyasa build
liyasa test --agents
```

## The lock file

`liyasa.lock` records what a build resolved: the Liyasa version, the theme
preset, the bundled fonts by digest, and the companion runtime. Two machines
given the same lock produce the same site.

::::accordions

:::accordion{title="E0005 — the lock cannot be read"}
Either the file is not valid TOML, or its format version is newer than this
CLI understands. The second is the interesting case: a key this build does not
know may be the one that changes the output, so it refuses rather than ignoring
it.

Upgrade Liyasa, or regenerate the lock with the version you are on.
:::

:::accordion{title="E0010 — --locked refused a change"}
`--locked` asserts that the lock is already correct, which is what you want in
CI: a build that quietly updates the lock is a build that is not reproducing
anything.

The message names what would have changed.

```sh
liyasa lock update
```

Run that locally, read the diff, and commit it.
:::

:::accordion{title="W0015 — the companion runtime differs from the lock"}
The installed companion is not the version the lock records. Rendering that goes
through it — maths, pre-rendered diagrams, screenshots — may differ from the
build the lock describes.

Reinstall the companion, or update the lock if the new version is the one you
mean to use.
:::

::::

## Updating

`liyasa update` downloads a release, checks it, and only then replaces the
binary. Three codes cover the three ways that check can fail, and all three stop
the update rather than continuing:

| Code | What failed |
|---|---|
| [`E0009`](/errors/E0009) | The release index could not be read, or has nothing for this platform |
| [`E0008`](/errors/E0008) | The artifact downloaded, and its digest is not the one the index names |
| [`E0007`](/errors/E0007) | The digest is signed, and the signature did not verify |

:::warning{title="Do not work around these"}
A digest or signature failure means the bytes you received are not the bytes
that were published. That is a corrupted download on a bad day and something
worse on a bad network. Install from the releases page and verify the checksum
by hand rather than retrying until it passes.
:::

## What this build can do

[`E0006`](/errors/E0006) means the feature exists in Liyasa but not in the
binary you are running. The message says which feature, and the help line says
what to use instead.

A release binary carries the build and verification engines. The server arrives
with its own crate, and a build compiled without a feature reports it here
rather than pretending.

## Scaffolding

[`E0012`](/errors/E0012) is `liyasa new` pointed at a directory that already has
files in it; it refuses rather than mixing a starter into an existing tree.
[`E0013`](/errors/E0013) is a starter template that does not exist — this
release ships one, so `liyasa new` without `--template` is the way to use it.

## Checks that did not run

Three warnings exist so that a partial run never reads as a clean one. None of
them is a failure, and all three are worth reading before you trust a green
result:

- [`W0017`](/errors/W0017) — a remote OpenAPI source was not fetched
- [`W0018`](/errors/W0018) — external links were not requested
- [`W0019`](/errors/W0019) — a verification check class could not run

The useful habit is to treat "no findings" as meaningful only when no warning
above it says a whole class was skipped.

## Getting help

[Build errors](/help/build-errors) covers what happens once a command reaches
the build, and [sandbox setup](/help/sandbox) covers the container runtime and
the companion.
