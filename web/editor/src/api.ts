// The HTTP calls the editor makes.
//
// **Almost none of them are served.** `crates/liyasa-server/src/routes/` has
// no `/_liyasa/editor/` route at all: drafts, reviews, previews, the activity
// feed and the agent are WP-14's and WP-16's surfaces and do not exist yet.
// `servedBy` says so per route, `call` refuses an unbuilt one without making a
// request, and the pane that needed it says what is missing rather than
// rendering an empty list.
//
// That last part is the whole point. An editor that asked for drafts, got a
// 404 from a path nobody wired, and drew an empty drafts list would be telling
// an author with twelve drafts that they have none. `web/dashboard/src/api.ts`
// took the same decision one package earlier, for the same reason.

export type ServedBy = "wp-14" | "wp-16" | "unbuilt";

export interface Endpoint {
  id: string;
  method: "GET" | "POST" | "PUT" | "PATCH" | "DELETE";
  path: string;
  requirement: string;
  servedBy: ServedBy;
}

export const API_BASE = "/_liyasa/api/v1";
export const EDITOR_BASE = "/_liyasa/editor";

export const ENDPOINTS: Endpoint[] = [
  // ED-07: the lazy file system the WebAssembly session resolves through.
  { id: "fs.read", method: "GET", path: `${EDITOR_BASE}/fs/{path}`, requirement: "ED-07", servedBy: "unbuilt" },

  // ED-20, ED-21: drafts and their versioned autosave.
  { id: "drafts.list", method: "GET", path: `${EDITOR_BASE}/drafts`, requirement: "ED-20", servedBy: "unbuilt" },
  { id: "drafts.create", method: "POST", path: `${EDITOR_BASE}/drafts`, requirement: "ED-20", servedBy: "unbuilt" },
  { id: "drafts.get", method: "GET", path: `${EDITOR_BASE}/drafts/{id}`, requirement: "ED-20", servedBy: "unbuilt" },
  { id: "drafts.save", method: "PUT", path: `${EDITOR_BASE}/drafts/{id}/files`, requirement: "ED-21", servedBy: "unbuilt" },
  { id: "drafts.tab", method: "POST", path: `${EDITOR_BASE}/drafts/{id}/tab`, requirement: "ED-21", servedBy: "unbuilt" },

  // ED-22: a preview build of the draft.
  { id: "preview.build", method: "POST", path: `${EDITOR_BASE}/drafts/{id}/preview`, requirement: "ED-22", servedBy: "unbuilt" },
  { id: "preview.render", method: "POST", path: `${EDITOR_BASE}/render`, requirement: "ED-07", servedBy: "unbuilt" },

  // ED-23, ED-24, ED-51: review and the publishing policy.
  { id: "review.submit", method: "POST", path: `${EDITOR_BASE}/drafts/{id}/review`, requirement: "ED-23", servedBy: "unbuilt" },
  { id: "review.comments", method: "GET", path: `${EDITOR_BASE}/reviews/{id}/comments`, requirement: "ED-23", servedBy: "unbuilt" },
  { id: "review.comment", method: "POST", path: `${EDITOR_BASE}/reviews/{id}/comments`, requirement: "ED-23", servedBy: "unbuilt" },
  { id: "review.decide", method: "POST", path: `${EDITOR_BASE}/reviews/{id}/decision`, requirement: "ED-51", servedBy: "unbuilt" },
  { id: "policy.get", method: "GET", path: `${EDITOR_BASE}/policy`, requirement: "ED-24", servedBy: "unbuilt" },
  { id: "publish.now", method: "POST", path: `${EDITOR_BASE}/drafts/{id}/publish`, requirement: "ED-24", servedBy: "unbuilt" },

  // ED-25, ED-26: git sync and the workspace that has none.
  { id: "sync.events", method: "GET", path: `${EDITOR_BASE}/events`, requirement: "ED-25", servedBy: "unbuilt" },
  { id: "workspace.revisions", method: "GET", path: `${EDITOR_BASE}/workspace/revisions`, requirement: "ED-26", servedBy: "unbuilt" },
  { id: "workspace.restore", method: "POST", path: `${EDITOR_BASE}/workspace/restore/{revision}`, requirement: "ED-26", servedBy: "unbuilt" },
  { id: "workspace.export", method: "POST", path: `${EDITOR_BASE}/workspace/export`, requirement: "ED-26", servedBy: "unbuilt" },

  // ED-32: the activity feed.
  { id: "activity.feed", method: "GET", path: `${EDITOR_BASE}/activity`, requirement: "ED-32", servedBy: "unbuilt" },

  // ED-40, ED-41, ED-42: the sidebar agent, on the operator's keys.
  { id: "agent.run", method: "POST", path: `${EDITOR_BASE}/agent/run`, requirement: "ED-40", servedBy: "unbuilt" },
  { id: "agent.policy", method: "GET", path: `${EDITOR_BASE}/agent/policy`, requirement: "ED-42", servedBy: "unbuilt" },

  // ED-50, ED-52: the unified review queue and path ownership.
  { id: "queue.list", method: "GET", path: `${EDITOR_BASE}/queue`, requirement: "ED-50", servedBy: "unbuilt" },
  { id: "owners.for", method: "GET", path: `${EDITOR_BASE}/owners`, requirement: "ED-52", servedBy: "unbuilt" },

  // ED-13: the media library's uploads.
  { id: "assets.list", method: "GET", path: `${EDITOR_BASE}/assets`, requirement: "ED-13", servedBy: "unbuilt" },
  { id: "assets.upload", method: "POST", path: `${EDITOR_BASE}/assets`, requirement: "ED-13", servedBy: "unbuilt" },
  { id: "assets.delete", method: "DELETE", path: `${EDITOR_BASE}/assets/{path}`, requirement: "ED-13", servedBy: "unbuilt" },

  // These four exist in `liyasa-server` today.
  { id: "content.tree", method: "GET", path: `${API_BASE}/content`, requirement: "REST-02", servedBy: "wp-14" },
  { id: "builds.trigger", method: "POST", path: `${API_BASE}/builds`, requirement: "GIT-21", servedBy: "wp-16" },
  { id: "builds.status", method: "GET", path: `${API_BASE}/builds/{id}`, requirement: "GIT-21", servedBy: "wp-16" },
  { id: "deployments.current", method: "GET", path: `${API_BASE}/deployments/{env}`, requirement: "REST-01", servedBy: "wp-14" },
];

export function findEndpoint(id: string): Endpoint | undefined {
  return ENDPOINTS.find((endpoint) => endpoint.id === id);
}

/** Fills `{name}` holes, encoding each value. */
export function endpointPath(id: string, parameters: Record<string, string> = {}): string {
  const endpoint = findEndpoint(id);
  if (!endpoint) throw new Error(`no endpoint \`${id}\``);
  return endpoint.path.replace(/\{(\w+)\}/g, (_whole, name: string) => {
    const value = parameters[name];
    if (value === undefined) {
      throw new Error(`\`${id}\` needs ${/^[aeiou]/i.test(name) ? "an" : "a"} \`${name}\``);
    }
    return encodeURIComponent(value);
  });
}

/** Every requirement of this package that is waiting on a handler somewhere. */
export function unservedBy(): string[] {
  return [
    ...new Set(
      ENDPOINTS.filter((endpoint) => endpoint.servedBy === "unbuilt").map(
        (endpoint) => endpoint.requirement,
      ),
    ),
  ].sort();
}

export interface Problem {
  status: number;
  title: string;
  detail?: string;
}

export type Result<T> = { ok: true; value: T } | { ok: false; problem: Problem };

export interface CallSpec {
  method?: Endpoint["method"];
  body?: unknown;
  query?: Record<string, string | number | undefined>;
}

/**
 * One call.
 *
 * An endpoint nobody serves fails here, without a request. The refusal names
 * the requirement, so the pane can say "ED-20 is not built in this server"
 * rather than showing an outage or, worse, an empty result.
 */
export async function call<T>(
  id: string,
  spec: CallSpec = {},
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
        detail: `${endpoint.requirement}: no handler answers ${endpoint.method} ${endpoint.path} in this build`,
      },
    };
  }

  const search = new URLSearchParams();
  for (const [name, value] of Object.entries(spec.query ?? {})) {
    if (value !== undefined) search.set(name, String(value));
  }
  const path = endpointPath(id, parameters);
  const url = search.toString() === "" ? path : `${path}?${search}`;

  const headers: Record<string, string> = { accept: "application/json" };
  const init: RequestInit = { method: spec.method ?? endpoint.method, headers };
  if (spec.body !== undefined) {
    headers["content-type"] = "application/json";
    init.body = JSON.stringify(spec.body);
  }

  try {
    const response = await fetcher(url, init);
    if (!response.ok) {
      const body = (await response.json().catch(() => ({}))) as Record<string, unknown>;
      return {
        ok: false,
        problem: {
          status: response.status,
          title: typeof body["title"] === "string" ? body["title"] : response.statusText || "Request failed",
          ...(typeof body["detail"] === "string" ? { detail: body["detail"] } : {}),
        },
      };
    }
    return { ok: true, value: (await response.json()) as T };
  } catch (cause) {
    // A dropped connection is not an empty answer, and drawing one as an empty
    // list tells an author with twelve drafts that they have none.
    return { ok: false, problem: { status: 0, title: "Could not reach the server", detail: String(cause) } };
  }
}
