---
title: Components
description: "Every built-in component, with its props and a live example of each."
---

# Components

Components are written as directives, so a page that uses them is still readable Markdown and still reviewable in a diff.

```markdown
:::note{title="A container"}
Three colons open and close it.
:::

::image{src="/assets/example.svg" alt="A leaf component takes no children"}

An :kbd[inline] component sits inside a sentence.
```

Nest a container inside another by giving the outer one more colons. Props are written in braces: strings are quoted, numbers and booleans are not, and a bare name is a flag that means `true`.

Every component also has a Markdown serialization for agents, a plain-text one for search, and an editor block, so it behaves the same in all four places.

| Component | Kind | Reference |
|---|---|---|
| `accordion` | container | [/reference/components/disclosure](/reference/components/disclosure) |
| `accordions` | container | [/reference/components/disclosure](/reference/components/disclosure) |
| `assistant` | leaf | [/reference/components/page](/reference/components/page) |
| `badge` | inline | [/reference/components/inline](/reference/components/inline) |
| `banner` | container | [/reference/components/page](/reference/components/page) |
| `callout` | container | [/reference/components/callouts](/reference/components/callouts) |
| `card` | container | [/reference/components/layout](/reference/components/layout) |
| `cards` | container | [/reference/components/layout](/reference/components/layout) |
| `check` | container | [/reference/components/callouts](/reference/components/callouts) |
| `code` | inline | [/reference/components/code](/reference/components/code) |
| `code-group` | container | [/reference/components/code](/reference/components/code) |
| `color` | inline | [/reference/components/inline](/reference/components/inline) |
| `column` | container | [/reference/components/layout](/reference/components/layout) |
| `columns` | container | [/reference/components/layout](/reference/components/layout) |
| `danger` | container | [/reference/components/callouts](/reference/components/callouts) |
| `divider` | leaf | [/reference/components/layout](/reference/components/layout) |
| `embed` | leaf | [/reference/components/media](/reference/components/media) |
| `endpoint` | container | [/reference/components/api](/reference/components/api) |
| `expandable` | container | [/reference/components/disclosure](/reference/components/disclosure) |
| `expandables` | container | [/reference/components/disclosure](/reference/components/disclosure) |
| `fact` | inline | [/reference/components/inline](/reference/components/inline) |
| `feedback` | leaf | [/reference/components/page](/reference/components/page) |
| `file` | leaf | [/reference/components/media](/reference/components/media) |
| `files` | container | [/reference/components/media](/reference/components/media) |
| `frame` | container | [/reference/components/layout](/reference/components/layout) |
| `github` | leaf | [/reference/components/page](/reference/components/page) |
| `hero` | container | [/reference/components/layout](/reference/components/layout) |
| `icon` | inline | [/reference/components/inline](/reference/components/inline) |
| `iframe` | leaf | [/reference/components/media](/reference/components/media) |
| `image` | leaf | [/reference/components/media](/reference/components/media) |
| `info` | container | [/reference/components/callouts](/reference/components/callouts) |
| `kbd` | inline | [/reference/components/inline](/reference/components/inline) |
| `md` | container | [/reference/components/page](/reference/components/page) |
| `note` | container | [/reference/components/callouts](/reference/components/callouts) |
| `openapi-schema` | leaf | [/reference/components/api](/reference/components/api) |
| `panel` | container | [/reference/components/layout](/reference/components/layout) |
| `param` | container | [/reference/components/api](/reference/components/api) |
| `prompt` | container | [/reference/components/page](/reference/components/page) |
| `region` | container | [/reference/components/page](/reference/components/page) |
| `request-example` | container | [/reference/components/api](/reference/components/api) |
| `response-example` | container | [/reference/components/api](/reference/components/api) |
| `response-field` | container | [/reference/components/api](/reference/components/api) |
| `screenshot` | leaf | [/reference/components/media](/reference/components/media) |
| `snippet-from` | leaf | [/reference/components/code](/reference/components/code) |
| `step` | container | [/reference/components/disclosure](/reference/components/disclosure) |
| `steps` | container | [/reference/components/disclosure](/reference/components/disclosure) |
| `tab` | container | [/reference/components/disclosure](/reference/components/disclosure) |
| `tabs` | container | [/reference/components/disclosure](/reference/components/disclosure) |
| `terminal` | container | [/reference/components/code](/reference/components/code) |
| `tile` | container | [/reference/components/layout](/reference/components/layout) |
| `tiles` | container | [/reference/components/layout](/reference/components/layout) |
| `tip` | container | [/reference/components/callouts](/reference/components/callouts) |
| `toc` | leaf | [/reference/components/page](/reference/components/page) |
| `tooltip` | inline | [/reference/components/inline](/reference/components/inline) |
| `tree` | container | [/reference/components/page](/reference/components/page) |
| `update` | container | [/reference/components/page](/reference/components/page) |
| `video` | leaf | [/reference/components/media](/reference/components/media) |
| `visibility` | container | [/reference/components/page](/reference/components/page) |
| `warning` | container | [/reference/components/callouts](/reference/components/callouts) |
