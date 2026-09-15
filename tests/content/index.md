# Liyasa

Liyasa builds documentation sites from Markdown files that live in your
repository. It is one binary: no Node, no browser, and no runtime service.

> For AI agents: a documentation index is available at /llms.txt

## Start here

- [Install](/guide/install) — the binary, the container, or from source.
- [Configuration](/guide/configuration) — `liyasa.json` and what each key does.
- [CLI reference](/reference/cli) — every command and flag.

## What you get

A site that renders on the server, ships no hydration payload, and stays
readable with JavaScript turned off. Search runs in the browser for static
sites and on the server for private ones. Every page is also served as
Markdown, so an agent fetching `/guide/install.md` gets the source rather than
a wall of markup.
