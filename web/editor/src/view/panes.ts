// The panes that list things: drafts (ED-20), the conflict resolver (ED-21),
// the activity feed (ED-32) and the media library (ED-13).
//
// Each of these has data behind it that nothing serves yet — every editor route
// is `unbuilt` in `src/api.ts`. So every one of them takes its rows as an
// argument and renders an **honest empty state**: not "no drafts", which is a
// claim about the project, but "nothing is serving this yet", which is a claim
// about the build. An author with twelve drafts must never be shown a pane that
// says they have none.

import { html, raw } from "../escape.ts";
import type { Fragment } from "../escape.ts";
import { ageOf } from "../drafts.ts";
import type { Conflict, Draft } from "../drafts.ts";
import { KIND_LABEL, byDay, counts } from "../activity.ts";
import type { Activity, ActivityKind } from "../activity.ts";
import { deleteRefusal, searchAssets } from "../media.ts";
import type { Asset } from "../media.ts";

/** Why a pane has no rows: unserved, or genuinely empty. */
export type Source = "served" | "unbuilt";

/**
 * The empty state.
 *
 * The distinction this function exists for: "you have no drafts" and "this
 * build cannot tell you whether you have drafts" are different sentences, and
 * showing the first when the second is true is the failure every entry in this
 * project's defect ledger has in common.
 */
export function renderEmpty(source: Source, what: string, requirement: string): Fragment {
  return source === "unbuilt"
    ? html`<p class="empty unserved" data-unserved="${requirement}">
        Nothing in this build answers for ${what} yet (${requirement}), so this list is not empty —
        it is unknown.
      </p>`
    : html`<p class="empty">No ${what} yet.</p>`;
}

// --- ED-20: drafts ----------------------------------------------------------

export function renderDrafts(options: {
  drafts: Draft[];
  source: Source;
  now: number;
  query?: string;
  openDraft?: string;
}): Fragment {
  if (options.drafts.length === 0) {
    return html`<section class="drafts" data-drafts aria-label="Drafts">
      <h2>Drafts</h2>
      ${renderEmpty(options.source, "drafts", "ED-20")}
    </section>`;
  }
  return html`<section class="drafts" data-drafts aria-label="Drafts">
    <h2>Drafts</h2>
    <label class="draft-search">
      Search drafts
      <input type="search" data-draft-search value="${options.query ?? ""}" />
    </label>
    <ul class="draft-list">
      ${options.drafts.map((draft) => renderDraft(draft, options.now, draft.id === options.openDraft))}
    </ul>
  </section>`;
}

function renderDraft(draft: Draft, now: number, open: boolean): Fragment {
  const pages = draft.pages.length;
  return html`<li class="draft draft-${draft.status}" data-draft="${draft.id}"${open ? raw(' aria-current="true"') : null}>
    <a href="?draft=${draft.id}" data-open-draft="${draft.id}">${draft.title}</a>
    <span class="draft-status">${panesStatusWord(draft.status)}</span>
    <span class="draft-author">${draft.author}</span>
    <span class="draft-age">${ageOf(draft.updatedAt, now)}</span>
    <span class="draft-pages">${pages} page${pages === 1 ? "" : "s"}</span>
  </li>`;
}

function panesStatusWord(status: Draft["status"]): string {
  switch (status) {
    case "in-review":
      return "In review";
    case "merged":
      return "Published";
    case "closed":
      return "Closed";
    default:
      return "Open";
  }
}

// --- ED-21: the conflict resolver -------------------------------------------

export interface ConflictState {
  path: string;
  conflicts: Conflict[];
  /** Rendered HTML per side, from the WebAssembly preview. */
  rendered: { mine: string; theirs: string };
}

/**
 * ED-21's three-way merge UI.
 *
 * > the conflict is detected and the three-way merge UI shows both versions
 * > **rendered**
 *
 * Rendered, not diffed: the two sides come from the preview, so the author
 * compares two pages rather than reading conflict markers. The marker text
 * never appears in this pane, which is what `merge3` keeps the three texts for.
 *
 * The rendered HTML is inserted with `raw` because it *is* markup — it came from
 * the same sanitising renderer the published page uses. Everything else on the
 * page goes through the escaper.
 */
export function renderConflict(state: ConflictState): Fragment {
  return html`<section class="conflict" data-conflict="${state.path}" aria-label="Resolve a conflict">
    <h2>This draft changed somewhere else</h2>
    <p class="why">
      ${state.conflicts.length === 0
        ? "Your edit and the other change do not overlap, so both have been kept."
        : `${state.conflicts.length} place${state.conflicts.length === 1 ? "" : "s"} changed on both sides. Choose which to keep.`}
    </p>
    <div class="conflict-sides">
      <article data-conflict-mine aria-label="Your version">
        <h3>Yours</h3>
        ${raw(state.rendered.mine)}
      </article>
      <article data-conflict-theirs aria-label="The published version">
        <h3>Already published</h3>
        ${raw(state.rendered.theirs)}
      </article>
    </div>
    ${state.conflicts.length === 0 ? null : renderConflictRegions(state.conflicts)}
    <div class="form-actions">
      <button type="button" class="primary" data-conflict-accept>Keep mine</button>
      <button type="button" data-conflict-discard>Discard mine</button>
    </div>
  </section>`;
}

function renderConflictRegions(conflicts: Conflict[]): Fragment {
  return html`<ol class="conflict-regions">
    ${conflicts.map(
      (conflict) => html`<li data-conflict-line="${conflict.line}">
        <span class="where">Line ${conflict.line}</span>
        <del class="suggestion-before">${conflict.theirs.replace(/\n+$/, "")}</del>
        <ins class="suggestion-after">${conflict.mine.replace(/\n+$/, "")}</ins>
      </li>`,
    )}
  </ol>`;
}

// --- ED-32: the activity feed -----------------------------------------------

export function renderActivity(options: {
  entries: Activity[];
  source: Source;
  now: number;
  kinds?: ActivityKind[];
}): Fragment {
  const totals = counts(options.entries);
  return html`<section class="activity" data-activity aria-label="Activity">
    <h2>Activity</h2>
    <div class="activity-filters" role="group" aria-label="Filter activity">
      ${(Object.keys(KIND_LABEL) as ActivityKind[]).map((kind) => {
        const on = options.kinds === undefined || options.kinds.includes(kind);
        return html`<button type="button" data-activity-filter="${kind}" aria-pressed="${on ? "true" : "false"}">${KIND_LABEL[kind]} (${totals[kind]})</button>`;
      })}
    </div>
    ${options.entries.length === 0
      ? renderEmpty(options.source, "activity", "ED-32")
      : html`${byDay(options.entries, options.now).map(
          (group) => html`<section class="activity-day" data-activity-day="${group.day}">
            <h3>${group.label}</h3>
            <ul>
              ${group.entries.map(
                (entry) => html`<li class="activity-entry activity-${entry.kind}" data-activity-entry="${entry.id}">
                  <span class="activity-kind">${KIND_LABEL[entry.kind]}</span>
                  <span class="activity-who">${entry.actor}</span>
                  <span class="activity-what">${entry.summary}</span>
                </li>`,
              )}
            </ul>
          </section>`,
        )}`}
  </section>`;
}

// --- ED-13: the media library -----------------------------------------------

export function renderMedia(options: {
  assets: Asset[];
  source: Source;
  query?: string;
  selected?: string;
}): Fragment {
  const shown = searchAssets(options.assets, options.query ?? "");
  return html`<section class="media" data-media-library aria-label="Media library">
    <h2>Media</h2>
    <label class="media-search">
      Search media
      <input type="search" data-media-search value="${options.query ?? ""}" />
    </label>
    ${options.assets.length === 0
      ? renderEmpty(options.source, "media", "ED-13")
      : html`<ul class="asset-list">
          ${shown.map((asset) => renderAsset(asset, asset.path === options.selected))}
        </ul>`}
  </section>`;
}

/**
 * One asset.
 *
 * The alt text is editable here and nowhere else in the library, because it is
 * a property of the *image* rather than of one use of it — ED-13 asks for an
 * alt text editor, and putting it on the page that happens to show the image
 * would mean the same picture described differently in four places.
 *
 * Delete carries its refusal rather than discovering it on the click: an asset
 * four pages use cannot be deleted, and a button that looks available until
 * pressed teaches an author to distrust the pane.
 */
function renderAsset(asset: Asset, selected: boolean): Fragment {
  const refusal = deleteRefusal(asset);
  const id = `asset-${asset.path.replace(/[^\w-]/g, "-")}`;
  return html`<li class="asset" data-asset="${asset.path}"${selected ? raw(' aria-current="true"') : null}>
    <img src="${asset.path}" alt="${asset.alt}" loading="lazy" width="120" />
    <div class="asset-detail">
      <code>${asset.path}</code>
      <label for="${id}-alt">Description for screen readers</label>
      <input type="text" id="${id}-alt" data-asset-alt="${asset.path}" value="${asset.alt}" />
      ${asset.dark ? html`<p class="field-help">Dark-mode pair: <code>${asset.dark}</code></p>` : null}
      <p class="asset-usage">
        ${asset.usedOn.length === 0
          ? "Not used on any page."
          : `Used on ${asset.usedOn.length} page${asset.usedOn.length === 1 ? "" : "s"}: ${asset.usedOn.join(", ")}`}
      </p>
      <div class="asset-actions">
        <button type="button" data-asset-replace="${asset.path}">Replace…</button>
        <button type="button" data-asset-delete="${asset.path}"${refusal ? raw(` disabled aria-describedby="${id}-refusal"`) : null}>Delete</button>
      </div>
      ${refusal ? html`<p class="field-error" id="${id}-refusal">${refusal.message}</p>` : null}
    </div>
  </li>`;
}
