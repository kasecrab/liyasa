import { test } from "node:test";
import assert from "node:assert/strict";

import { bundle } from "../build.mjs";

/** A module graph in memory, keyed by the absolute paths `bundle` resolves. */
function files(map: Record<string, string>): (path: string) => string {
  return (path: string) => {
    const source = map[path];
    if (source === undefined) throw new Error(`no such module ${path}`);
    return source;
  };
}

test("a dependency is emitted before the module that imports it", () => {
  const code = bundle(
    "/site/entry.ts",
    files({
      "/site/entry.ts": 'import { greet } from "./greet.ts";\ngreet();\n',
      "/site/greet.ts": 'export function greet(): void {\n  console.log("hi");\n}\n',
    }),
  );
  assert.ok(code.indexOf("function greet") < code.indexOf("greet();"));
  assert.match(code, /^\(function \(\) \{\n"use strict";/);
  assert.doesNotMatch(code, /import|export/);
});

test("type annotations are stripped and runtime code is not", () => {
  const code = bundle(
    "/site/entry.ts",
    files({
      "/site/entry.ts": "export const size: number = 2;\nexport type Big = { size: number };\n",
    }),
  );
  assert.match(code, /const size\s+= 2;/);
  assert.doesNotMatch(code, /Big/);
});

test("a module is emitted once however often it is imported", () => {
  const code = bundle(
    "/site/entry.ts",
    files({
      "/site/entry.ts": 'import { a } from "./a.ts";\nimport { b } from "./b.ts";\na();\nb();\n',
      "/site/a.ts": 'import { shared } from "./shared.ts";\nexport function a() { shared(); }\n',
      "/site/b.ts": 'import { shared } from "./shared.ts";\nexport function b() { shared(); }\n',
      "/site/shared.ts": "export function shared() {}\n",
    }),
  );
  assert.equal(code.match(/function shared/g)?.length, 1);
});

test("two modules may not declare the same name", () => {
  assert.throws(
    () =>
      bundle(
        "/site/entry.ts",
        files({
          "/site/entry.ts": 'import { install } from "./one.ts";\nimport { install as other } from "./two.ts";\n',
          "/site/one.ts": "export function install() {}\n",
          "/site/two.ts": "export function install() {}\n",
        }),
      ),
    /`install` is declared in/,
  );
});

test("a circular import is a build failure, not a stack overflow", () => {
  assert.throws(
    () =>
      bundle(
        "/site/entry.ts",
        files({
          "/site/entry.ts": 'import { a } from "./a.ts";\n',
          "/site/a.ts": 'import { b } from "./b.ts";\nexport const a = b;\n',
          "/site/b.ts": 'import { a } from "./a.ts";\nexport const b = a;\n',
        }),
      ),
    /circular import/,
  );
});

test("only relative TypeScript modules may be imported", () => {
  for (const [source, message] of [
    ['import { test } from "node:test";\n', /not a relative import/],
    ['import { a } from "./a.js";\n', /does not name a \.ts file/],
  ] as const) {
    assert.throws(() => bundle("/site/entry.ts", files({ "/site/entry.ts": source })), message);
  }
});
