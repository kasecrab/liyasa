// The toolbar every page carries (ANA-71): the date range, the
// period-over-period toggle, the six filters, and the saved views.
//
// Each control is a link rather than a form. A link is shareable, works before
// any script runs, restores on a back button, and is what a saved view stores;
// a `<select>` that only works once `dashboard.js` has loaded is none of those.

import { html } from "./escape.ts";
import type { Fragment } from "./escape.ts";
import { FILTER_DIMENSIONS, activeFilterCount } from "./filters.ts";
import type { Filters } from "./filters.ts";
import { describeRange, RANGE_PRESETS } from "./ranges.ts";
import type { Range } from "./ranges.ts";
import { routeHref } from "./router.ts";
import type { RouteState } from "./router.ts";
import type { SavedView } from "./views.ts";
import { viewHref } from "./views.ts";

/** What the filter bar offers for each dimension, from the data on screen. */
export interface FilterOptions {
  version?: string[];
  locale?: string[];
  region?: string[];
  product?: string[];
  caller?: string[];
}

export function renderRangePicker(state: RouteState, range: Range): Fragment {
  const current =
    state.rangeSpec.kind === "last" ? state.rangeSpec.days : Math.round((range.to - range.from) / 86_400_000);
  return html`<fieldset class="ly-control ly-range">
    <legend>Range</legend>
    ${RANGE_PRESETS.map((preset) => {
      const href = routeHref({ ...state, rangeSpec: { kind: "last", days: preset.days } });
      const active = state.rangeSpec.kind === "last" && state.rangeSpec.days === preset.days;
      return html`<a href="${href}" aria-current="${active ? "true" : "false"}">${preset.label}</a>`;
    })}
    <span class="ly-range-current">${describeRange(range)}</span>
    <span class="ly-visually-hidden">${String(current)} days</span>
  </fieldset>`;
}

export function renderCompareToggle(state: RouteState): Fragment {
  const href = routeHref({ ...state, compare: !state.compare });
  return html`<a
    class="ly-control ly-compare"
    href="${href}"
    role="switch"
    aria-checked="${state.compare ? "true" : "false"}"
    >Compare with the previous period</a
  >`;
}

export function renderFilterBar(state: RouteState, options: FilterOptions): Fragment {
  const count = activeFilterCount(state.filters);
  const groups = FILTER_DIMENSIONS.map((dimension) => {
    const values = options[dimension.name as keyof FilterOptions] ?? [];
    const chosen = state.filters[dimension.field];
    if (values.length === 0 && !chosen) return null;
    const items = values.map((value) => {
      const next: Filters = { ...state.filters, [dimension.field]: value };
      const active = chosen === value;
      const href = routeHref({
        ...state,
        filters: active ? { ...state.filters, [dimension.field]: undefined } : next,
      });
      return html`<a href="${href}" aria-pressed="${active ? "true" : "false"}">${value}</a>`;
    });
    return html`<div class="ly-filter-group" data-dimension="${dimension.name}">
      <span class="ly-filter-label">${dimension.label}</span>
      ${items}
    </div>`;
  });
  const clear = count > 0 ? html`<a class="ly-filter-clear" href="${routeHref({ ...state, filters: {} })}">Clear ${String(count)}</a>` : null;
  return html`<section class="ly-control ly-filters" aria-label="Filters">
    ${groups}${clear}
  </section>`;
}

export function renderSavedViews(state: RouteState, views: SavedView[]): Fragment {
  const forPage = views.filter((view) => view.page === state.page);
  return html`<section class="ly-control ly-views" aria-label="Saved views">
    <span class="ly-filter-label">Saved</span>
    ${forPage.length === 0
      ? html`<span class="ly-empty">none yet</span>`
      : forPage.map(
          (view) =>
            html`<a href="${viewHref(view)}" data-view="${view.id}">${view.name}</a
              ><button type="button" data-remove-view="${view.id}" aria-label="Delete ${view.name}">
                ×
              </button>`,
        )}
    <button type="button" data-save-view="${state.page}">Save this view</button>
  </section>`;
}

export function renderToolbar(
  state: RouteState,
  range: Range,
  options: FilterOptions,
  views: SavedView[],
): Fragment {
  return html`<div class="ly-toolbar">
    ${renderRangePicker(state, range)} ${renderCompareToggle(state)}
    ${renderFilterBar(state, options)} ${renderSavedViews(state, views)}
  </div>`;
}

/** The navigation, which is eleven links and nothing clever. */
export function renderNav(state: RouteState, pages: Array<{ id: string; label: string }>): Fragment {
  return html`<nav class="ly-nav" aria-label="Dashboard">
    <ul>
      ${pages.map((page) => {
        const href = routeHref({ ...state, page: page.id, focus: undefined });
        const current = page.id === state.page;
        return html`<li>
          <a href="${href}" aria-current="${current ? "page" : "false"}">${page.label}</a>
        </li>`;
      })}
    </ul>
  </nav>`;
}
