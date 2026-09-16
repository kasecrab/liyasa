---
title: Component gallery
description: "Every built-in component, with its props and a live example of each."
---

# Component gallery

Components are written as directives, so a page that uses them is still readable Markdown and still reviewable in a diff.

```markdown
:::note{title="A container"}
Three colons open and close it.
:::

::image{src="/assets/example.svg" alt="A leaf component takes no children"}

An :kbd[inline] component sits inside a sentence.
```

Nest a container inside another by giving the outer one more colons. Props are written in braces: strings are quoted, numbers and booleans are not, and a list is written `[a,b]`.

Every component also has a Markdown serialization for agents, a plain-text one for search, and an editor block, so it behaves the same in all four places.

| Component | Kind | Reference |
|---|---|---|
| `accordion` | container | [/reference/gallery/disclosure](/reference/gallery/disclosure) |
| `accordions` | container | [/reference/gallery/disclosure](/reference/gallery/disclosure) |
| `assistant` | leaf | [/reference/gallery/page](/reference/gallery/page) |
| `badge` | inline | [/reference/gallery/inline](/reference/gallery/inline) |
| `banner` | container | [/reference/gallery/page](/reference/gallery/page) |
| `callout` | container | [/reference/gallery/callouts](/reference/gallery/callouts) |
| `card` | container | [/reference/gallery/layout](/reference/gallery/layout) |
| `cards` | container | [/reference/gallery/layout](/reference/gallery/layout) |
| `check` | container | [/reference/gallery/callouts](/reference/gallery/callouts) |
| `code` | inline | [/reference/gallery/code](/reference/gallery/code) |
| `code-group` | container | [/reference/gallery/code](/reference/gallery/code) |
| `color` | inline | [/reference/gallery/inline](/reference/gallery/inline) |
| `column` | container | [/reference/gallery/layout](/reference/gallery/layout) |
| `columns` | container | [/reference/gallery/layout](/reference/gallery/layout) |
| `danger` | container | [/reference/gallery/callouts](/reference/gallery/callouts) |
| `divider` | leaf | [/reference/gallery/layout](/reference/gallery/layout) |
| `embed` | leaf | [/reference/gallery/media](/reference/gallery/media) |
| `endpoint` | container | [/reference/gallery/api](/reference/gallery/api) |
| `expandable` | container | [/reference/gallery/disclosure](/reference/gallery/disclosure) |
| `expandables` | container | [/reference/gallery/disclosure](/reference/gallery/disclosure) |
| `fact` | inline | [/reference/gallery/inline](/reference/gallery/inline) |
| `feedback` | leaf | [/reference/gallery/page](/reference/gallery/page) |
| `file` | leaf | [/reference/gallery/media](/reference/gallery/media) |
| `files` | container | [/reference/gallery/media](/reference/gallery/media) |
| `frame` | container | [/reference/gallery/layout](/reference/gallery/layout) |
| `github` | leaf | [/reference/gallery/page](/reference/gallery/page) |
| `hero` | container | [/reference/gallery/layout](/reference/gallery/layout) |
| `icon` | inline | [/reference/gallery/inline](/reference/gallery/inline) |
| `iframe` | leaf | [/reference/gallery/media](/reference/gallery/media) |
| `image` | leaf | [/reference/gallery/media](/reference/gallery/media) |
| `info` | container | [/reference/gallery/callouts](/reference/gallery/callouts) |
| `kbd` | inline | [/reference/gallery/inline](/reference/gallery/inline) |
| `md` | container | [/reference/gallery/page](/reference/gallery/page) |
| `note` | container | [/reference/gallery/callouts](/reference/gallery/callouts) |
| `openapi-schema` | leaf | [/reference/gallery/api](/reference/gallery/api) |
| `panel` | container | [/reference/gallery/layout](/reference/gallery/layout) |
| `param` | container | [/reference/gallery/api](/reference/gallery/api) |
| `prompt` | container | [/reference/gallery/page](/reference/gallery/page) |
| `region` | container | [/reference/gallery/page](/reference/gallery/page) |
| `request-example` | container | [/reference/gallery/api](/reference/gallery/api) |
| `response-example` | container | [/reference/gallery/api](/reference/gallery/api) |
| `response-field` | container | [/reference/gallery/api](/reference/gallery/api) |
| `screenshot` | leaf | [/reference/gallery/media](/reference/gallery/media) |
| `snippet-from` | leaf | [/reference/gallery/code](/reference/gallery/code) |
| `step` | container | [/reference/gallery/disclosure](/reference/gallery/disclosure) |
| `steps` | container | [/reference/gallery/disclosure](/reference/gallery/disclosure) |
| `tab` | container | [/reference/gallery/disclosure](/reference/gallery/disclosure) |
| `tabs` | container | [/reference/gallery/disclosure](/reference/gallery/disclosure) |
| `terminal` | container | [/reference/gallery/code](/reference/gallery/code) |
| `tile` | container | [/reference/gallery/layout](/reference/gallery/layout) |
| `tiles` | container | [/reference/gallery/layout](/reference/gallery/layout) |
| `tip` | container | [/reference/gallery/callouts](/reference/gallery/callouts) |
| `toc` | leaf | [/reference/gallery/page](/reference/gallery/page) |
| `tooltip` | inline | [/reference/gallery/inline](/reference/gallery/inline) |
| `tree` | container | [/reference/gallery/page](/reference/gallery/page) |
| `update` | container | [/reference/gallery/page](/reference/gallery/page) |
| `video` | leaf | [/reference/gallery/media](/reference/gallery/media) |
| `visibility` | container | [/reference/gallery/page](/reference/gallery/page) |
| `warning` | container | [/reference/gallery/callouts](/reference/gallery/callouts) |
