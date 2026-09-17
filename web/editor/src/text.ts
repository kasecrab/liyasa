// The text arithmetic several modules need, in one place.
//
// It is here rather than repeated because the bundler requires every binding
// to be declared once across the entry graph (RFC 1100), and because the byte
// conversion in particular is the kind of thing that is right in one copy and
// subtly wrong in the next: the API's spans are **byte** offsets into UTF-8 and
// every string the editor holds is UTF-16.

/** Maps a byte offset in `text` to its string index. Builds the map once. */
export function byteIndex(text: string): (offset: number) => number {
  const encoder = new TextEncoder();
  const map = new Map<number, number>();
  let at = 0;
  for (let index = 0; index < text.length; ) {
    map.set(at, index);
    const point = text.codePointAt(index) as number;
    const unit = String.fromCodePoint(point);
    at += encoder.encode(unit).length;
    index += unit.length;
  }
  map.set(at, text.length);
  const total = at;
  return (offset) => map.get(Math.min(Math.max(offset, 0), total)) ?? text.length;
}

/** One byte offset, without building a map. For a short string or a single call. */
export function byteToIndex(text: string, offset: number): number {
  const encoder = new TextEncoder();
  let bytes = 0;
  let index = 0;
  for (const character of text) {
    if (bytes >= offset) break;
    bytes += encoder.encode(character).length;
    index += character.length;
  }
  return index;
}

/** The string index each line of `text` starts at, the first being 0. */
export function lineStarts(text: string): number[] {
  const starts = [0];
  for (let at = 0; at < text.length; at += 1) if (text[at] === "\n") starts.push(at + 1);
  return starts;
}

/** The 0-based line an offset falls on. */
export function lineOf(starts: number[], at: number): number {
  let line = 0;
  while (line + 1 < starts.length && (starts[line + 1] as number) <= at) line += 1;
  return line;
}

/** One line of `text`, without its newline. */
export function lineText(text: string, starts: number[], line: number): string {
  const start = starts[line] as number;
  const end = starts[line + 1] ?? text.length + 1;
  return text.slice(start, end - 1).replace(/\n$/, "");
}

/** Escapes `text` so it matches itself inside a regular expression. */
export function escapeRegExp(text: string): string {
  return text.replace(/[.*+?^${}()|[\]\\]/g, "\\$&");
}

/** A plain object, and not an array or `null`. */
export function isRecord(value: unknown): value is Record<string, unknown> {
  return typeof value === "object" && value !== null && !Array.isArray(value);
}
