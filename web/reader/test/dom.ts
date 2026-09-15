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
