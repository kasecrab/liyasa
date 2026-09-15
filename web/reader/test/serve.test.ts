import { test, before, after } from "node:test";
import assert from "node:assert/strict";
import { mkdtemp, mkdir, writeFile } from "node:fs/promises";
import { tmpdir } from "node:os";
import { join } from "node:path";
import type { Server } from "node:http";

import { serve, resolveFile } from "../e2e/serve.mjs";

let root = "";
let server: Server;
let origin = "";

before(async () => {
  root = await mkdtemp(join(tmpdir(), "liyasa-site-"));
  await mkdir(join(root, "guide/install"), { recursive: true });
  await writeFile(join(root, "index.html"), "<!doctype html><title>Home</title>");
  await writeFile(join(root, "guide/install/index.html"), "<!doctype html><title>Install</title>");
  await writeFile(join(root, "guide/install.md"), "# Install\n");
  await writeFile(join(root, "reader.js"), "(function () {})();\n");
  server = serve(root, 0);
  await new Promise((done) => server.on("listening", done));
  const address = server.address();
  origin = `http://127.0.0.1:${typeof address === "object" && address ? address.port : 0}`;
});

after(() => server.close());

test("a route is served from its index file", async () => {
  const response = await fetch(`${origin}/guide/install`);
  assert.equal(response.status, 200);
  assert.equal(response.headers.get("content-type"), "text/html; charset=utf-8");
  assert.match(await response.text(), /Install/);
});

test("the markdown twin is served as markdown", async () => {
  const response = await fetch(`${origin}/guide/install.md`);
  assert.equal(response.headers.get("content-type"), "text/markdown; charset=utf-8");
  assert.equal(await response.text(), "# Install\n");
});

test("a script is served as javascript", async () => {
  const response = await fetch(`${origin}/reader.js`);
  assert.equal(response.headers.get("content-type"), "text/javascript; charset=utf-8");
});

test("an unknown route is a 404 with a body", async () => {
  const response = await fetch(`${origin}/nothing/here`);
  assert.equal(response.status, 404);
  assert.match(await response.text(), /Not found/);
});

test("a path may not climb out of the site", async () => {
  assert.equal(await resolveFile(root, "/../../etc/passwd"), null);
  assert.equal(await resolveFile(root, "/%2e%2e/%2e%2e/etc/passwd"), null);
});
