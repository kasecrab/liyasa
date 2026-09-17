// The six filter dimensions of ANA-71, plus the site and environment every
// request is scoped to.
//
// The query-string names and their order are a contract with
// `liyasa_analytics::query::Filters`. `test/filters.fixture.json` is the
// document both sides are held to; neither is checked against the other's
// output.

export type CallerKind = "human" | "agent" | "bot" | "integration";

export const CALLER_KINDS: CallerKind[] = ["human", "agent", "bot", "integration"];

export interface Filters {
  site?: string;
  env?: string;
  version?: string;
  locale?: string;
  region?: string;
  product?: string;
  caller?: CallerKind;
  routePrefix?: string;
}

/** Query-string name against object field, in the canonical order. */
export const FILTER_NAMES: Array<[string, keyof Filters]> = [
  ["site", "site"],
  ["env", "env"],
  ["version", "version"],
  ["locale", "locale"],
  ["region", "region"],
  ["product", "product"],
  ["caller", "caller"],
  ["route", "routePrefix"],
];

/** The dimensions a filter bar offers, which is ANA-71's list. */
export const FILTER_DIMENSIONS: Array<{ name: string; field: keyof Filters; label: string }> = [
  { name: "version", field: "version", label: "Version" },
  { name: "locale", field: "locale", label: "Locale" },
  { name: "region", field: "region", label: "Region" },
  { name: "product", field: "product", label: "Product" },
  { name: "caller", field: "caller", label: "Caller" },
];

export function filtersToQuery(filters: Filters): string {
  const parameters = new URLSearchParams();
  for (const [name, field] of FILTER_NAMES) {
    const value = filters[field];
    if (value) parameters.append(name, value);
  }
  return parameters.toString();
}

/**
 * Reads a query string back.
 *
 * An unknown name is ignored and an unknown caller kind leaves the filter
 * unset rather than selecting nothing, so a URL bookmarked before a release
 * still shows traffic.
 */
export function filtersFromQuery(query: string): Filters {
  const parameters = new URLSearchParams(query.replace(/^\?/, ""));
  const filters: Filters = {};
  for (const [name, field] of FILTER_NAMES) {
    const value = parameters.get(name);
    if (!value) continue;
    if (field === "caller") {
      if ((CALLER_KINDS as string[]).includes(value)) filters.caller = value as CallerKind;
      continue;
    }
    filters[field] = value;
  }
  return filters;
}

/** How many dimensions are set, which is what the filter button counts. */
export function activeFilterCount(filters: Filters): number {
  return FILTER_NAMES.filter(([, field]) => Boolean(filters[field])).length;
}

export function filtersEqual(a: Filters, b: Filters): boolean {
  return filtersToQuery(a) === filtersToQuery(b);
}
