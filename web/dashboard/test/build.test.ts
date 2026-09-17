// The bundle builds, fits its budget, and the committed `dist/` matches.

import { readFileSync } from "node:fs";
import { dirname, resolve } from "node:path";
import { fileURLToPath } from "node:url";
import { gzipSync } from "node:zlib";
import test from "node:test";
import assert from "node:assert/strict";

import { bundle } from "../../reader/build.mjs";
import { ENTRIES } from "../build.mjs";

const HERE = dirname(fileURLToPath(import.meta.url));

test("the committed bundle is what the source produces", () => {
  for (const { entry, out, budget } of ENTRIES) {
    const built = bundle(resolve(HERE, "..", entry), (path: string) => readFileSync(path, "utf8"));
    const committed = readFileSync(resolve(HERE, "..", "dist", out), "utf8");
    assert.equal(built, committed, `${out} is stale: run \`npm run build:dashboard\``);
    const compressed = gzipSync(Buffer.from(built), { level: 9 }).length;
    assert.ok(compressed <= budget, `${out}: ${compressed} bytes compressed, budget ${budget}`);
  }
});

test("the bundle touches the document in one place", () => {
  const built = readFileSync(resolve(HERE, "..", "dist", "dashboard.js"), "utf8");
  const mounts = built.match(/document\.getElementById/g) ?? [];
  assert.equal(mounts.length, 1, "rendering is pure; only the entry point mounts");
});

test("the shell lists ANA-70's pages without any script running", () => {
  const shell = readFileSync(resolve(HERE, "..", "index.html"), "utf8");
  for (const page of [
    "overview",
    "traffic",
    "search",
    "assistant",
    "feedback",
    "truth",
    "proposals",
    "deployments",
    "automations",
    "content",
    "settings",
  ]) {
    assert.ok(shell.includes(`#/${page}`), `${page} is missing from the shell`);
  }
  assert.match(shell, /<noscript>/);
});
