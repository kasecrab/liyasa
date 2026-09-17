// The WebAssembly session, and ED-07's resolve loop.
//
// WP-24a's decision, which this module implements rather than relitigates: the
// module never calls back into JavaScript. A response carries
// `missing: string[]` — paths the draft named that the session does not hold —
// and the host fetches each, hands it over with `seed`, and calls again. A
// synchronous callback from wasm into JS needs `XMLHttpRequest` or
// `Atomics.wait` over a `SharedArrayBuffer`, and neither is something a module
// should require of the page that embeds it.
//
// The loop is bounded twice. A server that keeps answering with a file that
// still leaves the path missing — a redirect loop, a stale cache, a path the
// draft names two ways — would otherwise spin forever on a keystroke.

import type {
  ParseRequest,
  ParseResponse,
  PreviewRequest,
  PreviewResponse,
  SerializeRequest,
  SerializeResponse,
  ValidateRequest,
  ValidateResponse,
} from "../../../crates/liyasa-wasm/ts/liyasa-wasm.d.ts";

/** The part of `liyasa-wasm`'s `Session` this module drives. */
export interface WasmSession {
  seed(path: string, text: string): void;
  parse(request: ParseRequest): ParseResponse;
  preview(request: PreviewRequest): PreviewResponse;
  validate(request: ValidateRequest): ValidateResponse;
  serialize(request: SerializeRequest): SerializeResponse;
}

export interface Resolver {
  /** Fetches one path through `/_liyasa/editor/fs/<path>`, or `null`. */
  read(path: string): Promise<string | null>;
}

/** How many rounds of seeding a single call may take. */
export const MAX_ROUNDS = 8;

export interface Resolved<T> {
  response: T;
  /** Paths the resolver could not supply, so the pane can say which. */
  unresolved: string[];
  rounds: number;
}

interface HasMissing {
  missing: string[];
}

/**
 * Runs `call`, seeding whatever it says is missing, until it stops asking.
 *
 * Two bounds, and both have caught something in a browser at some point in
 * every editor that has this shape:
 *
 *   * `MAX_ROUNDS`, so a server that answers every fetch without resolving the
 *     path cannot spin on a keystroke.
 *   * a path asked for twice is not fetched twice. A resolver that returns
 *     content the session does not accept as that path would otherwise make no
 *     progress while looking like it was.
 */
export async function resolving<T extends HasMissing>(
  session: WasmSession,
  resolver: Resolver,
  call: () => T,
): Promise<Resolved<T>> {
  const asked = new Set<string>();
  const unresolved: string[] = [];

  for (let round = 1; round <= MAX_ROUNDS; round += 1) {
    const response = call();
    const wanted = response.missing.filter((path) => !asked.has(path));
    if (wanted.length === 0) {
      // Whatever is still missing here was asked for and did not resolve —
      // the resolver answered, and the session did not accept the answer as
      // that path. Returning an empty `unresolved` would report success on a
      // page that still cannot render.
      return {
        response,
        unresolved: [...new Set([...unresolved, ...response.missing])],
        rounds: round,
      };
    }
    for (const path of wanted) {
      asked.add(path);
      const text = await resolver.read(path);
      if (text === null) {
        unresolved.push(path);
        continue;
      }
      session.seed(path, text);
    }
  }

  // The bound was reached. The last answer is still the best one available and
  // the caller is told how it got there rather than being handed a silent
  // partial render.
  const response = call();
  return {
    response,
    unresolved: [...new Set([...unresolved, ...response.missing])],
    rounds: MAX_ROUNDS,
  };
}

/**
 * The editor's four calls, each with the resolve loop around it.
 *
 * `serialize` has none: it reads no file the session does not already hold,
 * and its response has no `missing` to answer.
 */
export class EditorSession {
  session: WasmSession;
  resolver: Resolver;

  constructor(session: WasmSession, resolver: Resolver) {
    this.session = session;
    this.resolver = resolver;
  }

  parse(request: ParseRequest): Promise<Resolved<ParseResponse>> {
    return resolving(this.session, this.resolver, () => this.session.parse(request));
  }

  preview(request: PreviewRequest): Promise<Resolved<PreviewResponse>> {
    return resolving(this.session, this.resolver, () => this.session.preview(request));
  }

  validate(request: ValidateRequest): ValidateResponse {
    return this.session.validate(request);
  }

  serialize(request: SerializeRequest): SerializeResponse {
    return this.session.serialize(request);
  }
}

/**
 * A 32-character hexadecimal session nonce.
 *
 * WP-24a makes this required and gives it no default: it is what makes a
 * directive marker unforgeable, and a predictable nonce lets an author type a
 * marker into a page and have the preview parse it as a component nobody
 * declared. Generated here from the platform's CSPRNG, per session.
 */
export function sessionNonce(): string {
  const bytes = new Uint8Array(16);
  globalThis.crypto.getRandomValues(bytes);
  return [...bytes].map((byte) => byte.toString(16).padStart(2, "0")).join("");
}

/**
 * The editor holds the last good HTML while diagnostics are non-empty.
 *
 * WP-24a's note says so explicitly: a preview expands at `Undefined::Strict`,
 * so a half-typed `{{ ` blanks the render and returns `E0201`, deliberately.
 * Showing that blank would make the preview flash empty on every keystroke
 * inside an expression. Holding the last good render is the editor's job, and
 * this is where it is done.
 */
export class PreviewHold {
  html: string;
  stale: boolean;

  constructor() {
    this.html = "";
    this.stale = false;
  }

  update(response: PreviewResponse): { html: string; stale: boolean } {
    const failed = response.diagnostics.some((diagnostic) => diagnostic.severity === "error");
    if (!failed) {
      this.html = response.html;
      this.stale = false;
    } else {
      this.stale = this.html !== "";
    }
    return { html: this.html, stale: this.stale };
  }
}
