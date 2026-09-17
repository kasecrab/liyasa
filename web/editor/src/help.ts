// ED-72 and ED-74: the words the editor uses, the tour, contextual help, and
// the page templates.
//
// ED-72's rule is one table and one disclosure. The editor says draft,
// suggest, review, publish and undo; branch, commit, pull request, rebase and
// revert exist under "advanced" for the people who want them. Both halves
// matter — hiding git from somebody who knows git is as unhelpful as showing
// it to somebody who does not — so the mapping is explicit and reversible
// rather than a set of strings scattered through the views.

export interface Term {
  plain: string;
  git: string;
  /** One sentence for the "advanced" disclosure. */
  explains: string;
}

/** ED-72's vocabulary, and the git term behind each word. */
export const VOCABULARY: Record<string, Term> = {
  draft: {
    plain: "draft",
    git: "branch",
    explains: "A draft is a branch named `liyasa/<you>/<page>`; your changes live there until they are published.",
  },
  suggest: {
    plain: "suggest",
    git: "commit and push",
    explains: "Suggesting saves your changes to your draft's branch and pushes it.",
  },
  review: {
    plain: "review",
    git: "pull request",
    explains: "A review is a pull request against the branch this site is published from.",
  },
  publish: {
    plain: "publish",
    git: "merge and deploy",
    explains: "Publishing merges your draft and starts a deployment, if the project's policy allows it.",
  },
  undo: {
    plain: "undo",
    git: "revert",
    explains: "Undo adds a commit that puts the previous text back; nothing is erased from the history.",
  },
  version: {
    plain: "version",
    git: "commit",
    explains: "Every save is a commit, so any point in a draft's history can be restored.",
  },
};

/** The word to show, given whether the reader asked for the git terms. */
export function say(term: keyof typeof VOCABULARY | string, advanced: boolean): string {
  const entry = VOCABULARY[term];
  if (!entry) return term;
  return advanced ? `${entry.plain} (${entry.git})` : entry.plain;
}

export interface TourStep {
  id: string;
  target: string;
  title: string;
  body: string;
}

/** ED-74's onboarding tour: six steps, each pointing at something on screen. */
export const TOUR: TourStep[] = [
  {
    id: "editing",
    target: "[data-editor-surface]",
    title: "This is your page",
    body: "Type as you would anywhere else. Markdown shortcuts work: `## ` starts a heading, `- ` starts a list.",
  },
  {
    id: "slash",
    target: "[data-editor-surface]",
    title: "Add a callout, a table, an image",
    body: "Press `/` anywhere to insert a component without leaving the keyboard.",
  },
  {
    id: "modes",
    target: "[data-mode-switch]",
    title: "Two ways to look at the same page",
    body: "Visual mode and source mode edit the same file. Switching between them changes nothing.",
  },
  {
    id: "preview",
    target: "[data-preview]",
    title: "What readers will see",
    body: "The preview is built by the same code that builds the site, so it is not an approximation.",
  },
  {
    id: "drafts",
    target: "[data-drafts]",
    title: "Nothing is live yet",
    body: "Every change goes into a draft. Publishing is a separate step, and it may need a review first.",
  },
  {
    id: "help",
    target: "[data-help]",
    title: "Help, whenever",
    body: "Press `?` for the keyboard shortcuts, or open this menu for the rest.",
  },
];

export interface PageTemplate {
  id: string;
  label: string;
  /** Which of the four documentation kinds this is (the Diátaxis split). */
  kind: "tutorial" | "how-to" | "reference" | "explanation" | "other";
  description: string;
  /** The page, with guidance in it rather than lorem ipsum. */
  body: string;
}

/**
 * ED-74's page templates: one per documentation type, filled with guidance.
 *
 * The guidance is *in the page* as prose the author replaces, not as a comment
 * they delete. Somebody who has never written documentation needs to see what
 * a good section looks like, and an empty page with a heading teaches nothing.
 */
export const TEMPLATES: PageTemplate[] = [
  {
    id: "tutorial",
    label: "Tutorial",
    kind: "tutorial",
    description: "A lesson that takes a beginner from nothing to a first success.",
    body: [
      "## What you will build",
      "",
      "One sentence naming the thing that will exist at the end. A reader decides",
      "here whether this is the page they want.",
      "",
      "## Before you start",
      "",
      "- What must already be installed",
      "- What access is needed",
      "",
      "::::steps",
      "",
      ':::step{title="The first step"}',
      "One action, one result. Show the command and what it prints.",
      ":::",
      "",
      ':::step{title="The second step"}',
      "Keep each step small enough to check.",
      ":::",
      "",
      "::::",
      "",
      "## What to do next",
      "",
      "Two or three links, chosen rather than listed.",
      "",
    ].join("\n"),
  },
  {
    id: "how-to",
    label: "How-to guide",
    kind: "how-to",
    description: "Steps for somebody who already knows what they want.",
    body: [
      "## Goal",
      "",
      "One sentence, in the reader's words, naming the task.",
      "",
      "## Steps",
      "",
      "1. The first thing to do",
      "2. The second thing to do",
      "",
      ':::note{title="If this fails"}',
      "The common failure, and what it means.",
      ":::",
      "",
    ].join("\n"),
  },
  {
    id: "reference",
    label: "Reference",
    kind: "reference",
    description: "What something is, exhaustively and without narrative.",
    body: [
      "## Summary",
      "",
      "What this is, in one sentence.",
      "",
      "## Fields",
      "",
      "| Name | Type | Default | What it does |",
      "| --- | --- | --- | --- |",
      "|  |  |  |  |",
      "",
      "## Examples",
      "",
      "```json",
      "{}",
      "```",
      "",
    ].join("\n"),
  },
  {
    id: "explanation",
    label: "Explanation",
    kind: "explanation",
    description: "Why something works the way it does.",
    body: [
      "## The short answer",
      "",
      "Two sentences for a reader who will not read the rest.",
      "",
      "## Why it works this way",
      "",
      "The constraint, the alternatives, and why this one.",
      "",
      "## What this means in practice",
      "",
      "The consequence the reader will actually meet.",
      "",
    ].join("\n"),
  },
  {
    id: "changelog",
    label: "Changelog entry",
    kind: "other",
    description: "One dated entry for the changelog.",
    body: [
      "## YYYY-MM-DD",
      "",
      "What changed, from the reader's side rather than the commit's. \"Rate limits",
      "are now per key rather than per account\" is an entry; \"refactored the",
      "limiter\" is not.",
      "",
      "If a reader has to do something, say so here and link to the page that",
      "explains it.",
      "",
    ].join("\n"),
  },
  {
    id: "api-endpoint",
    label: "API endpoint",
    kind: "reference",
    description: "One operation from an API description.",
    body: [
      "## Summary",
      "",
      "What this operation does, in one sentence.",
      "",
      "## Request",
      "",
      "Describe each parameter that is not obvious from its name.",
      "",
      "## Response",
      "",
      "What comes back, and what an error looks like.",
      "",
    ].join("\n"),
  },
];

export function findTemplate(id: string): PageTemplate | undefined {
  return TEMPLATES.find((template) => template.id === id);
}

/** A new page from a template, with its front matter filled in. */
export function pageFromTemplate(id: string, options: { title: string; pageId: string }): string {
  const template = findTemplate(id);
  if (!template) throw new Error(`no page template \`${id}\``);
  return `---\nid: ${options.pageId}\ntitle: ${options.title}\n---\n\n${template.body}`;
}

export interface HelpTopic {
  id: string;
  title: string;
  body: string;
}

/** ED-74's contextual help, keyed by what the author is looking at. */
export const HELP: Record<string, HelpTopic> = {
  frontmatter: {
    id: "frontmatter",
    title: "Page settings",
    body: "The fields at the top of a page: its title, its description, and who may see it. The ones most pages use are shown; the rest are under Advanced.",
  },
  templating: {
    id: "templating",
    title: "Values that change",
    body: "`{{ }}` shows a value the project keeps in one place, such as a price or a version. Changing it there changes every page that shows it.",
  },
  components: {
    id: "components",
    title: "Components",
    body: "Callouts, tabs, steps and cards. Press `/` to insert one; select it to change its settings.",
  },
  review: {
    id: "review",
    title: "Review",
    body: "Somebody else reads your suggestion and either approves it or asks for changes. The project decides who, and whether it is required.",
  },
};
