---
title: Page
description: "Banners, changelog entries, region gates, feedback, and the components that act on the page as a whole."
---

# Page

Banners, changelog entries, region gates, feedback, and the components that act on the page as a whole.

Every example below is rendered by this page, not pasted in as a picture of one: the source is shown and then the same source runs.

## `banner`

A container component. Also written as `Banner`.

````markdown
:::banner{color="#4338CA" dismissible=true id="gallery-banner"}
A banner sits above the page content and can be dismissed for good.
:::
````

:::banner{color="#4338CA" dismissible=true id="gallery-banner"}
A banner sits above the page content and can be dismissed for good.
:::

| Prop | Type | Required | Default | What it does |
|---|---|---|---|---|
| `color` | colour | — | — | Accent colour: a theme token name or a hex value. |
| `dismissible` | boolean | — | — | Lets the reader close the banner; `id` is what remembers that. |
| `id` | string | — | — | Identifies the banner so a dismissal is remembered across pages. |

## `update`

A container component. Also written as `Update`.

````markdown
:::update{date="2026-09-01" version="0.1" title="First release"}
Changelog entries carry a date, a version, and a stable anchor.
:::
````

:::update{date="2026-09-01" version="0.1" title="First release"}
Changelog entries carry a date, a version, and a stable anchor.
:::

| Prop | Type | Required | Default | What it does |
|---|---|---|---|---|
| `date` | string | yes | — | Release date, `YYYY-MM-DD`. |
| `version` | string | — | — | Version this entry describes. |
| `labels` | string[] | — | — | Tags the entry is filtered by, e.g. `breaking` or `api`. |
| `title` | string | — | — | Headline for the entry. |

## `prompt`

A container component. Also written as `Prompt`.

````markdown
:::prompt{title="Ask an assistant"}
Explain how Liyasa verifies code samples.
:::
````

:::prompt{title="Ask an assistant"}
Explain how Liyasa verifies code samples.
:::

| Prop | Type | Required | Default | What it does |
|---|---|---|---|---|
| `title` | string | — | — | Headline above the prompt. |
| `open` | `cursor` \| `claude` \| `chatgpt`[] | — | — | Assistants to offer an `open in` button for. |

## `github`

A leaf component. Also written as `GitHub`, `Github`.

````markdown
::github{repo="kasecrab/liyasa"}
````

::github{repo="kasecrab/liyasa"}

| Prop | Type | Required | Default | What it does |
|---|---|---|---|---|
| `repo` | string | yes | — | Repository as `owner/name`. |

## `md`

A container component. Also written as `Md`, `markdown`.

````markdown
:::md
Raw Markdown, passed through without component processing.
:::
````

:::md
Raw Markdown, passed through without component processing.
:::

It takes no props.

## `visibility`

A container component. Also written as `Visibility`.

````markdown
:::visibility{humans=true agents=false}
This paragraph is in the HTML and not in the Markdown output.
:::
````

:::visibility{humans=true agents=false}
This paragraph is in the HTML and not in the Markdown output.
:::

| Prop | Type | Required | Default | What it does |
|---|---|---|---|---|
| `humans` | boolean | — | — | Include in the HTML output. |
| `agents` | boolean | — | — | Include in the Markdown output. |
| `groups` | string[] | — | — | Authenticated groups that may see it. |
| `regions` | string[] | — | — | Regions it is shown in. |
| `locales` | string[] | — | — | Locales it is shown in. |
| `versions` | string[] | — | — | Versions it is shown in. |

## `region`

A container component. Also written as `Region`.

````markdown
:::region{only=[us,eu]}
Payments settle through our United States entity.
:::
````

:::region{only=[us,eu]}
Payments settle through our United States entity.
:::

| Prop | Type | Required | Default | What it does |
|---|---|---|---|---|
| `only` | string[] | — | — | Regions this block is shown in. |
| `except` | string[] | — | — | Regions this block is hidden in. |

## `feedback`

A leaf component. Also written as `Feedback`.

````markdown
::feedback{question="Was this page useful?"}
````

::feedback{question="Was this page useful?"}

| Prop | Type | Required | Default | What it does |
|---|---|---|---|---|
| `question` | string | — | `"Was this helpful?"` | What the reader is asked. |

## `assistant`

A leaf component. Also written as `Assistant`.

````markdown
::assistant{prompt="How do I add a second locale?" label="Ask about locales"}
````

::assistant{prompt="How do I add a second locale?" label="Ask about locales"}

| Prop | Type | Required | Default | What it does |
|---|---|---|---|---|
| `prompt` | string | yes | — | The question the assistant opens with. |
| `label` | string | — | — | Text on the button; defaults to the prompt. |

## `tree`

A container component. Also written as `Tree`, `file-tree`, `FileTree`.

````markdown
:::tree{root="my-docs"}
- liyasa.json
- index.md
- guides/
  - install.md
:::
````

:::tree{root="my-docs"}
- liyasa.json
- index.md
- guides/
  - install.md
:::

| Prop | Type | Required | Default | What it does |
|---|---|---|---|---|
| `root` | string | — | — | Label for the top of the tree, e.g. the repository name. |
| `active` | string | — | — | Path highlighted as the file being described. |
| `expanded` | boolean | — | — | Opens every folder. |

## `toc`

A leaf component. Also written as `Toc`, `TableOfContents`.

````markdown
::toc{depth=2}
````

::toc{depth=2}

| Prop | Type | Required | Default | What it does |
|---|---|---|---|---|
| `depth` | number | — | `3` | Deepest heading level listed, 1 to 6. |
| `from` | route | — | — | Lists the pages under this route instead of the headings on this page. |

Props and types above are generated from each component's own schema, which is what the build validates against: a missing required prop is [`E0314`](/errors/E0314), a wrong type is [`E0315`](/errors/E0315), and an unknown prop is [`W0316`](/errors/W0316).

See [the component gallery](/reference/gallery) for the other groups.
