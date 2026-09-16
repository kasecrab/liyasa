---
title: Layout
description: "Cards, columns, tiles, frames, and the components that arrange a page rather than carry content."
---

# Layout

Cards, columns, tiles, frames, and the components that arrange a page rather than carry content.

Every example below is rendered by this page, not pasted in as a picture of one: the source is shown and then the same source runs.

## `cards`

A container component. Also written as `card-group`, `CardGroup`, `Cards`.

````markdown
::::cards{cols=2}

:::card{title="Install" href="/getting-started/install" icon="download"}
One binary, no runtime.
:::

:::card{title="Quickstart" href="/getting-started/quickstart" icon="rocket"}
A site in a minute.
:::

::::
````

::::cards{cols=2}

:::card{title="Install" href="/getting-started/install" icon="download"}
One binary, no runtime.
:::

:::card{title="Quickstart" href="/getting-started/quickstart" icon="rocket"}
A site in a minute.
:::

::::

| Prop | Type | Required | Default | What it does |
|---|---|---|---|---|
| `cols` | number | — | `2` | Columns in the grid, 1 to 4. |
| `gap` | string | — | — | Space between cards: a theme spacing token or a CSS length. |

## `card`

A container component. Also written as `Card`.

````markdown
:::card{title="A single card" href="/reference/cli" icon="terminal" cta="Read the reference" arrow=true}
A card with a call to action links its whole surface.
:::
````

:::card{title="A single card" href="/reference/cli" icon="terminal" cta="Read the reference" arrow=true}
A card with a call to action links its whole surface.
:::

| Prop | Type | Required | Default | What it does |
|---|---|---|---|---|
| `title` | string | — | — | Card heading. |
| `icon` | icon | — | — | Icon shown above or beside the title. |
| `href` | route | — | — | Makes the whole card a link to this route or URL. |
| `img` | asset | — | — | Image shown on top, or on the left when `horizontal`. |
| `horizontal` | boolean | — | — | Lays the image beside the body instead of above it. |
| `cta` | string | — | — | Call-to-action text shown at the foot of the card. |
| `color` | colour | — | — | Accent colour: a theme token name or a hex value. |
| `arrow` | boolean | — | — | Shows an arrow beside the call to action. |

## `columns`

A container component. Also written as `Columns`.

````markdown
::::columns{cols=2}

:::column
Columns arrange content side by side and collapse to one column on a narrow screen.
:::

:::column
They carry no meaning of their own, so do not let a distinction live only in which column something is in.
:::

::::
````

::::columns{cols=2}

:::column
Columns arrange content side by side and collapse to one column on a narrow screen.
:::

:::column
They carry no meaning of their own, so do not let a distinction live only in which column something is in.
:::

::::

| Prop | Type | Required | Default | What it does |
|---|---|---|---|---|
| `cols` | number | — | `2` | Columns in the grid, 1 to 4. |
| `gap` | string | — | — | Space between columns: a theme spacing token or a CSS length. |
| `align` | `start` \| `center` \| `end` \| `stretch` | — | `"stretch"` | How columns line up against each other vertically. |

## `column`

A container component. Also written as `Column`.

````markdown
::::columns{cols=2}

:::column{span=1}
A column may span more than one track.
:::

:::column
The rest of the row.
:::

::::
````

::::columns{cols=2}

:::column{span=1}
A column may span more than one track.
:::

:::column
The rest of the row.
:::

::::

| Prop | Type | Required | Default | What it does |
|---|---|---|---|---|
| `span` | number | — | — | Columns this one spans, 1 to 4. |

## `tiles`

A container component. Also written as `Tiles`.

````markdown
::::tiles{cols=3}

:::tile{title="Build" icon="hammer"}
`liyasa build`
:::

:::tile{title="Verify" icon="shield-check"}
`liyasa verify`
:::

:::tile{title="Deploy" icon="upload"}
`liyasa deploy`
:::

::::
````

::::tiles{cols=3}

:::tile{title="Build" icon="hammer"}
`liyasa build`
:::

:::tile{title="Verify" icon="shield-check"}
`liyasa verify`
:::

:::tile{title="Deploy" icon="upload"}
`liyasa deploy`
:::

::::

| Prop | Type | Required | Default | What it does |
|---|---|---|---|---|
| `cols` | number | — | `3` | Columns in the grid, 1 to 4. |

## `tile`

A container component. Also written as `Tile`.

````markdown
::::tiles{cols=2}

:::tile{title="A tile" icon="square"}
Tiles are denser than cards and are meant to be scanned.
:::

:::tile{title="Another" icon="square"}
Use them for a grid of short links.
:::

::::
````

::::tiles{cols=2}

:::tile{title="A tile" icon="square"}
Tiles are denser than cards and are meant to be scanned.
:::

:::tile{title="Another" icon="square"}
Use them for a grid of short links.
:::

::::

| Prop | Type | Required | Default | What it does |
|---|---|---|---|---|
| `title` | string | — | — | Tile label. |
| `icon` | icon | — | — | Icon shown above the label. |
| `href` | route | — | — | Makes the tile a link to this route or URL. |

## `frame`

A container component. Also written as `Frame`.

````markdown
:::frame{caption="A framed figure" hint="Frames add a border and a caption"}
Anything inside a frame is presented as a figure.
:::
````

:::frame{caption="A framed figure" hint="Frames add a border and a caption"}
Anything inside a frame is presented as a figure.
:::

| Prop | Type | Required | Default | What it does |
|---|---|---|---|---|
| `caption` | string | — | — | Caption shown under the frame. |
| `hint` | string | — | — | Smaller note under the caption. |
| `video` | boolean | — | — | Frames a video rather than an image: no zoom, and the aspect ratio is kept. |
| `align` | `left` \| `center` \| `right` \| `full` | — | `"center"` | How the frame sits in the text column. |

## `panel`

A container component. Also written as `Panel`.

````markdown
:::panel
A panel is a plain surface: no icon, no colour, just separation from the page.
:::
````

:::panel
A panel is a plain surface: no icon, no colour, just separation from the page.
:::

It takes no props.

## `hero`

A container component. Also written as `Hero`.

````markdown
:::hero{title="Liyasa" subtitle="Documentation that stays true"}
A hero renders its title as the page's heading.
:::
````

:::hero{title="Liyasa" subtitle="Documentation that stays true"}
A hero renders its title as the page's heading.
:::

| Prop | Type | Required | Default | What it does |
|---|---|---|---|---|
| `title` | string | — | — | Headline, rendered as the page's H1. |
| `subtitle` | string | — | — | Sentence under the headline. |
| `image` | asset | — | — | Image or illustration beside the text. |
| `actions` | string[] | — | — | Buttons, each `Label -> /route`; the first is the primary action. |

## `divider`

A leaf component. Also written as `Divider`.

````markdown
::divider{label="Reference"}
````

::divider{label="Reference"}

| Prop | Type | Required | Default | What it does |
|---|---|---|---|---|
| `label` | string | — | — | Text shown in the middle of the rule. |

Props and types above are generated from each component's own schema, which is what the build validates against: a missing required prop is [`E0314`](/errors/E0314), a wrong type is [`E0315`](/errors/E0315), and an unknown prop is [`W0316`](/errors/W0316).

See [the component gallery](/reference/gallery) for the other groups.
