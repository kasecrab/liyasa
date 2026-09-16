---
title: Media
description: "Images, video, embedded frames, downloads, and screenshots that verification can re-capture."
---

# Media

Images, video, embedded frames, downloads, and screenshots that verification can re-capture.

Every example below is rendered by this page, not pasted in as a picture of one: the source is shown and then the same source runs.

## `image`

A leaf component. Also written as `Image`, `img`.

````markdown
::image{src="/assets/example.svg" alt="A rectangle labelled example" width=480 height=180 caption="Images carry their dimensions so the page does not shift as they load"}
````

::image{src="/assets/example.svg" alt="A rectangle labelled example" width=480 height=180 caption="Images carry their dimensions so the page does not shift as they load"}

| Prop | Type | Required | Default | What it does |
|---|---|---|---|---|
| `src` | asset | yes | — | The image. A path under `assets/`, or an absolute URL. |
| `alt` | string | yes | — | What the image says, for a reader who cannot see it. Empty only when the image is decorative. |
| `dark` | asset |  | — | Variant shown in dark mode. |
| `width` | number |  | — | Intrinsic width in pixels; prevents layout shift. |
| `height` | number |  | — | Intrinsic height in pixels; prevents layout shift. |
| `caption` | string |  | — | Caption shown under the image. |
| `zoom` | boolean |  | `true` | Opens the image full size when clicked. |
| `align` | `left` \| `center` \| `right` \| `full` |  | `"center"` | How the image sits in the text column. |
| `border` | boolean |  | — | Draws a border around the image. |

## `video`

A leaf component. Also written as `Video`.

````markdown
::video{src="/assets/example.mp4" poster="/assets/example.svg" caption="A short clip" controls=true}
````

::video{src="/assets/example.mp4" poster="/assets/example.svg" caption="A short clip" controls=true}

| Prop | Type | Required | Default | What it does |
|---|---|---|---|---|
| `src` | asset | yes | — | The video file, or a YouTube, Vimeo, or Loom URL. |
| `poster` | asset |  | — | Still shown before the video plays. |
| `autoplay` | boolean |  | — | Plays as soon as it is visible. Requires `muted`. |
| `loop` | boolean |  | — | Restarts when it ends. |
| `muted` | boolean |  | — | Starts with no sound. |
| `controls` | boolean |  | `true` | Shows the player's controls. |
| `caption` | string |  | — | Caption shown under the video. |
| `title` | string |  | — | Accessible name for an embedded player. |

## `iframe`

A leaf component. Also written as `Iframe`, `IFrame`.

````markdown
::iframe{src="/reference/cli" title="The CLI reference, embedded" height="240px"}
````

::iframe{src="/reference/cli" title="The CLI reference, embedded" height="240px"}

| Prop | Type | Required | Default | What it does |
|---|---|---|---|---|
| `src` | route | yes | — | The page to frame. |
| `title` | string | yes | — | What the frame holds. A screen reader announces this instead of the frame. |
| `height` | string |  | — | CSS height, e.g. `480px`. |
| `allow` | string |  | — | Permissions policy for the frame, e.g. `clipboard-write`. |

## `embed`

A leaf component. Also written as `Embed`.

````markdown
::embed{url="https://www.youtube.com/watch?v=dQw4w9WgXcQ" title="An embedded video"}
````

::embed{url="https://www.youtube.com/watch?v=dQw4w9WgXcQ" title="An embedded video"}

| Prop | Type | Required | Default | What it does |
|---|---|---|---|---|
| `url` | route | yes | — | The page to embed. Must be from an allow-listed provider. |
| `title` | string |  | — | Accessible name for the frame; defaults to the provider's name. |
| `height` | string |  | — | CSS height, e.g. `480px`. |

## `file`

A leaf component. Also written as `File`.

````markdown
::file{src="/assets/example.svg" name="example.svg" size="1 KB" type="SVG"}
````

::file{src="/assets/example.svg" name="example.svg" size="1 KB" type="SVG"}

| Prop | Type | Required | Default | What it does |
|---|---|---|---|---|
| `src` | asset | yes | — | The file to download. |
| `name` | string |  | — | Name shown on the card; defaults to the file name. |
| `size` | string |  | — | Size shown on the card, e.g. `2.4 MB`. |
| `type` | string |  | — | File type shown on the card; defaults to the extension. |

## `files`

A container component. Also written as `Files`.

````markdown
::::files

::file{src="/assets/example.svg" name="example.svg"}

::::
````

::::files

::file{src="/assets/example.svg" name="example.svg"}

::::

It takes no props.

## `screenshot`

A leaf component. Also written as `Screenshot`.

````markdown
::screenshot{src="/assets/example.svg" alt="The deployment list" app="dashboard" route="/deployments" viewport="1280x800"}
````

::screenshot{src="/assets/example.svg" alt="The deployment list" app="dashboard" route="/deployments" viewport="1280x800"}

| Prop | Type | Required | Default | What it does |
|---|---|---|---|---|
| `src` | asset | yes | — | Where the capture is stored; the automation writes it. |
| `alt` | string | yes | — | What the screenshot shows. |
| `app` | string |  | — | Which application to capture, as named in the verification config. |
| `route` | route |  | — | Route within that application. |
| `selector` | string |  | — | CSS selector to crop to. |
| `viewport` | string |  | — | Viewport to capture at, e.g. `1280x800`. |
| `caption` | string |  | — | Caption shown under the screenshot. |

Props and types above are generated from each component's own schema, which is what the build validates against: a missing required prop is [`E0314`](/errors/E0314), a wrong type is [`E0315`](/errors/E0315), and an unknown prop is [`W0316`](/errors/W0316).

See [the component gallery](/reference/gallery) for the other groups.
