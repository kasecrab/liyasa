// Builds the reader runtime without installing anything.
//
// The toolchain PRD §6.2 names for `web/` is Vite 8 with the native TypeScript
// compiler, and this file is not a replacement for it: it is the subset that
// produces the two shipped bundles, written against what Node 24 already has
// (`module.stripTypeScriptTypes`), so the runtime builds and tests in an
// environment with no package registry. `plan/rfcs/1100-reader-toolchain.md`
// records the trade and what replaces it.
//
// The rules a source file follows, all enforced below:
//
//   * imports are relative, name a `.ts` file, and are never circular
//   * every binding is declared once across the whole entry graph
//   * `export` marks what other modules read; nothing is a default export
//
// The output is one classic script — the theme loads it with `<script defer>`,
// not as a module (THM-31), so the graph is concatenated in dependency order
// inside a single function scope.

import { readFileSync, writeFileSync, mkdirSync } from "node:fs";
import { dirname, resolve } from "node:path";
import { fileURLToPath } from "node:url";
import { gzipSync } from "node:zlib";
import { stripTypeScriptTypes } from "node:module";

const HERE = dirname(fileURLToPath(import.meta.url));

/** Entry, output name, and the THM-31 budget the compressed output must fit. */
export const ENTRIES = [
  { entry: "src/reader.ts", out: "reader.js", budget: 50 * 1024 },
  { entry: "src/measure.ts", out: "measure.js", budget: 8 * 1024 },
];

const IMPORT = /^\s*import\s+(?:\{[^}]*\}|[\w*\s,]+)\s+from\s+["']([^"']+)["'];?\s*$/gm;
const BARE_IMPORT = /^\s*import\s+["']([^"']+)["'];?\s*$/gm;
const EXPORT_KEYWORD = /^export\s+(?=const|let|var|function|class|async|interface|type)/gm;
const DECLARATION = /^(?:export\s+)?(?:async\s+)?(?:function|class|const|let|var)\s+([A-Za-z_$][\w$]*)/gm;

function read(path) {
  return readFileSync(path, "utf8");
}

function dependencies(source, from) {
  const found = [];
  for (const match of source.matchAll(IMPORT)) found.push(match[1]);
  for (const match of source.matchAll(BARE_IMPORT)) found.push(match[1]);
  return found.map((specifier) => {
    if (!specifier.startsWith(".")) {
      throw new Error(`${from}: \`${specifier}\` is not a relative import`);
    }
    if (!specifier.endsWith(".ts")) {
      throw new Error(`${from}: \`${specifier}\` does not name a .ts file`);
    }
    return resolve(dirname(from), specifier);
  });
}

/** Type annotations out, module syntax out, everything else untouched. */
function compile(source) {
  return stripTypeScriptTypes(source, { mode: "strip" })
    .replace(IMPORT, "")
    .replace(BARE_IMPORT, "")
    .replace(EXPORT_KEYWORD, "")
    .replace(/^\s*export\s*\{[^}]*\};?\s*$/gm, "");
}

function names(source) {
  return [...source.matchAll(DECLARATION)].map((match) => match[1]);
}

/**
 * Concatenates the graph reachable from `entry` into one classic script.
 *
 * @param {string} entry absolute path of the entry module
 * @param {(path: string) => string} load reads a module, for tests
 */
export function bundle(entry, load = read) {
  const order = [];
  const done = new Set();
  const visiting = new Set();
  const declared = new Map();

  const visit = (path) => {
    if (done.has(path)) return;
    if (visiting.has(path)) throw new Error(`circular import at ${path}`);
    visiting.add(path);
    const source = load(path);
    for (const dependency of dependencies(source, path)) visit(dependency);
    visiting.delete(path);
    done.add(path);
    const compiled = compile(source);
    for (const name of names(compiled)) {
      const owner = declared.get(name);
      if (owner) throw new Error(`\`${name}\` is declared in ${owner} and in ${path}`);
      declared.set(name, path);
    }
    order.push(compiled.trim());
  };

  visit(entry);
  return `(function () {\n"use strict";\n\n${order.join("\n\n")}\n})();\n`;
}

function main() {
  const report = [];
  for (const { entry, out, budget } of ENTRIES) {
    const code = bundle(resolve(HERE, entry));
    mkdirSync(resolve(HERE, "dist"), { recursive: true });
    writeFileSync(resolve(HERE, "dist", out), code);
    const compressed = gzipSync(Buffer.from(code), { level: 9 }).length;
    if (compressed > budget) {
      console.error(`${out}: ${compressed} bytes compressed, over the ${budget} byte budget`);
      process.exitCode = 1;
    }
    report.push(`${out}: ${code.length} bytes, ${compressed} compressed, budget ${budget}`);
  }
  console.log(report.join("\n"));
}

if (process.argv[1] === fileURLToPath(import.meta.url)) main();
