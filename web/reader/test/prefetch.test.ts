import { test } from "node:test";
import assert from "node:assert/strict";
import { readFileSync } from "node:fs";
import { runInNewContext } from "node:vm";

import { element, vendoredWindow } from "./dom.ts";
import type { FakeElement, VendoredOptions, VendoredWindow } from "./dom.ts";

const MODULE = readFileSync(
  new URL("../../../crates/liyasa-theme/assets/js/prefetch.js", import.meta.url),
  "utf8",
);

const SIDEBAR = '[data-liyasa="sidebar"] a[href], [data-liyasa="pagination"] a[href]';

function link(href: string, origin = "https://docs.example"): FakeElement {
  return element("a", { href, origin });
}

function load(options: VendoredOptions = {}, sidebar: FakeElement[] = []): VendoredWindow {
  const win = vendoredWindow(options);
  win.query.set(SIDEBAR, sidebar);
  const context = runInNewContext(`${MODULE}\n;globalThis;`, {
    window: win,
    document: win.document,
    navigator: (win as unknown as { navigator: unknown }).navigator,
    IntersectionObserver: (win as unknown as { IntersectionObserver: unknown }).IntersectionObserver,
  });
  assert.ok(context);
  return win;
}

function hover(win: VendoredWindow, target: FakeElement): void {
  win.document.dispatch("pointerenter", { target });
}

test("hovering a link in the site prefetches it", () => {
  const win = load();
  hover(win, link("https://docs.example/guide/configuration"));
  assert.deepEqual(win.prefetched(), ["https://docs.example/guide/configuration"]);
});

test("the same link is prefetched once", () => {
  const win = load();
  const target = link("https://docs.example/guide/configuration");
  hover(win, target);
  hover(win, target);
  assert.equal(win.prefetched().length, 1);
});

test("a link to another origin is left alone", () => {
  const win = load();
  hover(win, link("https://example.com/elsewhere", "https://example.com"));
  assert.deepEqual(win.prefetched(), []);
});

test("a download is not a page to prefetch", () => {
  const win = load();
  const target = link("https://docs.example/liyasa.tar.gz");
  target.setAttribute("download", "");
  hover(win, target);
  assert.deepEqual(win.prefetched(), []);
});

test("a reader who asked to save data is prefetched nothing", () => {
  const win = load({ saveData: true });
  hover(win, link("https://docs.example/guide/configuration"));
  assert.deepEqual(win.prefetched(), []);
  assert.equal(win.observers.length, 0, "nothing is even observed");
});

test("prefers-reduced-data is honoured like save-data", () => {
  const win = load({ reducedData: true });
  hover(win, link("https://docs.example/guide/configuration"));
  assert.deepEqual(win.prefetched(), []);
});

test("a navigation link is prefetched when it comes into view", () => {
  const target = link("https://docs.example/reference/cli");
  const win = load({}, [target]);
  const observer = win.observers[0];
  assert.ok(observer, "the sidebar is observed");
  assert.deepEqual(observer.observed, [target]);

  observer.callback([{ isIntersecting: false, target }]);
  assert.deepEqual(win.prefetched(), []);
  observer.callback([{ isIntersecting: true, target }]);
  assert.deepEqual(win.prefetched(), ["https://docs.example/reference/cli"]);
});
