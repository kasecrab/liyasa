// ANA-71's filter contract, against the fixture Rust also reads.
//
// `crates/liyasa-analytics/tests/it/query.rs` holds the other side to the same
// file. Neither checks the other's output; a name that differs between them
// fails here and there rather than silently filtering nothing.

import { readFileSync } from "node:fs";
import { dirname, resolve } from "node:path";
import { fileURLToPath } from "node:url";
import test from "node:test";
import assert from "node:assert/strict";

import {
  activeFilterCount,
  FILTER_NAMES,
  filtersEqual,
  filtersFromQuery,
  filtersToQuery,
} from "../src/filters.ts";
import type { Filters } from "../src/filters.ts";

const HERE = dirname(fileURLToPath(import.meta.url));
const fixture = JSON.parse(readFileSync(resolve(HERE, "filters.fixture.json"), "utf8"));

test("the fixture and this module agree on the names", () => {
  assert.deepEqual(
    FILTER_NAMES.map(([name]) => name),
    fixture.names,
  );
});

test("every fixture case encodes and parses back", () => {
  for (const testCase of fixture.cases) {
    const filters = testCase.filters as Filters;
    assert.equal(filtersToQuery(filters), testCase.query, testCase.why);
    assert.deepEqual(filtersFromQuery(testCase.query), filters, testCase.why);
  }
});

test("the lenient cases are lenient in the same way", () => {
  for (const testCase of fixture.lenient) {
    assert.deepEqual(filtersFromQuery(testCase.query), testCase.filters as Filters, testCase.why);
  }
});

test("a leading question mark is not part of a name", () => {
  assert.deepEqual(filtersFromQuery("?version=v2"), { version: "v2" });
});

test("the count is what a filter button shows", () => {
  assert.equal(activeFilterCount({}), 0);
  assert.equal(activeFilterCount({ version: "v2", caller: "agent" }), 2);
  assert.equal(activeFilterCount({ version: "" }), 0, "an empty value is not a filter");
});

test("two filter sets are equal when they mean the same query", () => {
  assert.ok(filtersEqual({ version: "v2" }, { version: "v2" }));
  assert.ok(
    filtersEqual({ version: "v2", locale: undefined }, { version: "v2" }),
    "an unset dimension is not a difference",
  );
  assert.ok(!filtersEqual({ version: "v2" }, { version: "v1" }));
});
