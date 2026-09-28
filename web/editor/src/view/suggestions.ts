// ED-41's tracked changes: the pane that shows what the agent proposes.
//
// > Every AI change is a suggestion shown as a tracked change; the author
// > accepts or rejects per block; nothing is written without acceptance.
//
// The guarantee is `agent.ts`'s — `acceptedEdits` is the only function that
// turns a suggestion into a `SegmentEdit`, and it reads `status`. This file owes
// two things on top of it: that a reader can see **both** texts rather than a
// verdict, and that accept and reject are reachable per suggestion rather than
// only in bulk.

import { html, raw } from "../escape.ts";
import type { Fragment } from "../escape.ts";
import type { BlockSuggestion, SuggestionSet } from "../agent.ts";
import { pendingCount } from "../agent.ts";

/**
 * The suggestion pane.
 *
 * The count is of what is *left to decide*, not of what arrived: "3
 * suggestions" beside two already-accepted ones tells an author nothing about
 * what they still have to do.
 */
export function renderSuggestions(set: SuggestionSet): Fragment {
  const pending = pendingCount(set);
  return html`<section class="suggestions" data-suggestions="${set.id}" aria-label="Suggested changes">
    <h2>Suggested changes</h2>
    <p class="suggestion-count" role="status" aria-live="polite">
      ${pending === 0
        ? `Nothing left to decide. ${decided(set)} of ${set.suggestions.length} decided.`
        : `${pending} of ${set.suggestions.length} still to decide.`}
    </p>
    ${set.withheld.length === 0 ? null : renderWithheld(set)}
    ${set.suggestions.length === 0
      ? html`<p class="empty">This run proposed nothing.</p>`
      : html`<ol class="suggestion-list">
          ${set.suggestions.map((suggestion) => renderSuggestion(suggestion))}
        </ol>`}
    ${pending === 0
      ? null
      : html`<div class="form-actions">
          <button type="button" class="primary" data-accept-remaining>Accept the rest</button>
          <button type="button" data-reject-all>Reject all</button>
        </div>`}
  </section>`;
}

function decided(set: SuggestionSet): number {
  return set.suggestions.filter((suggestion) => suggestion.status !== "pending").length;
}

/**
 * ED-42: what the validator refused.
 *
 * Counted rather than hidden. A run that proposed five changes and offered
 * three has done something the author should know about, and silence would read
 * as "the model only found three".
 */
function renderWithheld(set: SuggestionSet): Fragment {
  const count = set.withheld.length;
  return html`<p class="withheld" data-withheld>
    ${count} suggestion${count === 1 ? " was" : "s were"} not offered: ${count === 1 ? "it" : "they"}
    would not have built.
    <span class="detail">${set.withheld
      .flatMap((suggestion) => suggestion.diagnostics.map((diagnostic) => diagnostic.code))
      .join(", ")}</span>
  </p>`;
}

/**
 * One tracked change.
 *
 * Both texts, marked up as `<del>` and `<ins>` so a screen reader announces the
 * removal and the insertion rather than reading two paragraphs that differ in a
 * way nobody said. A diff summary in place of the texts would make the author
 * trust the editor's reading of the change instead of their own.
 */
export function renderSuggestion(suggestion: BlockSuggestion): Fragment {
  const id = `suggestion-${suggestion.id}`;
  // Attributes on one line, deliberately: a template that wraps them puts a
  // newline between two attributes, and every assertion about the pair then has
  // to know where the renderer happened to break the line.
  return html`<li class="suggestion suggestion-${suggestion.status}" data-suggestion="${suggestion.id}" data-status="${suggestion.status}" aria-labelledby="${id}-why">
    <p class="why" id="${id}-why">${suggestion.rationale}</p>
    <del class="suggestion-before">${suggestion.before.replace(/\n+$/, "")}</del>
    <ins class="suggestion-after">${suggestion.after.replace(/\n+$/, "")}</ins>
    ${suggestion.diagnostics.length === 0 ? null : renderWarnings(suggestion)}
    <div class="suggestion-actions">
      <button type="button" data-accept="${suggestion.id}" aria-keyshortcuts="a"${suggestion.status === "accepted" ? raw(' aria-pressed="true"') : null}>Accept</button>
      <button type="button" data-reject="${suggestion.id}" aria-keyshortcuts="r"${suggestion.status === "rejected" ? raw(' aria-pressed="true"') : null}>Reject</button>
      <span class="suggestion-status">${statusWord(suggestion.status)}</span>
    </div>
  </li>`;
}

/**
 * A suggestion that validates with warnings is still offered (ED-42 withholds
 * only on errors), and the warnings are shown so the decision is informed.
 */
function renderWarnings(suggestion: BlockSuggestion): Fragment {
  return html`<ul class="suggestion-warnings">
    ${suggestion.diagnostics.map(
      (diagnostic) => html`<li><span class="code">${diagnostic.code}</span> ${diagnostic.message}</li>`,
    )}
  </ul>`;
}

function statusWord(status: BlockSuggestion["status"]): string {
  switch (status) {
    case "accepted":
      return "Accepted";
    case "rejected":
      return "Rejected";
    default:
      return "Not decided";
  }
}

/** What a screen reader hears when one suggestion is decided. */
export function suggestionAnnouncement(set: SuggestionSet, id: string): string {
  const suggestion = set.suggestions.find((candidate) => candidate.id === id);
  if (!suggestion) return "";
  const left = pendingCount(set);
  const word = suggestion.status === "accepted" ? "Accepted" : "Rejected";
  // Nothing is written yet, and saying so is the point: ED-41's guarantee is
  // invisible unless the editor states it.
  return left === 0
    ? `${word}. Nothing is written until you apply these changes.`
    : `${word}. ${left} still to decide.`;
}
