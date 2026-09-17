---
title: CLI reference
description: "Every Liyasa command and flag, generated from the same definitions that produce `--help`."
---

# CLI reference

Liyasa is one command with subcommands. Everything below is generated from the
same definitions that produce `--help`, so this page and the terminal cannot
disagree:

```sh
liyasa <command> --help
```

Commands are listed alphabetically, which is what a reference is for. If you
are looking for the order to do things in, start with
[the quickstart](/getting-started/quickstart).

## Global flags

Every command accepts these. Each one can also come from the environment variable in the last column, which is what makes a flag settable once for a whole CI job.

| Flag | Value | Default | Environment | What it does |
|---|---|---|---|---|
| `--color` | `<WHEN>` | `auto` | `LIYASA_COLOR` | When to colour output |
| `--config` | `<PATH>` | — | `LIYASA_CONFIG` | Read this `liyasa.json` instead of searching upward from the working directory |
| `--dry-run` | — | — | `LIYASA_DRY_RUN` | Print what would happen and change nothing |
| `--json` | — | — | `LIYASA_JSON` | Print output as JSON. Where a command has `--format`, this is the same as `--format json` and `--format` wins if both are given |
| `--offline` | — | — | `LIYASA_OFFLINE` | Make no outbound request; fail rather than reach the network |
| `--quiet`, `-q` | — | — | `LIYASA_QUIET` | Print errors and nothing else |

## Commands

### `liyasa broken-links`

Check internal and external links

| Flag | Value | Default | Environment | What it does |
|---|---|---|---|---|
| `--allow` | `<URL\|HOST>` | — | — | A URL or host to accept without checking. Repeatable |
| `--concurrency` | `<N>` | `8` | — | How many requests to have in flight at once |
| `--format` | `<FORMAT>` | `text` | — | — |
| `--internal-only` | — | — | — | Check only links inside the site |
| `--timeout` | `<SECONDS>` | `10` | — | How long to wait for one response, in seconds |

### `liyasa build`

Build the site into the output directory

| Flag | Value | Default | Environment | What it does |
|---|---|---|---|---|
| `--base-path` | `<PATH>` | — | `LIYASA_BASE_PATH` | Serve the site from this path prefix |
| `--build-time` | `<WHEN>` | — | `LIYASA_BUILD_TIME` | Date this build from a fixed instant, so two builds of the same inputs agree (§6.6.2). A Unix timestamp or an RFC 3339 date.  `SOURCE_DATE_EPOCH` wins over it, and a git commit is used when neither is given. |
| `--check-determinism` | — | — | — | Build twice and report any file that differed (E0706) |
| `--clean` | — | — | — | Empty the output directory and the cache first |
| `--drafts` | — | — | `LIYASA_DRAFTS` | Include pages marked `draft: true` |
| `--env` | `<ENV>` | — | `LIYASA_ENV` | Merge `liyasa.<env>.json` over the configuration |
| `--locked` | — | — | — | Fail rather than change `liyasa.lock` |
| `--output`, `-o` | `<DIR>` | — | `LIYASA_OUTPUT` | Where the built site goes |
| `--profile` | — | — | — | Print how long each phase took |
| `--strict` | — | — | `LIYASA_STRICT` | Treat warnings as errors |

### `liyasa companion`

Manage the optional browser runtime (§6.12)

#### `liyasa companion install`

Download and verify the pinned browser runtime

| Flag | Value | Default | Environment | What it does |
|---|---|---|---|---|
| `--source` | `<SOURCE>` | — | `LIYASA_COMPANION_SOURCE` | The archive to install. A directory or a `file://` URL today |

#### `liyasa companion remove`

Delete the installed runtime

#### `liyasa companion status`

Say whether the runtime is installed and which version

### `liyasa completions`

Print a shell completion script

| Flag | Value | Default | Environment | What it does |
|---|---|---|---|---|
| `<shell>` | `<SHELL>` | — | — | The shell to generate for |

### `liyasa dev`

Serve the project with live reload

| Flag | Value | Default | Environment | What it does |
|---|---|---|---|---|
| `--disable-openapi` | — | — | — | Skip the OpenAPI pages, which are the slowest part of a cold start |
| `--disable-prefetch` | — | — | — | Do not prefetch linked pages in the reader |
| `--drafts` | — | — | `LIYASA_DRAFTS` | Include pages marked `draft: true` |
| `--groups` | `<A,B>` | — | — | Mock reader groups, comma separated |
| `--host` | `<HOST>` | `127.0.0.1` | `LIYASA_HOST` | — |
| `--local-schema` | — | — | — | Validate against the schema in this working copy rather than the published one |
| `--locale` | `<LOCALE>` | — | — | Render this locale |
| `--no-open` | — | — | — | — |
| `--open` | — | — | `LIYASA_OPEN` | Open a browser once the first render is ready |
| `--port`, `-p` | `<PORT>` | `3000` | `LIYASA_PORT` | — |
| `--region` | `<REGION>` | — | — | Mock the reader's region |
| `--verify` | — | — | — | Re-run verification on every rebuild |
| `--version` | `<VERSION>` | — | — | Render this content version |

### `liyasa doctor`

Report what this machine can and cannot do

### `liyasa export`

Export the built site in another shape

| Flag | Value | Default | Environment | What it does |
|---|---|---|---|---|
| `--markdown` | — | — | — | Only the Markdown twins and `llms.txt` |
| `--offline` | — | — | — | Rewrite every asset reference so the export works from a file:// URL |
| `--output`, `-o` | `<DIR>` | — | `LIYASA_OUTPUT` | Where the export goes |
| `--pdf` | — | — | — | One PDF of the whole site. Needs the companion runtime |
| `--static` | — | — | — | The static site. The default |
| `--zip` | — | — | — | Wrap the export in a zip archive |

### `liyasa format`

Rewrite Markdown, front matter, and config into canonical form

| Flag | Value | Default | Environment | What it does |
|---|---|---|---|---|
| `--check` | — | — | — | Report what is unformatted and change nothing |
| `--directives` | — | — | — | Rewrite tag-form components into directive form |
| `<paths>` | `<PATHS>` | — | — | Only these files. Defaults to the whole project |

### `liyasa lock`

Inspect and refresh `liyasa.lock`

#### `liyasa lock check`

Report what would change without writing (the `--locked` predicate)

#### `liyasa lock update`

Refresh `liyasa.lock` from the project as it is now

### `liyasa lsp`

Run the language server an editor talks to over stdin and stdout

### `liyasa migrate-config`

Upgrade `liyasa.json` between schema versions

| Flag | Value | Default | Environment | What it does |
|---|---|---|---|---|
| `--write` | — | — | — | Write the upgraded configuration back. Without it the result is printed |

### `liyasa new`

Create a new documentation project

| Flag | Value | Default | Environment | What it does |
|---|---|---|---|---|
| `--ci` | — | — | — | Write a continuous-integration workflow |
| `--git` | — | — | — | Run `git init` in the new project |
| `--name` | `<NAME>` | — | — | The site name. Prompted for when absent unless `--yes` |
| `--no-ci` | — | — | — | Do not write a continuous-integration workflow |
| `--no-git` | — | — | — | Do not run `git init` |
| `--no-openapi` | — | — | — | Leave the sample OpenAPI specification out |
| `--openapi` | — | — | — | Include the sample OpenAPI specification |
| `--preset` | `<PRESET>` | — | — | The theme preset to start from |
| `--template` | `<NAME\|URL>` | — | — | A built-in starter name or a git URL |
| `--yes`, `-y` | — | — | — | Take the default for every question instead of asking |
| `<directory>` | `<DIRECTORY>` | — | — | Where to put the project. Defaults to the working directory |

### `liyasa schema`

Print a JSON Schema

| Flag | Value | Default | Environment | What it does |
|---|---|---|---|---|
| `<which>` | `<WHICH>` | — | — | Which schema to print. Prints the list when absent |

### `liyasa score`

Print the documentation quality score

| Flag | Value | Default | Environment | What it does |
|---|---|---|---|---|
| `--format` | `<FORMAT>` | `text` | — | — |
| `--output` | `<DIR>` | — | `LIYASA_OUTPUT` | The built site to score. Defaults to the configured output directory |

### `liyasa search`

Query the local search index

| Flag | Value | Default | Environment | What it does |
|---|---|---|---|---|
| `--index` | `<DIR>` | — | — | The search index directory. Defaults to the built site's |
| `--limit` | `<N>` | `10` | — | — |
| `--locale` | `<LOCALE>` | — | — | — |
| `--tab` | `<TAB>` | — | — | — |
| `--version` | `<VERSION>` | — | — | — |
| `<query>` | `<QUERY>` | — | — | What to search for |

### `liyasa serve`

Run the Liyasa server

| Flag | Value | Default | Environment | What it does |
|---|---|---|---|---|
| `--collector-only` | — | — | — | Serve only the analytics ingest endpoint |
| `--db` | `<URL>` | — | `LIYASA_DB` | — |
| `--init` | — | — | — | Run first-time setup and print the one-time admin token |
| `--listen` | `<ADDR>` | `0.0.0.0:8080` | `LIYASA_LISTEN` | — |
| `--storage` | `<URL>` | — | `LIYASA_STORAGE` | — |
| `--tls` | `<DOMAIN>` | — | — | Terminate TLS for these domains with automatic certificates |

### `liyasa telemetry`

Turn anonymous usage reporting on or off

#### `liyasa telemetry off`

Stop reporting anonymous usage

#### `liyasa telemetry on`

Start reporting anonymous usage

#### `liyasa telemetry status`

Say whether reporting is on. The default is off

### `liyasa test`

Run accessibility, performance, and agent-readiness tests

| Flag | Value | Default | Environment | What it does |
|---|---|---|---|---|
| `--a11y` | — | — | — | Accessibility checks |
| `--agents` | — | — | — | The §25 agent-readiness checks against the built output |
| `--format` | `<FORMAT>` | `text` | — | — |
| `--output` | `<DIR>` | — | `LIYASA_OUTPUT` | The built site to test. Defaults to the configured output directory |
| `--perf` | — | — | — | Lighthouse budgets. Needs the companion runtime |
| `--search` | — | — | — | The search assertions in `tests/search.toml` |
| `--urls` | `<URL>` | — | — | Score exactly these pages instead of sampling the site (§25). A route (`/guide/install`) or an absolute URL on this site's origin. Repeat the flag or separate with commas.  Explicitly selected pages are scored as given regardless of how few there are, where a sample of under five is not. |

### `liyasa theme`

Theme override helpers

#### `liyasa theme diff`

Show what an override changed relative to the current default

| Flag | Value | Default | Environment | What it does |
|---|---|---|---|---|
| `<partial>` | `<PARTIAL>` | — | — | Only this partial |

#### `liyasa theme eject`

Copy a default partial into `theme/partials/` for editing

| Flag | Value | Default | Environment | What it does |
|---|---|---|---|---|
| `<partial>` | `<PARTIAL>` | — | — | The partial to copy. Prints the list when absent |

#### `liyasa theme tokens`

Print the resolved design tokens

### `liyasa update`

Replace this binary with a newer signed release

| Flag | Value | Default | Environment | What it does |
|---|---|---|---|---|
| `--check` | — | — | — | Report what is available and replace nothing |
| `--index` | `<SOURCE>` | — | `LIYASA_UPDATE_INDEX` | The release index to read. A directory or a `file://` URL today |
| `--version` | `<VERSION>` | — | — | Install this version rather than the newest |

### `liyasa validate`

Check configuration, content, links, and specs

| Flag | Value | Default | Environment | What it does |
|---|---|---|---|---|
| `--format` | `<FORMAT>` | `text` | — | — |
| `--links` | — | — | — | Shorthand for `--only links` |
| `--only` | `<SUBSET>` | — | — | Run only these checks. Repeat or comma-separate.  TODO(rfc-0901): CLI-04 spells the third subset `--config`, which is CLI-34's global flag for the configuration path. |
| `--openapi` | — | — | — | Shorthand for `--only openapi` |
| `--personalization` | — | — | — | Also list the pages that are rendered on demand rather than written as files (§6.6.4), so they can be kept few |
| `--strict` | — | — | `LIYASA_STRICT` | Treat warnings as errors |

### `liyasa verify`

Run the verification checks

| Flag | Value | Default | Environment | What it does |
|---|---|---|---|---|
| `--changed` | `<REF>` | — | — | Only pages that changed since this git reference |
| `--format` | `<FORMAT>` | `text` | — | — |
| `--no-cache` | — | — | — | Ignore cached check results |
| `--only` | `<CLASS>` | — | — | Run only these check classes |
| `--refresh` | — | — | — | Re-read every truth source before checking |

### `liyasa version`

Print the version

## Exit codes

| Code | Meaning |
|---|---|
| `0` | Everything asked for succeeded |
| `1` | One or more diagnostics of error severity, or a warning under `--strict` |
| `2` | The command line itself was wrong: an unknown flag, a missing argument |
| `3` | A verification check failed, as distinct from the build failing |
| `4` | A request Liyasa needed to make could not be made |

Verification and network have codes of their own so that CI can tell "the documentation is wrong" from "the check could not run". A job that treats every non-zero exit the same will stop distinguishing them.
