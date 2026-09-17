// Builds the editor without installing anything.
//
// The bundler is `web/reader/build.mjs`'s `bundle`, for the reason RFC 1100
// gives and `web/dashboard/build.mjs` repeats: PRD §6.2 names Vite 8 for
// `web/` and `npm install` needs a registry this machine does not always have.
// RFC 2430 records what that costs this package specifically — ProseMirror and
// CodeMirror are non-relative imports, which this bundler rejects by
// construction, so the editor is framework-free in the shape they would have
// imposed.
//
// The output is one classic script. The editor is an application rather than
// progressive enhancement, so unlike the reader it may assume it runs;
// `index.html` still says so rather than rendering an empty frame.

import { mkdirSync, readFileSync, writeFileSync } from "node:fs";
import { dirname, resolve } from "node:path";
import { fileURLToPath } from "node:url";
import { gzipSync } from "node:zlib";

import { bundle } from "../reader/build.mjs";

const HERE = dirname(fileURLToPath(import.meta.url));

// The budget is this package's own and is not one of NFR-05's: the WebAssembly
// module is what ED-06 measures, and it is three orders of magnitude larger.
// What this number buys is a bundle that a reviewer can still read, and a
// failure the moment the editor grows a dependency-shaped lump.
/** Entry, output name, and the budget the compressed bundle must fit. */
export const ENTRIES = [{ entry: "src/editor.ts", out: "editor.js", budget: 96 * 1024 }];

export function buildEditor() {
  const report = [];
  for (const { entry, out, budget } of ENTRIES) {
    const code = bundle(resolve(HERE, entry), (path) => readFileSync(path, "utf8"));
    mkdirSync(resolve(HERE, "dist"), { recursive: true });
    writeFileSync(resolve(HERE, "dist", out), code);
    const compressed = gzipSync(Buffer.from(code), { level: 9 }).length;
    if (compressed > budget) {
      console.error(`${out}: ${compressed} bytes compressed, over the ${budget} byte budget`);
      process.exitCode = 1;
    }
    report.push({ out, bytes: code.length, compressed, budget });
  }
  return report;
}

if (process.argv[1] === fileURLToPath(import.meta.url)) {
  for (const row of buildEditor()) {
    console.log(`${row.out}: ${row.bytes} bytes, ${row.compressed} compressed, budget ${row.budget}`);
  }
}
