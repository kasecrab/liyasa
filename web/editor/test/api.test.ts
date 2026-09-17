// The editor's HTTP surface, and the fact that nothing serves it yet.

import test from "node:test";
import assert from "node:assert/strict";

import { ENDPOINTS, call, endpointPath, findEndpoint, unservedBy } from "../src/api.ts";

test("every endpoint says which package serves it", () => {
  for (const endpoint of ENDPOINTS) {
    assert.ok(endpoint.requirement, `${endpoint.id} names its requirement`);
    assert.ok(["wp-14", "wp-16", "unbuilt"].includes(endpoint.servedBy), `${endpoint.id} names a server`);
    assert.match(endpoint.path, /^\/_liyasa\//, `${endpoint.id} is under the reserved prefix`);
  }
});

test("no two endpoints share an id", () => {
  const ids = ENDPOINTS.map((endpoint) => endpoint.id);
  assert.equal(new Set(ids).size, ids.length);
});

test("the editor's own routes are all unbuilt, and the list says so", () => {
  // `crates/liyasa-server/src/routes/` has no `/_liyasa/editor/` anywhere. A
  // list that claimed otherwise would send the editor at routes that 404, and
  // a 404 from a path nobody wired reads like an outage.
  const editorRoutes = ENDPOINTS.filter((endpoint) => endpoint.path.startsWith("/_liyasa/editor/"));
  assert.ok(editorRoutes.length > 10, "the editor needs more than a handful of routes");
  for (const endpoint of editorRoutes) {
    assert.equal(endpoint.servedBy, "unbuilt", `${endpoint.id} is not served by anything yet`);
  }
});

test("the routes that do exist are marked as served", () => {
  // These are in `liyasa-server` today; claiming they are unbuilt would make
  // the editor refuse calls that would have worked.
  for (const id of ["content.tree", "builds.trigger", "builds.status", "deployments.current"]) {
    assert.notEqual(findEndpoint(id)?.servedBy, "unbuilt", `${id} is served`);
  }
});

test("a path parameter is filled and encoded", () => {
  assert.equal(endpointPath("drafts.get", { id: "liyasa/ada/rate limits" }), "/_liyasa/editor/drafts/liyasa%2Fada%2Frate%20limits");
});

test("a missing path parameter is an error, not a path with a hole in it", () => {
  assert.throws(() => endpointPath("drafts.get"), /needs an? `id`/);
  assert.throws(() => endpointPath("nonsense"), /no endpoint/);
});

test("an unbuilt endpoint fails without making a request", async () => {
  let called = false;
  const result = await call("drafts.list", {}, {}, async () => {
    called = true;
    return new Response("{}");
  });
  assert.equal(called, false, "no request was made");
  assert.equal(result.ok, false);
  assert.equal(!result.ok && result.problem.status, 501);
  assert.match((!result.ok && result.problem.detail) || "", /ED-20/);
});

test("a served endpoint makes the request and returns the body", async () => {
  const result = await call<{ pages: string[] }>("content.tree", {}, {}, async () =>
    new Response(JSON.stringify({ pages: ["a.md"] }), {
      status: 200,
      headers: { "content-type": "application/json" },
    }),
  );
  assert.equal(result.ok, true);
  assert.deepEqual(result.ok && result.value, { pages: ["a.md"] });
});

test("a failed request carries the problem the server sent, not a guess", async () => {
  const result = await call("content.tree", {}, {}, async () =>
    new Response(JSON.stringify({ title: "Forbidden", detail: "no `content:read`" }), { status: 403 }),
  );
  assert.equal(result.ok, false);
  assert.equal(!result.ok && result.problem.status, 403);
  assert.equal(!result.ok && result.problem.title, "Forbidden");
});

test("a network failure is reported as one rather than as an empty answer", async () => {
  // An editor that treated a dropped connection as "no drafts" shows an empty
  // list to somebody who has twelve.
  const result = await call("content.tree", {}, {}, async () => {
    throw new TypeError("network error");
  });
  assert.equal(result.ok, false);
  assert.equal(!result.ok && result.problem.status, 0);
  assert.match((!result.ok && result.problem.detail) || "", /network error/);
});

test("a write carries the body and the method the endpoint declares", async () => {
  let seen: RequestInit | undefined;
  await call("content.tree", { body: { a: 1 }, method: "POST" }, {}, async (_url, init) => {
    seen = init;
    return new Response("{}", { status: 200, headers: { "content-type": "application/json" } });
  });
  assert.equal(seen?.method, "POST");
  assert.equal(seen?.body, JSON.stringify({ a: 1 }));
  assert.equal((seen?.headers as Record<string, string>)["content-type"], "application/json");
});

test("the requirements waiting on a server are listed, so a status can be honest", () => {
  const waiting = unservedBy();
  for (const requirement of ["ED-20", "ED-22", "ED-23", "ED-25", "ED-26", "ED-32", "ED-50", "ED-52"]) {
    assert.ok(waiting.includes(requirement), `${requirement} is waiting on a handler`);
  }
});
