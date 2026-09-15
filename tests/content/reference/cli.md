# CLI reference

Every command takes `--project` to point at a directory other than the current
one, `--config` to name a configuration file, and `--quiet`, `--verbose`, or
`--json` to choose how output is written. `--json` emits one diagnostic object
per line, which is what CI should consume.

## `liyasa new`

Creates a project: configuration with the schema reference in place, a
navigation tree, and three pages that demonstrate front matter, components,
and verified code samples.

| Flag | Default | What it does |
|---|---|---|
| `--template` | `docs` | `docs`, `api`, or a path to a local template |
| `--preset` | `aurora` | The theme preset to start from |
| `--git` | `true` | Initializes a repository and writes `.gitignore` |

## `liyasa dev`

Starts the development server, watches the project, and rebuilds what changed.
The rebuild is incremental: the build memoizes every query it makes, so a
single page edit re-renders that page and the pages whose content depends on
it, not the site.

| Flag | Default | What it does |
|---|---|---|
| `--port` | `3000` | Port to listen on |
| `--host` | `127.0.0.1` | Interface to bind |
| `--open` | `false` | Opens a browser once the first build finishes |
| `--drafts` | `false` | Includes pages marked `draft: true` |

## `liyasa build`

Builds the site into `dist/`: HTML, the Markdown twin of every page, the
search index, `llms.txt`, a sitemap, redirects, and the header files static
hosts read.

| Flag | Default | What it does |
|---|---|---|
| `--out` | `dist` | Output directory |
| `--base-path` | from config | Subpath the site is served under |
| `--clean` | `false` | Empties the output directory first |
| `--fail-on` | `error` | `error`, `warning`, or `never` |

The build is deterministic. Two builds of the same commit with the same
configuration produce byte-identical output, which is what makes the artifact
cache safe to share between CI runs.

## `liyasa test`

Runs the checks that hold a site to its budgets and to the agent-readiness
specification.

| Flag | What it runs |
|---|---|
| `--agents` | The 28 agent-readiness checks against the built output |
| `--a11y` | Static accessibility checks, and axe-core with the companion |
| `--links` | Internal and external link resolution |
| `--perf` | Lighthouse budgets, companion runtime only |

`--agents` prints served bytes, converted characters, and the ratio between
them for every page, so a page that grows past a budget is visible in the diff
of a report rather than in a reader's loading spinner.

## `liyasa verify`

Runs the verification engine: code samples are executed, facts are re-fetched
from their sources, API examples are checked against the specification, and
claims that no longer hold are reported with the evidence that contradicts
them.

| Flag | Default | What it does |
|---|---|---|
| `--runner` | all | Restricts the run to one runner |
| `--changed` | `false` | Only checks what the working tree changed |
| `--format` | `text` | `text`, `json`, `sarif`, or `junit` |
| `--sandbox` | `auto` | Container runtime for code-executing runners |

## `liyasa export`

Writes the site in another form.

| Flag | What it writes |
|---|---|
| `--markdown` | One Markdown file per page, with front matter |
| `--pdf` | A PDF from the print stylesheet, companion runtime only |
| `--offline` | A self-contained archive with search included |

## `liyasa doctor`

Reports the environment: version, cache directory, whether the companion
runtime is installed, whether a container runtime is available, and how much
of the cache is in use. It exits zero when nothing is wrong and prints a
diagnostic per problem when something is.

## Exit codes

| Code | Meaning |
|---|---|
| `0` | Everything the command ran succeeded |
| `1` | A diagnostic at or above `--fail-on` was emitted |
| `2` | The command was used incorrectly |
| `3` | The project or configuration could not be read |
