---
title: Callouts
description: "Notes, warnings, and the rest of the coloured boxes that break a page's flow on purpose."
---

# Callouts

Notes, warnings, and the rest of the coloured boxes that break a page's flow on purpose.

Every example below is rendered by this page, not pasted in as a picture of one: the source is shown and then the same source runs.

## `note`

A container component.

````markdown
:::note{title="Worth knowing"}
A note breaks the flow on purpose. Use it when the reader would otherwise carry on past something that changes what they are doing.
:::
````

:::note{title="Worth knowing"}
A note breaks the flow on purpose. Use it when the reader would otherwise carry on past something that changes what they are doing.
:::

| Prop | Type | Required | Default | What it does |
|---|---|---|---|---|
| `title` | string |  | — | Heading shown above the body; defaults to the callout's name. |
| `icon` | icon |  | — | Overrides the default icon. An empty value removes it. |
| `collapsible` | boolean |  | — | Renders the callout as a disclosure the reader can fold away. |
| `open` | boolean |  | — | Starts a collapsible callout open. |

## `tip`

A container component.

````markdown
:::tip
Tips are for the shortcut a reader would not find on their own.
:::
````

:::tip
Tips are for the shortcut a reader would not find on their own.
:::

| Prop | Type | Required | Default | What it does |
|---|---|---|---|---|
| `title` | string |  | — | Heading shown above the body; defaults to the callout's name. |
| `icon` | icon |  | — | Overrides the default icon. An empty value removes it. |
| `collapsible` | boolean |  | — | Renders the callout as a disclosure the reader can fold away. |
| `open` | boolean |  | — | Starts a collapsible callout open. |

## `warning`

A container component.

````markdown
:::warning{title="This is destructive"}
`liyasa build --clean` empties the output directory before it writes.
:::
````

:::warning{title="This is destructive"}
`liyasa build --clean` empties the output directory before it writes.
:::

| Prop | Type | Required | Default | What it does |
|---|---|---|---|---|
| `title` | string |  | — | Heading shown above the body; defaults to the callout's name. |
| `icon` | icon |  | — | Overrides the default icon. An empty value removes it. |
| `collapsible` | boolean |  | — | Renders the callout as a disclosure the reader can fold away. |
| `open` | boolean |  | — | Starts a collapsible callout open. |

## `info`

A container component.

````markdown
:::info
Information that is useful but not urgent.
:::
````

:::info
Information that is useful but not urgent.
:::

| Prop | Type | Required | Default | What it does |
|---|---|---|---|---|
| `title` | string |  | — | Heading shown above the body; defaults to the callout's name. |
| `icon` | icon |  | — | Overrides the default icon. An empty value removes it. |
| `collapsible` | boolean |  | — | Renders the callout as a disclosure the reader can fold away. |
| `open` | boolean |  | — | Starts a collapsible callout open. |

## `check`

A container component.

````markdown
:::check{title="Verified"}
This sample is executed on every build.
:::
````

:::check{title="Verified"}
This sample is executed on every build.
:::

| Prop | Type | Required | Default | What it does |
|---|---|---|---|---|
| `title` | string |  | — | Heading shown above the body; defaults to the callout's name. |
| `icon` | icon |  | — | Overrides the default icon. An empty value removes it. |
| `collapsible` | boolean |  | — | Renders the callout as a disclosure the reader can fold away. |
| `open` | boolean |  | — | Starts a collapsible callout open. |

## `danger`

A container component.

````markdown
:::danger{title="Data loss"}
Deleting a deployment removes its artifacts; rollback targets are not retained forever.
:::
````

:::danger{title="Data loss"}
Deleting a deployment removes its artifacts; rollback targets are not retained forever.
:::

| Prop | Type | Required | Default | What it does |
|---|---|---|---|---|
| `title` | string |  | — | Heading shown above the body; defaults to the callout's name. |
| `icon` | icon |  | — | Overrides the default icon. An empty value removes it. |
| `collapsible` | boolean |  | — | Renders the callout as a disclosure the reader can fold away. |
| `open` | boolean |  | — | Starts a collapsible callout open. |

## `callout`

A container component.

````markdown
:::callout{title="A callout in your own colour" icon="sparkles" color="#7c3aed"}
When none of the six named kinds fit, `callout` takes an icon and a colour.
:::
````

:::callout{title="A callout in your own colour" icon="sparkles" color="#7c3aed"}
When none of the six named kinds fit, `callout` takes an icon and a colour.
:::

| Prop | Type | Required | Default | What it does |
|---|---|---|---|---|
| `title` | string |  | — | Heading shown above the body. |
| `icon` | icon |  | — | Icon shown beside the title. |
| `color` | colour |  | — | Accent colour: a theme token name or a hex value. |
| `variant` | `soft` \| `outline` \| `solid` |  | `"soft"` | How strongly the colour is applied. |
| `collapsible` | boolean |  | — | Renders the callout as a disclosure the reader can fold away. |
| `open` | boolean |  | — | Starts a collapsible callout open. |

Props and types above are generated from each component's own schema, which is what the build validates against: a missing required prop is [`E0314`](/errors/E0314), a wrong type is [`E0315`](/errors/E0315), and an unknown prop is [`W0316`](/errors/W0316).

See [the component gallery](/reference/gallery) for the other groups.
