---
title: content
description: "Markdown, templating, and image behaviour (§8.9)."
sidebarTitle: content
---

# `content`

Markdown, templating, and image behaviour (§8.9).

Specified by CFG-80.

| Key | Type | Default | What it does |
|---|---|---|---|
| `content.codeblocks.lineNumbers` | boolean | — | Number the lines of a code block. |
| `content.codeblocks.theme` | string | — | Override `theme.codeTheme` for code blocks in content. |
| `content.codeblocks.wrap` | boolean | — | Wrap long lines instead of scrolling them sideways. |
| `content.customElements` | `strip` \| `keep` | — | What happens to an unknown HTML element: `strip` removes it and keeps its children, `keep` passes it through. |
| `content.frontmatter.strict` | boolean | `false` | Unknown front matter keys become warnings. |
| `content.html` | `allow` \| `sanitize` \| `off` | — | What happens to raw HTML in a page: `allow` keeps it, `sanitize` keeps what the allow list permits, `off` drops it and reports it. |
| `content.images.breakpoints` | integer[] | — | Widths, in pixels, that each image is resized to for the `srcset`. |
| `content.images.formats` | (`avif` \| `webp` \| `png` \| `jpeg`)[] | — | Formats each image is encoded in, most preferred first; a browser takes the first it understands. |
| `content.lastModified` | boolean | — | CFG-74: show the git or editor timestamp on every page. |
| `content.math` | boolean \| `katex` \| `pulldown-latex` | — | Whether `$...$` and `$$...$$` are rendered as mathematics, and by which engine. |
| `content.related.auto` | boolean | — | Choose related pages automatically when a page names none of its own. |
| `content.reviewCadence` | string | — | A duration such as `500ms`, `30s`, `180d`. |
| `content.templating.limits.depth` | integer | — | How deep includes and blocks may nest. |
| `content.templating.limits.iterations` | integer | — | How many loop iterations one page may run. |
| `content.templating.limits.outputBytes` | string | — | A byte size such as `512MB`. |
| `content.templating.undefined` | `strict` \| `lenient` \| `chainable` | — | What a template does with a name nothing defines: `strict` fails the build, `lenient` renders a visible marker in its place. |
| `content.wikilinks` | boolean | `false` | Whether `[[Page]]` links resolve against the content tree. Off by default, because the brackets mean nothing in CommonMark. |

Every key above is generated from `schemas/liyasa.schema.json`, which is the single source of truth for configuration: a key that is not in the schema is [`E0103`](/errors/E0103), and a value that does not match it is [`E0102`](/errors/E0102).

See [the configuration reference](/reference/config) for the other sections.
