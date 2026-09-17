// A static server for `web/editor/`, for the suites that drive the shell.
//
// The editor is not part of the reference site the workspace's shared
// `webServer` builds, and `web/playwright.config.ts` belongs to every package,
// so this package starts its own rather than adding a second server there.
//
// It is HTTP rather than a `file://` URL on purpose: a file URL is a different
// origin model from the one the editor ships under, so `localStorage`, the
// module graph and every fetch behave differently from production — and a
// suite that passes under `file://` has proved something about `file://`.

import { createServer } from "node:http";
import type { Server } from "node:http";
import { readFile } from "node:fs/promises";
import { dirname, extname, resolve, sep } from "node:path";
import { fileURLToPath } from "node:url";

const HERE = dirname(fileURLToPath(import.meta.url));
const ROOT = resolve(HERE, "../../editor");

const TYPES: Record<string, string> = {
  ".html": "text/html; charset=utf-8",
  ".css": "text/css; charset=utf-8",
  ".js": "text/javascript; charset=utf-8",
  ".json": "application/json; charset=utf-8",
  ".md": "text/markdown; charset=utf-8",
  ".svg": "image/svg+xml",
  ".png": "image/png",
};

export interface EditorServer {
  url: string;
  close(): Promise<void>;
}

/** Serves `web/editor/` on a port the operating system picks. */
export async function startEditorServer(): Promise<EditorServer> {
  const server: Server = createServer((request, response) => {
    const pathname = decodeURIComponent((request.url ?? "/").split("?")[0] ?? "/");
    const target = resolve(ROOT, `.${pathname === "/" ? "/index.html" : pathname}`);
    // A request that climbs out of the root is a 403, not a file.
    if (target !== ROOT && !target.startsWith(ROOT + sep)) {
      response.writeHead(403).end("outside the editor");
      return;
    }
    readFile(target).then(
      (bytes) => {
        response.writeHead(200, {
          "content-type": TYPES[extname(target)] ?? "application/octet-stream",
        });
        response.end(bytes);
      },
      () => response.writeHead(404, { "content-type": "text/plain" }).end("not found"),
    );
  });

  await new Promise<void>((done) => server.listen(0, "127.0.0.1", done));
  const address = server.address();
  if (address === null || typeof address === "string") throw new Error("the editor server has no port");
  return {
    url: `http://127.0.0.1:${address.port}/`,
    close: () => new Promise<void>((done) => server.close(() => done())),
  };
}
