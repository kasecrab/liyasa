---
title: Code
description: "Code groups, terminals, inline code, and samples pulled from a file that the build keeps in sync."
---

# Code

Code groups, terminals, inline code, and samples pulled from a file that the build keeps in sync.

Every example below is rendered by this page, not pasted in as a picture of one: the source is shown and then the same source runs.

## `code-group`

A container component. Also written as `CodeGroup`, `codegroup`.

````markdown
::::code-group

```sh {title="npm"}
npm install liyasa
```

```sh {title="cargo"}
cargo install liyasa
```

::::
````

::::code-group

```sh {title="npm"}
npm install liyasa
```

```sh {title="cargo"}
cargo install liyasa
```

::::

| Prop | Type | Required | Default | What it does |
|---|---|---|---|---|
| `sync` | string | — | — | Synchronizes every group with the same key site-wide and remembers the reader's choice. |
| `dropdown` | boolean | — | — | Shows a select instead of a row of tabs. |

## `code`

A inline component. Also written as `Code`.

````markdown
Press :code[liyasa build]{lang="sh"} to write `dist/`.
````

Press :code[liyasa build]{lang="sh"} to write `dist/`.

| Prop | Type | Required | Default | What it does |
|---|---|---|---|---|
| `lang` | string | — | — | Language the span is highlighted as. |

## `terminal`

A container component. Also written as `Terminal`.

````markdown
:::terminal{title="A session"}
liyasa build
liyasa verify
:::
````

:::terminal{title="A session"}
liyasa build
liyasa verify
:::

| Prop | Type | Required | Default | What it does |
|---|---|---|---|---|
| `title` | string | — | — | Window title shown above the session. |
| `prompt` | string | — | `"$ "` | Prompt prefix; the copy button removes it. |

## `snippet-from`

A leaf component. Also written as `SnippetFrom`, `code-from`.

````markdown
::snippet-from{file="README.md" lines="1-3" title="README.md"}
````

::snippet-from{file="README.md" lines="1-3" title="README.md"}

| Prop | Type | Required | Default | What it does |
|---|---|---|---|---|
| `file` | string | yes | — | Path to the file, relative to the repository root. |
| `lines` | string | — | — | Line range, e.g. `10-25`. Mutually exclusive with `symbol`. |
| `symbol` | string | — | — | Name of a `// [liyasa:start name]` marker region, or of a symbol the language server can find. |
| `repo` | string | — | — | Connected repository the file lives in; defaults to this one. |
| `ref` | string | — | — | Branch, tag, or commit to read the file at. |
| `lang` | string | — | — | Language to highlight as; defaults to the file's extension. |
| `title` | string | — | — | Title shown above the block; defaults to the file path. |

Props and types above are generated from each component's own schema, which is what the build validates against: a missing required prop is [`E0314`](/errors/E0314), a wrong type is [`E0315`](/errors/E0315), and an unknown prop is [`W0316`](/errors/W0316).

See [the component gallery](/reference/gallery) for the other groups.
