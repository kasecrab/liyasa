// The endpoint contract (ANA-70), against the fixture Rust also reads.

import { readFileSync } from "node:fs";
import { dirname, resolve } from "node:path";
import { fileURLToPath } from "node:url";
import test from "node:test";
import assert from "node:assert/strict";

import { ENDPOINTS, endpointPath, endpointUrl, findEndpoint, read } from "../src/api.ts";

const HERE = dirname(fileURLToPath(import.meta.url));
const fixture = JSON.parse(readFileSync(resolve(HERE, "endpoints.fixture.json"), "utf8"));

test("the fixture and this module list the same endpoints", () => {
  assert.deepEqual(ENDPOINTS, fixture.endpoints);
});

test("a path parameter is filled and encoded", () => {
  assert.equal(endpointPath("jobs.retry", { id: "01J" }), "/_liyasa/api/v1/jobs/01J/retry");
  assert.equal(
    endpointPath("deployments.rollback", { env: "pre prod", buildId: "b/1" }),
    "/_liyasa/api/v1/deployments/pre%20prod/rollback/b%2F1",
  );
});

test("a missing path parameter is a thrown error, not a path with a hole in it", () => {
  assert.throws(() => endpointPath("jobs.retry"), /needs a `id`/);
  assert.throws(() => endpointPath("nonsense"), /no endpoint/);
});

test("a read carries the range, grain and filters", () => {
  const url = endpointUrl("traffic.series", {
    range: { from: 1, to: 2 },
    grain: "hour",
    compare: true,
    filters: { version: "v2", caller: "agent" },
  });
  assert.equal(
    url,
    "/_liyasa/api/v1/analytics/series?from=1&to=2&grain=hour&compare=1&version=v2&caller=agent",
  );
});

test("a read with nothing to say carries no query string", () => {
  assert.equal(endpointUrl("jobs.list"), "/_liyasa/api/v1/jobs");
});

test("an endpoint nobody serves fails without a request", async () => {
  let called = false;
  const fetcher = (async () => {
    called = true;
    return new Response("{}");
  }) as unknown as typeof fetch;
  const result = await read("traffic.series", {}, {}, fetcher);
  assert.equal(result.ok, false);
  if (!result.ok) {
    assert.equal(result.problem.status, 501);
    assert.match(result.problem.detail ?? "", /no handler answers/);
  }
  assert.equal(called, false, "a 404 from a path nobody wired reads like an outage");
});

test("a served endpoint is fetched and its body returned", async () => {
  const fetcher = (async (url: string) => {
    assert.equal(url, "/_liyasa/api/v1/jobs");
    return new Response(JSON.stringify({ items: [] }), {
      status: 200,
      headers: { "content-type": "application/json" },
    });
  }) as unknown as typeof fetch;
  const result = await read<{ items: unknown[] }>("jobs.list", {}, {}, fetcher);
  assert.equal(result.ok, true);
  if (result.ok) assert.deepEqual(result.value, { items: [] });
});

test("a problem response becomes a problem rather than an exception", async () => {
  const fetcher = (async () =>
    new Response(JSON.stringify({ title: "Too many requests", detail: "slow down" }), {
      status: 429,
    })) as unknown as typeof fetch;
  const result = await read("jobs.list", {}, {}, fetcher);
  assert.equal(result.ok, false);
  if (!result.ok) {
    assert.equal(result.problem.status, 429);
    assert.equal(result.problem.title, "Too many requests");
  }
});

test("a network failure becomes a problem too", async () => {
  const fetcher = (async () => {
    throw new Error("offline");
  }) as unknown as typeof fetch;
  const result = await read("jobs.list", {}, {}, fetcher);
  assert.equal(result.ok, false);
  if (!result.ok) assert.equal(result.problem.status, 0);
});

test("ANA-02's schema is listed and is the only public route", () => {
  const schema = findEndpoint("schema.event");
  assert.equal(schema?.path, "/_liyasa/schema/event.json");
  assert.equal(schema?.auth, "public");
  const publicOnes = ENDPOINTS.filter((e) => e.auth === "public").map((e) => e.id);
  assert.deepEqual(publicOnes, ["schema.event"]);
});

test("every endpoint the fixture calls unbuilt is really unbuilt here", () => {
  for (const endpoint of fixture.endpoints) {
    assert.equal(findEndpoint(endpoint.id)?.servedBy, endpoint.servedBy, endpoint.id);
  }
});
