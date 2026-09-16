---
title: Inline
description: "Badges, icons, keys, colours, tooltips, and facts: components that sit inside a sentence."
---

# Inline

Badges, icons, keys, colours, tooltips, and facts: components that sit inside a sentence.

Every example below is rendered by this page, not pasted in as a picture of one: the source is shown and then the same source runs.

## `badge`

A inline component. Also written as `Badge`.

````markdown
Rate limits apply to every plan :badge[beta]{color="#7c3aed"}.
````

Rate limits apply to every plan :badge[beta]{color="#7c3aed"}.

| Prop | Type | Required | Default | What it does |
|---|---|---|---|---|
| `color` | colour | — | — | Accent colour: a theme token name or a hex value. |
| `variant` | `soft` \| `outline` \| `solid` | — | `"soft"` | How strongly the colour is applied. |
| `icon` | icon | — | — | Icon shown before the label. |

## `icon`

A inline component. Also written as `Icon`.

````markdown
Builds that succeed are marked :icon{name="check" label="passed"} in the list.
````

Builds that succeed are marked :icon{name="check" label="passed"} in the list.

| Prop | Type | Required | Default | What it does |
|---|---|---|---|---|
| `name` | icon | yes | — | Icon name in the chosen set. |
| `type` | string | — | — | Icon set the name comes from. |
| `size` | number | — | — | Size in pixels; defaults to the surrounding text's size. |
| `color` | colour | — | — | Colour: a theme token name or a hex value. |
| `label` | string | — | — | Accessible name. Without it the icon is decorative and screen readers skip it. |

## `kbd`

A inline component. Also written as `Kbd`.

````markdown
Press :kbd[Ctrl+K] to open search.
````

Press :kbd[Ctrl+K] to open search.

It takes no props.

## `color`

A inline component. Also written as `Color`, `Colour`, `colour`.

````markdown
The default accent is :color{value="#4338CA" name="indigo"}.
````

The default accent is :color{value="#4338CA" name="indigo"}.

| Prop | Type | Required | Default | What it does |
|---|---|---|---|---|
| `value` | colour | yes | — | The colour, as a CSS value. |
| `name` | string | — | — | What the colour is called; shown beside the swatch. |

## `tooltip`

A inline component. Also written as `Tooltip`.

````markdown
A :tooltip[fact]{text="A named value with a source of truth"} is checked on every build.
````

A :tooltip[fact]{text="A named value with a source of truth"} is checked on every build.

| Prop | Type | Required | Default | What it does |
|---|---|---|---|---|
| `text` | string | yes | — | What the tooltip says. |
| `href` | route | — | — | Makes the anchor a link as well as a tooltip. |

## `fact`

A inline component. Also written as `Fact`.

````markdown
The Pro plan allows :fact{id="limits.api.requests_per_minute"} requests per minute.
````

The Pro plan allows :fact{id="limits.api.requests_per_minute"} requests per minute.

| Prop | Type | Required | Default | What it does |
|---|---|---|---|---|
| `id` | string | yes | — | The fact's ID, as declared under `facts/`. |
| `format` | string | — | — | How to render the value, e.g. `currency` or `date`. |

Props and types above are generated from each component's own schema, which is what the build validates against: a missing required prop is [`E0314`](/errors/E0314), a wrong type is [`E0315`](/errors/E0315), and an unknown prop is [`W0316`](/errors/W0316).

See [the component gallery](/reference/gallery) for the other groups.
