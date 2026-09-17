// The HTTP calls the dashboard makes.
//
// The path list is a contract with `liyasa_analytics::api`, held to
// `test/endpoints.fixture.json` from both sides. Sixteen of these have no
// handler anywhere yet; `servedBy` says so, and a page whose data is not being
// served renders its controls and an explanation rather than an empty chart.

import { filtersToQuery } from "./filters.ts";
import type { Filters } from "./filters.ts";
import type { Grain, Range } from "./ranges.ts";

export type ServedBy = "wp-14" | "wp-16" | "unbuilt";

export interface Endpoint {
  id: string;
  method: string;
  path: string;
  requirement: string;
  servedBy: ServedBy;
}

export const API_BASE = "/_liyasa/api/v1";

export const ENDPOINTS: Endpoint[] = [
  { id: "traffic.series", method: "GET", path: `${API_BASE}/analytics/series`, requirement: "ANA-10", servedBy: "unbuilt" },
  { id: "traffic.totals", method: "GET", path: `${API_BASE}/analytics/totals`, requirement: "ANA-10", servedBy: "unbuilt" },
  { id: "traffic.pages", method: "GET", path: `${API_BASE}/analytics/pages`, requirement: "ANA-10", servedBy: "unbuilt" },
  { id: "traffic.referrers", method: "GET", path: `${API_BASE}/analytics/referrers`, requirement: "ANA-10", servedBy: "unbuilt" },
  { id: "traffic.journeys", method: "GET", path: `${API_BASE}/analytics/journeys`, requirement: "ANA-10", servedBy: "unbuilt" },
  { id: "traffic.variants", method: "GET", path: `${API_BASE}/analytics/variants`, requirement: "ANA-10", servedBy: "unbuilt" },
  { id: "traffic.delivery", method: "GET", path: `${API_BASE}/analytics/delivery`, requirement: "ANA-10", servedBy: "unbuilt" },
  { id: "traffic.horizon", method: "GET", path: `${API_BASE}/analytics/horizon`, requirement: "ANA-06", servedBy: "unbuilt" },
  { id: "search.queries", method: "GET", path: `${API_BASE}/analytics/search/queries`, requirement: "ANA-20", servedBy: "unbuilt" },
  { id: "search.pages", method: "GET", path: `${API_BASE}/analytics/search/pages`, requirement: "ANA-20", servedBy: "unbuilt" },
  { id: "search.trending", method: "GET", path: `${API_BASE}/analytics/search/trending`, requirement: "ANA-20", servedBy: "unbuilt" },
  { id: "assistant.summary", method: "GET", path: `${API_BASE}/analytics/assistant`, requirement: "ANA-10", servedBy: "unbuilt" },
  { id: "insights.cards", method: "GET", path: `${API_BASE}/analytics/insights`, requirement: "ANA-40", servedBy: "unbuilt" },
  { id: "insights.act", method: "POST", path: `${API_BASE}/analytics/insights/act`, requirement: "ANA-40", servedBy: "unbuilt" },
  { id: "settings.integrations", method: "GET", path: `${API_BASE}/analytics/integrations`, requirement: "ANA-60", servedBy: "unbuilt" },
  { id: "content.tree", method: "GET", path: `${API_BASE}/content`, requirement: "REST-02", servedBy: "wp-14" },
  { id: "feedback.list", method: "GET", path: "/_liyasa/feedback", requirement: "ANA-30", servedBy: "wp-14" },
  { id: "feedback.summary", method: "GET", path: "/_liyasa/feedback/summary", requirement: "ANA-30", servedBy: "wp-14" },
  { id: "feedback.status", method: "PATCH", path: "/_liyasa/feedback/{id}", requirement: "ANA-30", servedBy: "wp-14" },
  { id: "jobs.list", method: "GET", path: `${API_BASE}/jobs`, requirement: "HOST-07", servedBy: "wp-14" },
  { id: "jobs.retry", method: "POST", path: `${API_BASE}/jobs/{id}/retry`, requirement: "HOST-07", servedBy: "wp-14" },
  { id: "jobs.cancel", method: "POST", path: `${API_BASE}/jobs/{id}/cancel`, requirement: "HOST-07", servedBy: "wp-14" },
  { id: "deployments.list", method: "GET", path: `${API_BASE}/deployments`, requirement: "REST-01", servedBy: "wp-14" },
  { id: "deployments.current", method: "GET", path: `${API_BASE}/deployments/{env}`, requirement: "REST-01", servedBy: "wp-14" },
  { id: "builds.trigger", method: "POST", path: `${API_BASE}/builds`, requirement: "GIT-21", servedBy: "wp-16" },
  { id: "builds.queue", method: "GET", path: `${API_BASE}/builds`, requirement: "GIT-24", servedBy: "wp-16" },
  { id: "builds.status", method: "GET", path: `${API_BASE}/builds/{id}`, requirement: "GIT-21", servedBy: "wp-16" },
  { id: "builds.activate", method: "POST", path: `${API_BASE}/builds/{id}/deploy`, requirement: "GIT-21", servedBy: "wp-16" },
  { id: "deployments.history", method: "GET", path: `${API_BASE}/deployments/{env}/history`, requirement: "GIT-21", servedBy: "wp-16" },
  { id: "deployments.retained", method: "GET", path: `${API_BASE}/deployments/{env}/retained`, requirement: "GIT-40", servedBy: "wp-16" },
  { id: "deployments.rollback", method: "POST", path: `${API_BASE}/deployments/{env}/rollback/{buildId}`, requirement: "GIT-40", servedBy: "wp-16" },
  { id: "deployments.latest", method: "POST", path: `${API_BASE}/deployments/{env}/latest`, requirement: "GIT-41", servedBy: "wp-16" },
  { id: "drift.open", method: "GET", path: `${API_BASE}/drift`, requirement: "REST-05", servedBy: "unbuilt" },
  { id: "proposals.list", method: "GET", path: `${API_BASE}/proposals`, requirement: "REST-02", servedBy: "unbuilt" },
];

export function findEndpoint(id: string): Endpoint | undefined {
  return ENDPOINTS.find((endpoint) => endpoint.id === id);
}

/** Fills `{name}` holes, encoding each value. */
export function endpointPath(id: string, parameters: Record<string, string> = {}): string {
  const endpoint = findEndpoint(id);
  if (!endpoint) throw new Error(`no endpoint \`${id}\``);
  return endpoint.path.replace(/\{(\w+)\}/g, (_match, name: string) => {
    const value = parameters[name];
    if (value === undefined) throw new Error(`\`${id}\` needs a \`${name}\``);
    return encodeURIComponent(value);
  });
}

export interface QuerySpec {
  range?: Range;
  grain?: Grain;
  filters?: Filters;
  compare?: boolean;
  extra?: Record<string, string | number | undefined>;
}

/** The URL for a read, with the range, grain and ANA-71 filters on it. */
export function endpointUrl(
  id: string,
  spec: QuerySpec = {},
  parameters: Record<string, string> = {},
): string {
  const search = new URLSearchParams();
  if (spec.range) {
    search.set("from", String(spec.range.from));
    search.set("to", String(spec.range.to));
  }
  if (spec.grain) search.set("grain", spec.grain);
  if (spec.compare) search.set("compare", "1");
  for (const [name, value] of Object.entries(spec.extra ?? {})) {
    if (value !== undefined) search.set(name, String(value));
  }
  const filters = spec.filters ? filtersToQuery(spec.filters) : "";
  const query = [search.toString(), filters].filter(Boolean).join("&");
  const path = endpointPath(id, parameters);
  return query ? `${path}?${query}` : path;
}

export interface Problem {
  status: number;
  title: string;
  detail?: string;
}

export type Result<T> = { ok: true; value: T } | { ok: false; problem: Problem };

/**
 * One read.
 *
 * An endpoint nobody serves yet fails without a request: a 404 from a path
 * that was never wired reads like an outage, and this says what it is.
 */
export async function read<T>(
  id: string,
  spec: QuerySpec = {},
  parameters: Record<string, string> = {},
  fetcher: typeof fetch = fetch,
): Promise<Result<T>> {
  const endpoint = findEndpoint(id);
  if (!endpoint) return { ok: false, problem: { status: 0, title: `no endpoint \`${id}\`` } };
  if (endpoint.servedBy === "unbuilt") {
    return {
      ok: false,
      problem: {
        status: 501,
        title: "Not served yet",
        detail: `${endpoint.requirement}: no handler answers ${endpoint.path} in this build`,
      },
    };
  }
  try {
    const response = await fetcher(endpointUrl(id, spec, parameters), {
      headers: { accept: "application/json" },
    });
    if (!response.ok) {
      const body = (await response.json().catch(() => ({}))) as Record<string, unknown>;
      return {
        ok: false,
        problem: {
          status: response.status,
          title: typeof body["title"] === "string" ? body["title"] : response.statusText,
          detail: typeof body["detail"] === "string" ? body["detail"] : undefined,
        },
      };
    }
    return { ok: true, value: (await response.json()) as T };
  } catch (error) {
    return { ok: false, problem: { status: 0, title: "Request failed", detail: String(error) } };
  }
}
