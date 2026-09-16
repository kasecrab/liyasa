---
title: CLI reference
description: Every Liyasa command, what it does, and the flags that matter.
---

# CLI reference

Every command takes `--project` to point at a directory other than the current
one, `--config` to name a configuration file, and `--quiet`, `--verbose`, or
`--json` to choose how output is written. `--json` emits one diagnostic object
per line, with the code, the file, the span, and the help URL, which is what CI
should consume.

```sh
liyasa <command> --help
```

prints the same text as the sections below, generated from the same source.

## Authoring

### `liyasa new`

Creates a project: configuration with the schema reference in place, a
navigation tree, and pages that demonstrate front matter, components, and
verified code samples. Runnable in under ten seconds.

| Flag | Default | What it does |
|---|---|---|
| `--template` | `docs` | A named starter template, a path, or a URL |
| `--preset` | `aurora` | The theme preset to start from |
| `--yes` | `false` | Takes every default instead of asking |

### `liyasa dev`

Starts the development server, watches the project, and rebuilds what changed.
The build memoizes every query it makes, so one edit re-renders that page and
the pages whose content depends on it, not the site.

| Flag | Default | What it does |
|---|---|---|
| `--port` | `3000` | Port to listen on |
| `--host` | `127.0.0.1` | Interface to bind |
| `--open` | `false` | Opens a browser once the first build finishes |
| `--drafts` | `false` | Includes pages marked `draft: true` |
| `--verify` | `false` | Runs verification in watch mode |
| `--groups` | — | Mocks a reader's groups, for gated content |
| `--region` | — | Mocks a reader's region |

### `liyasa format`

Canonical formatting of Markdown, front matter, and configuration.

| Flag | Default | What it does |
|---|---|---|
| `--check` | `false` | Exits non-zero instead of writing; for CI |
| `--directives` | `false` | Converts the tag form to the directive form |

## Building and checking

### `liyasa build`

Incremental production build into `dist/`: HTML, the Markdown twin of every
page, the search index, `llms.txt`, a sitemap, redirects, and the header files
static hosts read.

| Flag | Default | What it does |
|---|---|---|
| `--clean` | `false` | Starts from an empty output directory and cache |
| `--env` | — | The configuration overlay to merge |
| `--base-path` | — | Overrides `build.basePath` |
| `--drafts` | `false` | Includes pages marked `draft: true` |
| `--strict` | `false` | Warnings become errors |
| `--profile` | `false` | Prints a timing per phase |
| `--check-determinism` | `false` | Builds twice and compares |

### `liyasa validate`

Configuration, front matter, content, components, links, OpenAPI, and
navigation. Everything that can be checked without leaving the repository.

| Flag | Default | What it does |
|---|---|---|
| `--config`, `--links`, `--openapi` | — | Run one subset |
| `--format` | `human` | `human`, `json`, or `sarif` |

### `liyasa verify`

The checks that reach outside the repository: executing code samples,
refreshing fact sources, resolving external links, comparing screenshots, and
prose rules. See [verification](/guides/verification).

| Flag | Default | What it does |
|---|---|---|
| `--only` | — | `code`, `facts`, `links`, `screenshots`, or `prose` |
| `--refresh` | `false` | Re-reads every source, ignoring its interval |
| `--no-cache` | `false` | Ignores cached results |
| `--changed` | — | Only pages changed since a git ref |
| `--format` | `human` | `human`, `json`, or `sarif` |

### `liyasa broken-links`

Internal and external link checking with concurrency, timeouts, and an allow
list. A familiar alias for a subset of `verify`.

### `liyasa test`

| Flag | What it does |
|---|---|
| `--a11y` | Accessibility checks; a real browser with the companion runtime |
| `--perf` | Lighthouse budgets; requires the companion runtime |
| `--agents` | The agent-readiness checks, against the built output |
| `--search` | Search assertions from `tests/search.toml` |

### `liyasa score`

Prints the documentation quality score with its sub-scores and the top actions
that would raise it.

## Publishing

### `liyasa export`

| Flag | What it does |
|---|---|
| `--static` | The default: a directory, as `build` produces |
| `--offline` | A bundle that works from the file system |
| `--markdown` | Only the `.md` files and `llms.txt` |
| `--pdf` | Requires the companion runtime |
| `--zip` | Archives the result |

### `liyasa deploy`

Pushes a build to a server from CI or a laptop. Prints the URL and the
deployment identifier.

| Flag | Default | What it does |
|---|---|---|
| `--env` | `production` | `production` or `preview` |
| `--message` | — | A note recorded with the deployment |
| `--gh-pages` | `false` | Publishes to a GitHub Pages branch instead |

`liyasa deployments list`, `status`, and `rollback` manage what has been
deployed.

### `liyasa domain`

`add`, `verify`, and `remove`, with an optional base path. See
[domains](/help/domains).

### `liyasa serve`

Runs the server: private sites, content negotiation, on-demand rendering,
analytics ingest, and webhooks.

| Flag | What it does |
|---|---|
| `--listen`, `--tls` | Where and how to listen |
| `--db`, `--storage` | Where state and artifacts live |
| `--init` | First-time setup |
| `--collector-only` | Only the analytics ingest endpoint, for static sites |

## Working with content

### `liyasa search`

Queries the local index. `--json` for structured results, `--expect <url>` to
assert that a query returns a page, which is how search regressions are caught
in CI.

### `liyasa import`

`mintlify`, `docusaurus`, `gitbook`, `readme`, `fern`, `document360`, or `mdx`.

### `liyasa schema`

Prints a JSON Schema: `config`, `frontmatter`, or `components`.

### `liyasa migrate-config`

Upgrades configuration between schema versions.

### `liyasa add`

Installs a `component`, `theme`, or `runner` pack from git or a registry.

### `liyasa theme`

`eject`, `diff`, and `tokens`, for working with theme overrides.

### `liyasa agent run`

Runs the writing agent against the working copy with your own keys. Produces a
diff; never commits without `--commit`.

## Environment

### `liyasa doctor`

Checks the toolchain, sandbox availability, the companion runtime, the
reachability of configured sources, and cache health. The first thing to run
when something does not work.

### `liyasa companion install`

Fetches the pinned browser build the optional features use.

### `liyasa index`

Configures editors and coding agents with the site's Model Context Protocol
server and rules, `--global` or project-local.

### `liyasa login`

`login`, `logout`, `status`, and `whoami`. A device-code flow; the token is
stored in the operating system keychain.

### `liyasa lsp`

The language server: completion for components and props, go-to-definition for
snippets and links, diagnostics as you type.

### `liyasa update`

Self-update with signature verification. `liyasa version` prints the version,
`liyasa telemetry on|off|status` controls opt-in anonymous telemetry, which is
off by default, and `liyasa completions <shell>` writes shell completions.

## Exit codes

| Code | Meaning |
|---|---|
| `0` | Success |
| `1` | One or more diagnostics of error severity |
| `2` | Usage error: an unknown flag or a missing argument |

A warning does not change the exit code unless `--strict` is set.
