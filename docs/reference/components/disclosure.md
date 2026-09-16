---
title: Disclosure
description: "Accordions, expandables, tabs, and steps: content the reader opens, switches, or follows in order."
---

# Disclosure

Accordions, expandables, tabs, and steps: content the reader opens, switches, or follows in order.

Every example below is rendered by this page, not pasted in as a picture of one: the source is shown and then the same source runs.

## `accordions`

A container component. Also written as `AccordionGroup`, `accordion-group`.

````markdown
::::accordions{one}

:::accordion{title="What does `one` do?"}
Opening one accordion closes the others.
:::

:::accordion{title="When should I use a group?"}
When the items are alternatives rather than a sequence.
:::

::::
````

::::accordions{one}

:::accordion{title="What does `one` do?"}
Opening one accordion closes the others.
:::

:::accordion{title="When should I use a group?"}
When the items are alternatives rather than a sequence.
:::

::::

| Prop | Type | Required | Default | What it does |
|---|---|---|---|---|
| `one` | boolean |  | — | Opening one accordion closes the others. |

## `accordion`

A container component. Also written as `Accordion`.

````markdown
:::accordion{title="Click to open" icon="help-circle"}
An accordion hides detail that most readers do not need, without hiding that it exists.
:::
````

:::accordion{title="Click to open" icon="help-circle"}
An accordion hides detail that most readers do not need, without hiding that it exists.
:::

| Prop | Type | Required | Default | What it does |
|---|---|---|---|---|
| `title` | string |  | — | Summary line the reader clicks. |
| `icon` | icon |  | — | Icon shown before the title. |
| `open` | boolean |  | — | Starts open. |
| `id` | string |  | — | Anchor for the URL hash; defaults to a slug of the title. |

## `expandables`

A container component. Also written as `ExpandableGroup`, `expandable-group`.

````markdown
::::expandables

:::expandable{title="options"}
Nested fields that would otherwise make a table unreadable.
:::

::::
````

::::expandables

:::expandable{title="options"}
Nested fields that would otherwise make a table unreadable.
:::

::::

It takes no props.

## `expandable`

A container component. Also written as `Expandable`.

````markdown
:::expandable{title="Show the full response"}
Expandables are for nested detail inside reference content.
:::
````

:::expandable{title="Show the full response"}
Expandables are for nested detail inside reference content.
:::

| Prop | Type | Required | Default | What it does |
|---|---|---|---|---|
| `title` | string |  | — | Summary line the reader clicks. |
| `open` | boolean |  | — | Starts open. |

## `tabs`

A container component. Also written as `Tabs`, `tab-group`, `TabGroup`.

````markdown
::::tabs{title="Install"}

:::tab{title="npm" sync="npm"}
`npm install liyasa`
:::

:::tab{title="pnpm" sync="pnpm"}
`pnpm add liyasa`
:::

::::
````

::::tabs{title="Install"}

:::tab{title="npm" sync="npm"}
`npm install liyasa`
:::

:::tab{title="pnpm" sync="pnpm"}
`pnpm add liyasa`
:::

::::

| Prop | Type | Required | Default | What it does |
|---|---|---|---|---|
| `title` | string |  | — | Names the group, e.g. `Install`; used to prefix tab titles in the agent output. |
| `sync` | string |  | — | Synchronizes every tab group with the same key site-wide and remembers the reader's choice. |

## `tab`

A container component. Also written as `Tab`.

````markdown
::::tabs

:::tab{title="Linux"}
The musl build is fully static.
:::

:::tab{title="macOS"}
Universal binaries for both architectures.
:::

::::
````

::::tabs

:::tab{title="Linux"}
The musl build is fully static.
:::

:::tab{title="macOS"}
Universal binaries for both architectures.
:::

::::

| Prop | Type | Required | Default | What it does |
|---|---|---|---|---|
| `title` | string |  | — | Tab label. Must say what the tab holds: agents read it flattened. |
| `icon` | icon |  | — | Icon shown before the label. |
| `sync` | string |  | — | Value this tab represents for its group's `sync` key, e.g. `npm`. |

## `steps`

A container component. Also written as `Steps`.

````markdown
::::steps

:::step{title="Install"}
`liyasa new acme-docs`
:::

:::step{title="Run"}
`liyasa dev`
:::

::::
````

::::steps

:::step{title="Install"}
`liyasa new acme-docs`
:::

:::step{title="Run"}
`liyasa dev`
:::

::::

| Prop | Type | Required | Default | What it does |
|---|---|---|---|---|
| `style` | `numbered` \| `icon` |  | `"numbered"` | Whether each step shows its number or its icon. |
| `start` | number |  | — | Number the first step carries. |

## `step`

A container component. Also written as `Step`.

````markdown
::::steps{start=3}

:::step{title="Deploy"}
Steps may start at a number other than one when a procedure continues across pages.
:::

::::
````

::::steps{start=3}

:::step{title="Deploy"}
Steps may start at a number other than one when a procedure continues across pages.
:::

::::

| Prop | Type | Required | Default | What it does |
|---|---|---|---|---|
| `title` | string |  | — | What this step does; becomes the step's anchor. |
| `icon` | icon |  | — | Icon shown in the marker when the group's style is `icon`. |
| `number` | number |  | — | Overrides the number this step would otherwise get. |

Props and types above are generated from each component's own schema, which is what the build validates against: a missing required prop is [`E0314`](/errors/E0314), a wrong type is [`E0315`](/errors/E0315), and an unknown prop is [`W0316`](/errors/W0316).

See [the component reference](/reference/components) for the other groups.
