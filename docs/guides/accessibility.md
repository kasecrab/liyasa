---
title: Accessibility
description: What the theme guarantees, what only an author can get right, and how to check both.
---

# Accessibility

Accessible documentation is documentation more people can read, including
people using a keyboard because their trackpad broke and people reading in
sunlight. The standard to hold to is WCAG 2.2 AA.

Liyasa splits the work: the theme is responsible for the markup and the
interaction, and you are responsible for the content. Neither can cover for the
other.

## What the theme handles

- **Semantic structure.** Landmarks, one `h1` per page, and a heading outline
  that follows the document rather than the visual design.
- **Keyboard access.** Every interactive element is reachable and operable by
  keyboard, with a visible focus ring that meets the contrast requirement.
  Disclosure components use native elements where possible.
- **Colour contrast.** Every colour pair in every shipped preset meets AA in
  both light and dark schemes. A custom colour that fails is
  [`E0107`](/errors/E0107) at build time.
- **Motion.** Animation respects `prefers-reduced-motion`, and no transition
  exceeds 200 ms.
- **No-JavaScript operation.** The site renders and navigates with scripting
  off. Search degrades to a form rather than disappearing.
- **Zoom and reflow.** Content reflows to 320 CSS pixels without horizontal
  scrolling, and text scales to 200% without loss.

## What you have to get right

::::accordions

:::accordion{title="Alt text"}
Describe what the image *tells the reader*, not what it depicts. A screenshot's
alt text should carry the information a sighted reader takes from it.

Decorative images take `alt=""` so assistive technology skips them. An image
with no alt attribute at all is [`E0305`](/errors/E0305).
:::

:::accordion{title="Heading order"}
Do not skip levels to get a font size. A skipped level is
[`W0306`](/errors/W0306) and it breaks the outline screen-reader users navigate
by. If you want smaller text, that is a styling decision, not a heading level.
:::

:::accordion{title="Link text"}
"Click here" repeated eleven times is eleven identical entries in a screen
reader's link list. Non-descriptive link text is [`W0405`](/errors/W0405). See
[linking](/guides/linking).
:::

:::accordion{title="Tables"}
Use a real header row. Do not use a table for layout. Do not merge cells in a
data table: the reading order becomes ambiguous and assistive technology
announces it wrongly.
:::

:::accordion{title="Meaning carried by colour or position"}
"The green rows are supported" fails for readers who cannot distinguish the
colour and for the Markdown output entirely. Add a word, an icon with a label,
or a column.
:::

:::accordion{title="Language"}
Set `locales` so the rendered pages carry the correct `lang` attribute.
Screen readers choose pronunciation from it, and German prose announced with
English phonetics is unintelligible rather than merely wrong.
:::

:::accordion{title="Embedded frames"}
Every `iframe` needs a `title`. Without one, a screen reader announces "iframe",
which tells the reader nothing about whether to enter it.
:::

::::

## Checking

```sh
liyasa test --a11y
```

Without the companion runtime this runs the static checks: alt text, heading
order, link text, table structure, and contrast computed from the theme tokens.
With the companion installed it additionally runs axe-core in a real browser,
which catches the things only a rendered page reveals: focus order, ARIA
misuse, and contrast against actual backgrounds.

:::warning{title="Automated checks find perhaps half of it"}
No tool can tell you that your alt text is unhelpful, that your tab order is
illogical, or that your procedure is impossible to follow without a mouse. Run
the checks, then try the site with the keyboard only, and once with a screen
reader.
:::

## Writing for cognitive accessibility

The rules in [style](/guides/style) are accessibility rules: short sentences,
one idea each, condition before instruction, procedures as numbered steps,
consistent terminology. They matter most for readers with cognitive
disabilities, and they help everyone else too.

Avoid idiom and metaphor in instructions. "Blow away the cache" is a guess for a
non-native reader and a wrong guess for a translation model.

## Next steps

[Media](/guides/media) covers alt text and captions in detail, and
[SEO](/guides/seo) covers the structural metadata that overlaps heavily with
what assistive technology reads.
