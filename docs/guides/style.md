---
title: Style
description: How to write sentences, headings, and procedures that readers and agents both parse correctly on the first pass.
---

# Style

Documentation style is not taste. Every rule below exists because breaking it
measurably costs a reader time, or costs an agent an incorrect answer.

## Write for the second reader

Nobody reads documentation from the top. They arrive from a search result,
somewhere in the middle, with a question already formed. Write every section as
though it is the first thing on the page: name the thing before referring to
it, and do not lean on context from three headings up.

## Sentences

- **One idea per sentence.** If a sentence has two clauses joined by "and" and
  both carry instructions, it is two sentences.
- **Present tense, active voice.** "The build writes `dist/`", not "`dist/` will
  have been written by the build".
- **Say the subject.** "This is deprecated" leaves the reader guessing what
  *this* is. Agents guess worse than people do.
- **Put the condition first.** "If the build fails, run `liyasa doctor`" beats
  "Run `liyasa doctor` if the build fails", because the reader who is not in
  that situation can stop reading after four words.

## Headings

Headings are the table of contents, the anchors, the search result titles, and
the chunk boundaries an agent retrieves against. They carry more weight than
their size suggests.

- Make them **descriptive, not clever**. "Rate limits" beats "Playing nicely".
- Make them **stand alone**. A heading called "Configuration" on eleven
  different pages is eleven identical search results.
- Never **skip a level**. An `h4` under an `h2` is a warning
  ([`W0306`](/errors/W0306)) because it breaks both screen readers and the
  document outline agents build.
- Do not put **only** a heading between two headings. A section with no prose is
  a sign the structure is wrong.

## Procedures

Anything with an order goes in a numbered list or a `steps` component, never in
a paragraph. A paragraph that contains "first", "then", and "finally" is a
procedure wearing a disguise.

```markdown
::::steps

:::step{title="Install the binary"}
...
:::

:::step{title="Create a project"}
...
:::

::::
```

Each step should have one command or one decision in it. If a step has three
commands, the reader cannot tell where to resume after an error.

## Words

:::warning{title="Words that cost you a support ticket"}
**"Simply", "just", "obviously", "easy".** If the reader is stuck, these tell
them the problem is with them. They add nothing when the reader is not stuck.

**"Should"** is ambiguous between expectation and obligation. Say "must" for
requirements and "expect" for outcomes.

**"Currently", "at the moment", "for now", "soon".** Undated statements about
time become lies. Use a version or a date, or make the claim a
[fact](/guides/fact-modelling) with a source.
:::

Keep one name for one thing. If the setting is `build.basePath`, it is not "the
base path option" in one place and "the path prefix" in another. A synonym is a
failed search.

## Code samples

- **Make them runnable.** A sample with `<YOUR_API_KEY>` and no explanation of
  where that comes from is a sample that does not run.
- **Show the output** when the reader needs to recognise success.
- **Keep them short.** Elide with a comment rather than with an ellipsis, so
  that copying the block still parses.
- **Verify them.** A code sample that Liyasa executes on every build cannot rot.
  [Verification](/guides/verification) covers how.

## Tables

Tables are for values that share a shape: flags, keys, limits, statuses. They
are not for prose. A table cell with three sentences in it should be a section.

Give every column a header that says what the values *are*, not what they are
*about*: "Default", not "Defaults info".

## Front matter

Every page needs a `title` and a `description`. The description is the search
snippet, the social preview, the `llms.txt` line, and often the only thing an
agent sees before deciding whether to fetch the page. A missing one is
[`W0630`](/errors/W0630).

```yaml
---
title: Rate limits
description: What the API allows per minute, per plan, and what happens when you exceed it.
---
```

Write the description as a sentence that could stand alone in a list of
unrelated links. "Learn about rate limits" fails that test.

## Enforcing it

Prose rules are checked rather than remembered. `liyasa verify --only prose`
runs the built-in style rules plus any Vale rule packages you configure, and
reports them as diagnostics with codes ([`W0631`](/errors/W0631) for a rule
match, [`W0632`](/errors/W0632) for a word outside the project dictionary).

```sh
liyasa verify --only prose
```

## Next steps

[Writing for agents](/guides/writing-for-agents) covers the parts of style that
matter specifically to machine readers, and
[accessibility](/guides/accessibility) covers the parts that matter to readers
using assistive technology.
