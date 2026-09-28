// A page must render what it fetches.
//
// This catches one specific way a dashboard lies: `PAGE_ENDPOINTS` names an
// endpoint, `dashboard.ts` requests it on every page load, and no renderer
// reads the result. The request succeeds, the page looks complete, and the
// requirement clause behind it is unmet. Three endpoints were in that state
// when this test was written — `traffic.variants`, `feedback.ratings` and
// `feedback.pages` — and nothing else in the suite could see it, because every
// other test asserts on markup that IS produced.
//
// The check reads `src/pages.ts` as text rather than driving the renderers,
// because the failure is structural: an endpoint no renderer mentions cannot
// reach the screen whatever data it returns.

import { readFileSync } from "node:fs";
import { dirname, resolve } from "node:path";
import { fileURLToPath } from "node:url";
import test from "node:test";
import assert from "node:assert/strict";

import { PAGE_ENDPOINTS } from "../src/pages.ts";
import { findEndpoint } from "../src/api.ts";
import { PAGES } from "../src/router.ts";

const HERE = dirname(fileURLToPath(import.meta.url));
const source = readFileSync(resolve(HERE, "..", "src", "pages.ts"), "utf8");

/** The body of one renderer, by the page id it draws. */
function rendererBody(page: string): string {
  const name = `render${page[0]!.toUpperCase()}${page.slice(1)}`;
  const start = source.indexOf(`export function ${name}(`);
  assert.notEqual(start, -1, `${page} has no ${name}`);
  const end = source.indexOf("\nexport ", start + 10);
  return source.slice(start, end === -1 ? undefined : end);
}

test("every endpoint a page fetches is read by that page's renderer", () => {
  const orphaned: string[] = [];
  for (const [page, ids] of Object.entries(PAGE_ENDPOINTS)) {
    const body = rendererBody(page);
    for (const id of ids) {
      if (!body.includes(`data["${id}"]`)) orphaned.push(`${page} fetches ${id} and draws nothing`);
    }
  }
  assert.deepEqual(
    orphaned,
    [],
    "an endpoint fetched and discarded is a request that succeeds while the clause behind it is unmet",
  );
});

test("every endpoint a renderer reads is one the page fetches", () => {
  // The other direction: a renderer that reads `data["x"]` for an endpoint the
  // page never requests draws its not-loaded panel forever.
  const unrequested: string[] = [];
  for (const page of Object.keys(PAGE_ENDPOINTS)) {
    const declared = new Set(PAGE_ENDPOINTS[page]);
    const body = rendererBody(page);
    for (const match of body.matchAll(/data\["([^"]+)"\]/g)) {
      if (!declared.has(match[1]!)) unrequested.push(`${page} reads ${match[1]} and never fetches it`);
    }
  }
  assert.deepEqual(unrequested, []);
});

test("every endpoint any page names exists in the api list", () => {
  for (const [page, ids] of Object.entries(PAGE_ENDPOINTS)) {
    for (const id of ids) {
      assert.ok(findEndpoint(id), `${page} names \`${id}\`, which is not an endpoint`);
    }
  }
});

test("every page in the navigation has an endpoint list", () => {
  for (const page of PAGES) {
    assert.ok(PAGE_ENDPOINTS[page.id], `${page.id} is in the nav and reads nothing`);
  }
});
