// Serves the generated reference site for the e2e suite.
//
// No dependencies, and no behaviour beyond what a static host has: a route is
// a directory with an `index.html`, `<route>.md` is the Markdown twin, and
// anything unknown is a 404 with a body. It exists because the suite needs
// something to point a browser at; `liyasa serve` replaces it once there is a
// CLI to run.

import { createServer } from "node:http";
import { readFile, stat } from "node:fs/promises";
import { join, extname, resolve, sep } from "node:path";
import { fileURLToPath } from "node:url";

const TYPES = {
  ".html": "text/html; charset=utf-8",
  ".md": "text/markdown; charset=utf-8",
  ".css": "text/css; charset=utf-8",
  ".js": "text/javascript; charset=utf-8",
  ".json": "application/json; charset=utf-8",
  ".svg": "image/svg+xml",
  ".woff2": "font/woff2",
  ".png": "image/png",
  ".txt": "text/plain; charset=utf-8",
};

/** The file a request path names, or `null` when it escapes the root. */
export async function resolveFile(root, pathname) {
  const decoded = decodeURIComponent(pathname.split("?")[0] ?? "/");
  const target = resolve(root, `.${decoded}`);
  if (target !== root && !target.startsWith(root + sep)) return null;

  const candidates = extname(target) ? [target] : [join(target, "index.html")];
  for (const candidate of candidates) {
    try {
      const found = await stat(candidate);
      if (found.isFile()) return candidate;
    } catch {
      // Next candidate, then the 404 below.
    }
  }
  return null;
}

export function reader(root) {
  const base = resolve(root);
  return async (request, response) => {
    const file = await resolveFile(base, request.url ?? "/");
    if (file === null) {
      response.writeHead(404, { "content-type": TYPES[".html"] });
      response.end("<!doctype html><title>Not found</title><h1>Not found</h1>");
      return;
    }
    const body = await readFile(file);
    response.writeHead(200, {
      "content-type": TYPES[extname(file)] ?? "application/octet-stream",
      "content-length": body.length,
      "cache-control": "no-store",
    });
    response.end(request.method === "HEAD" ? undefined : body);
  };
}

export function serve(root, port = 0) {
  return createServer(reader(root)).listen(port);
}

if (process.argv[1] === fileURLToPath(import.meta.url)) {
  const root = process.env["LIYASA_SITE"] ?? "../../target/reference-site";
  const port = Number(process.env["PORT"] ?? 4173);
  serve(root, port).on("listening", () => {
    console.log(`reference site on http://127.0.0.1:${port} from ${resolve(root)}`);
  });
}
