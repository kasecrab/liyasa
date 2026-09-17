// The editor application.
//
// This is the only module that touches the document, the network, storage or
// the WebAssembly module. Everything else is a function from state to markup
// or to `SegmentEdit`s, which is why `test/` needs no browser: a test against a
// stand-in DOM proves the stand-in works. `web/dashboard/src/dashboard.ts`
// took the same shape, for the same reason.
//
// Nothing here runs under `node --test`. What is testable about the shell is
// the markup its renderers produce, and those are pure and live beside the
// state they render.

import { html, raw } from "./escape.ts";
import type { Fragment } from "./escape.ts";
import { buildModel, editBlock, editBlocks, serializeModel } from "./model.ts";
import type { EditorModel, MarkdownBlock } from "./model.ts";
import { decorate, placeDiagnostics, tokenize } from "./source.ts";
import { SLASH_COMMANDS, matchShortcut, moveBlock, slashInsert, slashMatches } from "./commands.ts";
import { htmlToMarkdown, imagesIn, pasteSource } from "./paste.ts";
import { PREVIEW_DEFAULTS, capBytes, capIterations, chipSource, previewContext, previewLimits } from "./preview.ts";
import { formFields, parseFrontmatter, validateFrontmatter, writeFrontmatter } from "./frontmatter.ts";
import { createPage, deletePage, duplicatePage, movePage, navigationOf, renamePage, reorderNavigation, routeOf } from "./pages.ts";
import { applyPlan, applyTag, changeFactReference, findReplace, moveGroup } from "./bulk.ts";
import { darkPartner, deleteRefusal, imageDirective, searchAssets, sniff, transformQuery, validateUpload } from "./media.ts";
import { ageOf, alsoOpenElsewhere, draftBranch, merge3, pagesTouched, save, searchDrafts, seenTab } from "./drafts.ts";
import { call, endpointPath, findEndpoint, unservedBy } from "./api.ts";
import { may, primaryAction, refusal } from "./roles.ts";
import type { Grant } from "./roles.ts";
import { assignReviewers, decide, ownersFor, parseDocowners, publishPlan, queueRows, staleReminders } from "./review.ts";
import { OPERATIONS, acceptedEdits, pendingCount, rejectAll, runPayload, screen, setStatus } from "./agent.ts";
import { ACTIVITY_KINDS, KIND_LABEL, byDay, counts, feed } from "./activity.ts";
import { TASKS, addAFaqEntry, quickFixProposal, recordAChangelogEntry, renameEverywhere, replaceAScreenshot, suggestEditUrl, updateANumber } from "./tasks.ts";
import { HELP, TEMPLATES, TOUR, VOCABULARY, pageFromTemplate, say } from "./help.ts";
import { LANDMARKS, SHORTCUTS, chartTable, motionDuration, reviewAnnouncement, saveAnnouncement, validationAnnouncement } from "./a11y.ts";
import { messageFor } from "./messages.ts";
import { EditorSession, PreviewHold, sessionNonce } from "./session.ts";

/** What the shell holds while a draft is open. */
interface State {
  mode: "visual" | "source";
  advanced: boolean;
  grant: Grant;
  model: EditorModel | null;
  source: string;
  path: string;
  version: number;
  baseText: string;
  announcement: string;
}

const state: State = {
  mode: "visual",
  advanced: false,
  grant: { role: "contributor" },
  model: null,
  source: "",
  path: "",
  version: 0,
  baseText: "",
  announcement: "",
};

// --- rendering --------------------------------------------------------------

/** One block of the visual mode, as the markup the surface shows. */
export function renderBlock(block: MarkdownBlock): Fragment {
  const body = block.text.replace(/\n+$/, "");
  return html`<div class="block block-${block.kind}" data-block="${block.id}" tabindex="0" role="group"
    aria-label="${block.kind}">${body}</div>`;
}

/** The whole visual surface. */
export function renderSurface(model: EditorModel): Fragment {
  const parts: Fragment[] = [];
  for (const node of model.nodes) {
    if (node.blocks) {
      for (const block of node.blocks) parts.push(renderBlock(block));
      continue;
    }
    parts.push(
      html`<div class="block block-${node.kind}" data-block="${node.id}" tabindex="0" role="group"
        aria-label="${node.name ?? node.kind}">${node.text.replace(/\n+$/, "")}</div>`,
    );
  }
  return html`<div class="surface" data-editor-surface>${parts}</div>`;
}

/** The problems pane: ED-73's plain-language text, with a fix where there is one. */
export function renderProblems(source: string, diagnostics: Parameters<typeof placeDiagnostics>[1]): Fragment {
  const placed = placeDiagnostics(source, diagnostics);
  if (placed.length === 0) return html`<p class="empty">No problems found.</p>`;
  return html`<ul class="problems">
    ${placed.map((entry) => {
      const shown = messageFor(entry.diagnostic.code);
      return html`<li class="problem problem-${entry.diagnostic.severity}">
        <span class="where">Line ${entry.from.line}</span>
        <span class="what">${shown.plain}</span>
        ${shown.fix ? html`<button type="button" data-fix="${shown.fix.action}">${shown.fix.label}</button>` : null}
        <a class="code" href="${entry.diagnostic.url}">${entry.diagnostic.code}</a>
      </li>`;
    })}
  </ul>`;
}

/** The toolbar, whose main button is one this person can press (ED-75). */
export function renderToolbar(grant: Grant, advanced: boolean): Fragment {
  const primary = primaryAction(grant);
  return html`<header class="toolbar" role="banner" aria-label="Editor toolbar">
    <button type="button" data-mode-switch>${advanced ? "Source" : "Source"}</button>
    <button type="button" data-action="${primary.action}" class="primary">${primary.label}</button>
    <button type="button" data-help aria-keyshortcuts="?">Help</button>
    <label class="advanced">
      <input type="checkbox" data-advanced ${advanced ? raw("checked") : null} />
      ${say("draft", advanced)} details
    </label>
  </header>`;
}

/** The keyboard-shortcut sheet, which is also ED-80's evidence. */
export function renderShortcuts(): Fragment {
  return html`<table class="shortcuts">
    <caption>Keyboard shortcuts</caption>
    <thead><tr><th scope="col">Keys</th><th scope="col">Does</th><th scope="col">Where</th></tr></thead>
    <tbody>
      ${SHORTCUTS.map(
        (shortcut) => html`<tr><td><kbd>${shortcut.keys}</kbd></td><td>${shortcut.description}</td><td>${shortcut.scope}</td></tr>`,
      )}
    </tbody>
  </table>`;
}

// --- the shell --------------------------------------------------------------

function mount(): void {
  const root = document.querySelector("[data-editor]");
  if (!root) return;

  const reduced = globalThis.matchMedia?.("(prefers-reduced-motion: reduce)").matches ?? false;
  root.setAttribute("style", `--motion: ${motionDuration(180, reduced)}ms`);

  for (const landmark of LANDMARKS) {
    const region = root.querySelector(`[data-landmark="${landmark.label}"]`);
    region?.setAttribute("role", landmark.role);
    region?.setAttribute("aria-label", landmark.label);
  }

  announce("Editor ready");
}

function announce(message: string): void {
  state.announcement = message;
  const region = document.querySelector("[data-announce]");
  if (region) region.textContent = message;
}

if (typeof document !== "undefined") {
  if (document.readyState === "loading") {
    document.addEventListener("DOMContentLoaded", mount, { once: true });
  } else {
    mount();
  }
}

// Every module this application is made of is reachable from the entry, so the
// bundler walks the whole graph and the budget covers all of it. Listing them
// here rather than calling each one is deliberate: the shell wires them up as
// the panes are built, and a bundle that dropped half the editor because the
// shell had not yet reached it would pass its own budget test.
export const MODULES = {
  buildModel,
  editBlock,
  editBlocks,
  serializeModel,
  tokenize,
  decorate,
  placeDiagnostics,
  SLASH_COMMANDS,
  slashInsert,
  slashMatches,
  matchShortcut,
  moveBlock,
  htmlToMarkdown,
  imagesIn,
  pasteSource,
  PREVIEW_DEFAULTS,
  previewLimits,
  previewContext,
  capIterations,
  capBytes,
  chipSource,
  parseFrontmatter,
  writeFrontmatter,
  formFields,
  validateFrontmatter,
  routeOf,
  navigationOf,
  createPage,
  renamePage,
  movePage,
  duplicatePage,
  deletePage,
  reorderNavigation,
  findReplace,
  applyPlan,
  changeFactReference,
  moveGroup,
  applyTag,
  sniff,
  validateUpload,
  imageDirective,
  darkPartner,
  searchAssets,
  deleteRefusal,
  transformQuery,
  draftBranch,
  searchDrafts,
  pagesTouched,
  ageOf,
  save,
  merge3,
  seenTab,
  alsoOpenElsewhere,
  call,
  endpointPath,
  findEndpoint,
  unservedBy,
  may,
  refusal,
  primaryAction,
  parseDocowners,
  ownersFor,
  assignReviewers,
  staleReminders,
  queueRows,
  decide,
  publishPlan,
  OPERATIONS,
  runPayload,
  screen,
  setStatus,
  rejectAll,
  acceptedEdits,
  pendingCount,
  ACTIVITY_KINDS,
  KIND_LABEL,
  feed,
  byDay,
  counts,
  TASKS,
  suggestEditUrl,
  quickFixProposal,
  updateANumber,
  renameEverywhere,
  replaceAScreenshot,
  addAFaqEntry,
  recordAChangelogEntry,
  VOCABULARY,
  say,
  TOUR,
  TEMPLATES,
  pageFromTemplate,
  HELP,
  chartTable,
  saveAnnouncement,
  validationAnnouncement,
  reviewAnnouncement,
  messageFor,
  EditorSession,
  PreviewHold,
  sessionNonce,
  renderSurface,
  renderBlock,
  renderProblems,
  renderToolbar,
  renderShortcuts,
  announce,
  state,
};
