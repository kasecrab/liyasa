// ED-70 and ED-71: getting into the editor from a published page, and the
// guided tasks for people who will use this once a quarter.
//
// Every guided task ends the same way: a **proposal**, reviewed like any
// other. That is the requirement's last clause and it is the point of the
// feature — a task somebody runs once a quarter is exactly the one that should
// not write straight to the published site.

import { draftBranch } from "./drafts.ts";
import { changeFactReference, findReplace } from "./bulk.ts";
import type { BulkPlan, ScannedPage } from "./bulk.ts";
import { escapeRegExp } from "./text.ts";

/**
 * ED-70: where "Suggest an edit" on a published page goes.
 *
 * The route comes back as a query rather than a path segment so a route with
 * slashes needs no escaping scheme of its own, and `new` says the editor opens
 * a fresh draft rather than joining whichever one happens to be open.
 */
export function suggestEditUrl(route: string, options: { user: string; block?: string }): string {
  const search = new URLSearchParams({ page: route, draft: "new" });
  if (options.block) search.set("block", options.block);
  search.set("branch", draftBranch(options.user, slugOf(route)));
  return `/_liyasa/editor/?${search}`;
}

function slugOf(route: string): string {
  const trimmed = route.replace(/^\/+|\/+$/g, "");
  return trimmed === "" ? "home" : trimmed.replace(/\//g, "-");
}

export interface QuickFix {
  route: string;
  block: string;
  before: string;
  after: string;
}

/**
 * ED-70's quick fix: one block, edited without leaving the page.
 *
 * It is still a draft and still a proposal — the only thing "quick" removes is
 * the trip to the editor, not the review.
 */
export function quickFixProposal(fix: QuickFix, user: string): {
  branch: string;
  page: string;
  block: string;
  after: string;
  summary: string;
} | null {
  if (fix.after === fix.before) return null;
  return {
    branch: draftBranch(user, `quick-${slugOf(fix.route)}`),
    page: fix.route,
    block: fix.block,
    after: fix.after,
    summary: `Quick fix on ${fix.route}`,
  };
}

export type TaskId =
  | "update-a-number"
  | "rename-everywhere"
  | "replace-a-screenshot"
  | "add-a-faq-entry"
  | "record-a-changelog-entry";

export interface TaskSpec {
  id: TaskId;
  label: string;
  /** What the first screen asks for. */
  asks: string[];
  /** What the task shows before it proposes anything. */
  shows: string;
}

/** The five tasks ED-71 names. */
export const TASKS: TaskSpec[] = [
  {
    id: "update-a-number",
    label: "Update a number",
    asks: ["fact", "value"],
    shows: "the pages that show this number",
  },
  {
    id: "rename-everywhere",
    label: "Rename a feature everywhere",
    asks: ["from", "to"],
    shows: "every page and fact the old name appears on",
  },
  {
    id: "replace-a-screenshot",
    label: "Replace a screenshot",
    asks: ["asset", "file"],
    shows: "the pages that use this image",
  },
  { id: "add-a-faq-entry", label: "Add a FAQ entry", asks: ["question", "answer"], shows: "where it will go" },
  {
    id: "record-a-changelog-entry",
    label: "Record a changelog entry",
    asks: ["date", "summary"],
    shows: "the changelog it joins",
  },
];

export function findTask(id: string): TaskSpec | undefined {
  return TASKS.find((task) => task.id === id);
}

export interface Proposal {
  task: TaskId;
  summary: string;
  /** Every page the proposal touches, so the review shows the blast radius. */
  pages: string[];
  plan: BulkPlan | null;
  /** Files written outside the pages, such as a fact source. */
  writes: { path: string; text: string }[];
}

/**
 * ED-71's "update a number".
 *
 * The number lives in the fact source, not on the pages — that is the whole
 * reason facts exist. So the proposal writes the fact and *lists* the pages
 * that show it, rather than editing four pages and leaving the fact stale.
 */
export function updateANumber(
  pages: ScannedPage[],
  options: { fact: string; value: string; factFile: string; factSource: string },
): Proposal {
  const affected = pages
    .filter((page) => page.source.includes(`fact("${options.fact}")`) || page.source.includes(`fact('${options.fact}')`))
    .map((page) => page.path);
  const rewritten = setFactValue(options.factSource, options.fact, options.value);
  return {
    task: "update-a-number",
    summary: `Set ${options.fact} to ${options.value}`,
    pages: affected,
    plan: null,
    writes: rewritten === options.factSource ? [] : [{ path: options.factFile, text: rewritten }],
  };
}

/** The last segment of a dotted fact id, set in a JSON fact file. */
function setFactValue(source: string, fact: string, value: string): string {
  const key = fact.split(".").pop() ?? fact;
  const pattern = new RegExp(`("${escapeRegExp(key)}"\\s*:\\s*)(("(?:[^"\\\\]|\\\\.)*")|[^,\\n}]+)`);
  if (!pattern.test(source)) return source;
  const written = /^-?\d+(\.\d+)?$/.test(value) ? value : JSON.stringify(value);
  return source.replace(pattern, `$1${written}`);
}

/** ED-71's "rename a feature everywhere": pages and facts, with a preview. */
export function renameEverywhere(
  pages: ScannedPage[],
  options: { from: string; to: string; factIds?: { from: string; to: string } },
): Proposal {
  const plan = findReplace(pages, { find: options.from, replaceWith: options.to });
  const factPlan = options.factIds ? changeFactReference(pages, options.factIds) : null;
  const touched = new Set(plan.pages.map((page) => page.path));
  for (const page of factPlan?.pages ?? []) touched.add(page.path);
  return {
    task: "rename-everywhere",
    summary: `Rename “${options.from}” to “${options.to}” on ${touched.size} page${touched.size === 1 ? "" : "s"}`,
    pages: [...touched].sort(),
    plan,
    writes: [],
  };
}

/** ED-71's "replace a screenshot": matched by where the old one is used. */
export function replaceAScreenshot(
  pages: ScannedPage[],
  options: { asset: string; replacement: string },
): Proposal {
  const plan = findReplace(pages, { find: options.asset, replaceWith: options.replacement, scopes: ["text", "props"] });
  return {
    task: "replace-a-screenshot",
    summary: `Replace ${options.asset} with ${options.replacement}`,
    pages: plan.pages.map((page) => page.path),
    plan,
    writes: [],
  };
}

/** ED-71's "add a FAQ entry": appended, so the existing entries do not move. */
export function addAFaqEntry(
  faq: { path: string; source: string },
  options: { question: string; answer: string },
): Proposal {
  const entry = `\n## ${options.question.trim()}\n\n${options.answer.trim()}\n`;
  return {
    task: "add-a-faq-entry",
    summary: `Add “${options.question.trim()}” to the FAQ`,
    pages: [faq.path],
    plan: null,
    writes: [{ path: faq.path, text: `${faq.source.replace(/\n*$/, "\n")}${entry}` }],
  };
}

/**
 * ED-71's "record a changelog entry".
 *
 * Prepended under the heading, because a changelog reads newest first and an
 * entry appended to the end is one nobody sees.
 */
export function recordAChangelogEntry(
  changelog: { path: string; source: string },
  options: { date: string; summary: string },
): Proposal {
  if (!/^\d{4}-\d{2}-\d{2}$/.test(options.date)) {
    throw new Error(`\`${options.date}\` is not a date; a changelog entry is dated YYYY-MM-DD`);
  }
  const entry = `## ${options.date}\n\n${options.summary.trim()}\n\n`;
  const lines = changelog.source.split(/(?<=\n)/);
  // After the front matter and the page's own title, before the first entry.
  let at = 0;
  let seenTitle = false;
  for (; at < lines.length; at += 1) {
    const line = lines[at] as string;
    if (/^## /.test(line)) break;
    if (/^# /.test(line)) seenTitle = true;
  }
  if (!seenTitle && at === lines.length) at = lines.length;
  return {
    task: "record-a-changelog-entry",
    summary: `Record the ${options.date} changelog entry`,
    pages: [changelog.path],
    plan: null,
    writes: [
      {
        path: changelog.path,
        text: lines.slice(0, at).join("").replace(/\n*$/, "\n\n") + entry + lines.slice(at).join(""),
      },
    ],
  };
}
