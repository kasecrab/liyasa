// A scripted stand-in for the parts of a browser the runtime touches.
//
// Small on purpose: a module that needs more of a browser than this belongs in
// the e2e suite, where there is a real one.

export interface FakeSheet {
  text: string;
  replaceSync(text: string): void;
}

export interface FakeMedia {
  readonly query: string;
  matches: boolean;
  listeners: Array<() => void>;
  addEventListener(type: string, listener: () => void): void;
  /** Flips the query and notifies, the way a reader changing a setting does. */
  set(matches: boolean): void;
}

export interface FakeDocument {
  adoptedStyleSheets: FakeSheet[];
  startViewTransition?: () => void;
  listeners: Record<string, Array<(event: unknown) => void>>;
  addEventListener(type: string, listener: (event: unknown) => void): void;
  dispatch(type: string, event?: unknown): void;
}

export interface FakeWindow {
  document: FakeDocument;
  CSSStyleSheet?: new () => FakeSheet;
  CSSViewTransitionRule?: unknown;
  PerformanceObserver?: FakeObserverConstructor;
  media: Map<string, FakeMedia>;
  matchMedia?(query: string): FakeMedia;
  listeners: Record<string, Array<(event: unknown) => void>>;
  addEventListener(type: string, listener: (event: unknown) => void): void;
  dispatch(type: string, event?: unknown): void;
}

export interface FakeObserver {
  type: string;
  options: Record<string, unknown>;
  /** Hands the observer a batch, as the browser does. */
  emit(entries: unknown[]): void;
  disconnect(): void;
}

export interface FakeObserverConstructor {
  new (callback: (list: { getEntries(): unknown[] }) => void): {
    observe(options: Record<string, unknown>): void;
    disconnect(): void;
  };
  /** Every observer the code under test created. */
  readonly created: FakeObserver[];
  readonly supportedEntryTypes: string[];
}

function bus(): {
  listeners: Record<string, Array<(event: unknown) => void>>;
  addEventListener(type: string, listener: (event: unknown) => void): void;
  dispatch(type: string, event?: unknown): void;
} {
  const listeners: Record<string, Array<(event: unknown) => void>> = {};
  return {
    listeners,
    addEventListener(type, listener) {
      (listeners[type] ||= []).push(listener);
    },
    dispatch(type, event) {
      for (const listener of listeners[type] ?? []) listener(event ?? {});
    },
  };
}

export interface FakeOptions {
  /** `false` removes constructable stylesheets, as an old browser would. */
  constructableStyleSheets?: boolean;
  /** `false` removes every view-transition signal. */
  viewTransitions?: boolean;
  reducedMotion?: boolean;
  /** Entry types the fake `PerformanceObserver` reports as supported. */
  entryTypes?: string[];
}

export function fakeWindow(options: FakeOptions = {}): FakeWindow {
  const {
    constructableStyleSheets = true,
    viewTransitions = true,
    reducedMotion = false,
    entryTypes = ["largest-contentful-paint", "layout-shift", "event"],
  } = options;

  const documentBus = bus();
  const document: FakeDocument = {
    adoptedStyleSheets: [],
    ...documentBus,
  };
  if (viewTransitions) document.startViewTransition = () => {};

  const media = new Map<string, FakeMedia>();
  const windowBus = bus();
  const win: FakeWindow = {
    document,
    media,
    ...windowBus,
    matchMedia(query: string): FakeMedia {
      const existing = media.get(query);
      if (existing) return existing;
      const entry: FakeMedia = {
        query,
        matches: query.includes("reduce") ? reducedMotion : false,
        listeners: [],
        addEventListener(_type: string, listener: () => void) {
          entry.listeners.push(listener);
        },
        set(matches: boolean) {
          entry.matches = matches;
          for (const listener of entry.listeners) listener();
        },
      };
      media.set(query, entry);
      return entry;
    },
  };

  if (constructableStyleSheets) {
    win.CSSStyleSheet = class implements FakeSheet {
      text = "";
      replaceSync(text: string) {
        this.text = text;
      }
    };
  }
  if (viewTransitions) win.CSSViewTransitionRule = function CSSViewTransitionRule() {};

  win.PerformanceObserver = fakeObservers(entryTypes);
  return win;
}

function fakeObservers(entryTypes: string[]): FakeObserverConstructor {
  const created: FakeObserver[] = [];
  class Observer {
    callback: (list: { getEntries(): unknown[] }) => void;
    constructor(callback: (list: { getEntries(): unknown[] }) => void) {
      this.callback = callback;
    }
    observe(observed: Record<string, unknown>) {
      const record: FakeObserver = {
        type: String(observed["type"] ?? ""),
        options: observed,
        emit: (entries: unknown[]) => this.callback({ getEntries: () => entries }),
        disconnect: () => {},
      };
      created.push(record);
    }
    disconnect() {}
  }
  return Object.assign(Observer, { created, supportedEntryTypes: entryTypes }) as unknown as FakeObserverConstructor;
}

// --- the vendored modules -------------------------------------------------
//
// `crates/liyasa-theme/assets/js/` ships the rest of the runtime as classic
// scripts (RFC 1100). RX-04's prefetch half is one of them, and it is this
// package's requirement, so it is driven here rather than only in the e2e
// suite: the module is loaded into a scripted document and asked to behave.

export interface FakeElement {
  tag: string;
  attributes: Record<string, string>;
  href: string;
  origin: string;
  hidden: boolean;
  setAttribute(name: string, value: string): void;
  getAttribute(name: string): string | null;
  hasAttribute(name: string): boolean;
  closest(selector: string): FakeElement | null;
  addEventListener(type: string, listener: (event: unknown) => void): void;
  listeners: Record<string, Array<(event: unknown) => void>>;
  children: FakeElement[];
  appendChild(child: FakeElement): FakeElement;
  matches: string[];
}

function rel(node: FakeElement): string {
  return (node as unknown as { rel?: string }).rel ?? node.getAttribute("rel") ?? "";
}

export function element(tag: string, attributes: Record<string, string> = {}): FakeElement {
  const node: FakeElement = {
    tag,
    attributes: { ...attributes },
    href: attributes["href"] ?? "",
    origin: attributes["origin"] ?? "https://docs.example",
    hidden: false,
    children: [],
    listeners: {},
    matches: [],
    setAttribute(name, value) {
      node.attributes[name] = value;
      if (name === "href") node.href = value;
    },
    getAttribute: (name) => node.attributes[name] ?? null,
    hasAttribute: (name) => name in node.attributes,
    closest: (selector) => (node.matches.includes(selector) || node.tag === "a" ? node : null),
    addEventListener(type, listener) {
      (node.listeners[type] ||= []).push(listener);
    },
    appendChild(child) {
      node.children.push(child);
      return child;
    },
  };
  return node;
}

export interface VendoredWindow extends FakeWindow {
  head: FakeElement;
  /** Every `<link rel=prefetch>` the module added, in order. */
  prefetched(): string[];
  query: Map<string, FakeElement[]>;
  observers: Array<{ callback: (entries: unknown[]) => void; observed: FakeElement[] }>;
}

export interface VendoredOptions extends FakeOptions {
  saveData?: boolean;
  reducedData?: boolean;
  origin?: string;
}

/** A document the vendored classic scripts can run against. */
export function vendoredWindow(options: VendoredOptions = {}): VendoredWindow {
  const { saveData = false, reducedData = false, origin = "https://docs.example" } = options;
  const base = fakeWindow(options);
  const head = element("head");
  const query = new Map<string, FakeElement[]>();
  const observers: VendoredWindow["observers"] = [];

  const win = base as VendoredWindow;
  win.head = head;
  win.query = query;
  win.observers = observers;
  // A module sets `link.rel` and `link.href` as properties rather than through
  // `setAttribute`, the way the DOM allows, so the property is what is read.
  win.prefetched = () =>
    head.children
      .filter((child) => child.tag === "link" && rel(child) === "prefetch")
      .map((child) => child.href);

  const media = win.matchMedia;
  win.matchMedia = (queried: string): FakeMedia => {
    const entry = media?.call(win, queried) as FakeMedia;
    if (queried.includes("prefers-reduced-data")) entry.matches = reducedData;
    return entry;
  };

  Object.assign(win.document, {
    head,
    createElement: (tag: string) => element(tag),
    querySelectorAll: (selector: string) => query.get(selector) ?? [],
    querySelector: (selector: string) => query.get(selector)?.[0] ?? null,
  });

  Object.assign(win, {
    location: { origin, href: `${origin}/guide/install` },
    navigator: { connection: { saveData } },
    IntersectionObserver: class {
      constructor(callback: (entries: unknown[]) => void) {
        observers.push({ callback, observed: [] });
      }
      observe(target: FakeElement) {
        observers[observers.length - 1]?.observed.push(target);
      }
      unobserve() {}
    },
  });
  return win;
}
