// ED-13: the media library.
//
// What is here is what the browser can decide: the type an upload really is,
// whether it carries alt text, what Markdown an asset becomes, which pages use
// it, and what a crop asks the image endpoint for.
//
// What is **not** here is metadata stripping and SVG checking. Those are
// `liyasa_build::media::accept`'s, on the server, and this module says nothing
// about them — an editor that reported "metadata removed" beside an upload it
// only forwarded would be claiming something it never did. The server's
// refusal comes back as a `Diagnostic` and is shown as it arrives.

import type { Diagnostic } from "../../../crates/liyasa-wasm/ts/liyasa-wasm.d.ts";

export interface Asset {
  path: string;
  alt: string;
  usedOn: string[];
  dark?: string;
  bytes?: number;
}

export interface SniffedKind {
  extension: string;
  contentType: string;
  isImage: boolean;
}

/** Magic numbers, mirroring `liyasa_build::media::sniff`. */
const SIGNATURES: { bytes: number[]; kind: SniffedKind }[] = [
  { bytes: [0x89, 0x50, 0x4e, 0x47, 0x0d, 0x0a, 0x1a, 0x0a], kind: img("png", "image/png") },
  { bytes: [0xff, 0xd8, 0xff], kind: img("jpeg", "image/jpeg") },
  { bytes: [0x47, 0x49, 0x46, 0x38], kind: img("gif", "image/gif") },
  { bytes: [0x25, 0x50, 0x44, 0x46], kind: { extension: "pdf", contentType: "application/pdf", isImage: false } },
];

function img(extension: string, contentType: string): SniffedKind {
  return { extension, contentType, isImage: true };
}

/**
 * What the bytes are, whatever the file is called.
 *
 * An upload named `.png` that is really an SVG is how a script gets served
 * inline from the documentation origin, so the name is never consulted.
 */
export function sniff(bytes: Uint8Array): SniffedKind | null {
  for (const { bytes: signature, kind } of SIGNATURES) {
    if (signature.every((byte, at) => bytes[at] === byte)) return kind;
  }
  // RIFF....WEBP
  if (starts(bytes, [0x52, 0x49, 0x46, 0x46]) && starts(bytes.subarray(8), [0x57, 0x45, 0x42, 0x50])) {
    return img("webp", "image/webp");
  }
  // ....ftypavif
  if (bytes.length > 12 && starts(bytes.subarray(4), [0x66, 0x74, 0x79, 0x70, 0x61, 0x76, 0x69, 0x66])) {
    return img("avif", "image/avif");
  }
  const head = new TextDecoder().decode(bytes.subarray(0, 512)).trimStart();
  if (head.startsWith("<svg") || (head.startsWith("<?xml") && head.includes("<svg"))) {
    return img("svg", "image/svg+xml");
  }
  return null;
}

function starts(bytes: Uint8Array, signature: number[]): boolean {
  return signature.every((byte, at) => bytes[at] === byte);
}

export interface UploadCandidate {
  filename: string;
  bytes: Uint8Array;
  alt: string;
  /**
   * The author has said this image carries no information. It is a choice
   * rather than a way past the alt text rule, which is why it is separate
   * from an empty `alt`.
   */
  decorative?: boolean;
  allowTypes?: string[];
}

const DEFAULT_ALLOW_TYPES = ["png", "jpeg", "gif", "webp", "avif", "svg", "pdf", "mp4", "webm", "mp3"];

/** What the browser can refuse before spending an upload on it. */
export function validateUpload(candidate: UploadCandidate): Diagnostic[] {
  const kind = sniff(candidate.bytes);
  if (!kind) {
    return [
      diagnostic(
        "E0812",
        `\`${candidate.filename}\` is not a type Liyasa recognizes. The content is read, not the file name.`,
      ),
    ];
  }
  const allowed = candidate.allowTypes ?? DEFAULT_ALLOW_TYPES;
  if (!allowed.includes(kind.extension)) {
    return [
      diagnostic(
        "E0812",
        `\`${candidate.filename}\` is ${kind.extension}, which \`security.uploads.allowTypes\` does not list.`,
      ),
    ];
  }
  if (kind.isImage && candidate.decorative !== true && candidate.alt.trim() === "") {
    return [
      diagnostic(
        "E0305",
        `\`${candidate.filename}\` needs alt text: what the image says, for a reader who cannot see it. ` +
          `Mark it decorative if it says nothing.`,
      ),
    ];
  }
  return [];
}

export interface ImageProps {
  src: string;
  alt: string;
  dark?: string;
  width?: number;
  height?: number;
  caption?: string;
}

/**
 * The `::image` directive an asset becomes.
 *
 * `alt` is always written, even when it is empty: an omitted `alt` is an
 * image nobody decided about, and `alt=""` is one somebody marked decorative.
 * The two mean different things to a screen reader and to `E0305`.
 */
export function imageDirective(props: ImageProps): string {
  const parts = [`src="${quote(props.src)}"`, `alt="${quote(props.alt)}"`];
  if (props.dark) parts.push(`dark="${quote(props.dark)}"`);
  if (props.width !== undefined) parts.push(`width=${props.width}`);
  if (props.height !== undefined) parts.push(`height=${props.height}`);
  if (props.caption) parts.push(`caption="${quote(props.caption)}"`);
  return `::image{${parts.join(" ")}}`;
}

function quote(value: string): string {
  return value.replace(/\\/g, "\\\\").replace(/"/g, '\\"');
}

/**
 * The dark-mode partner of a light asset, when the project has one.
 *
 * The convention is a `-dark` suffix before the extension. The file has to
 * exist: offering a `dark` prop pointing at nothing is the editor writing a
 * broken reference on the author's behalf.
 */
export function darkPartner(path: string, assets: string[]): string | null {
  if (/-dark\.[^.]+$/.test(path)) return null;
  const candidate = path.replace(/(\.[^.]+)$/, "-dark$1");
  return assets.includes(candidate) ? candidate : null;
}

/** The library's search: file name, alt text, and the pages that use it. */
export function searchAssets<T extends Asset>(assets: T[], query: string): T[] {
  const needle = query.trim().toLowerCase();
  if (needle === "") return [...assets];
  return assets.filter((asset) =>
    [asset.path, asset.alt, ...asset.usedOn].some((field) => field.toLowerCase().includes(needle)),
  );
}

/**
 * Why an asset cannot be deleted, or `null`.
 *
 * The message names the pages. "Used on 4 pages" leaves the author hunting for
 * four pages, which is the work the editor is supposed to have done.
 */
export function deleteRefusal(asset: Asset): Diagnostic | null {
  if (asset.usedOn.length === 0) return null;
  return diagnostic(
    "E0703",
    `\`${asset.path}\` is used on ${asset.usedOn.length} page${asset.usedOn.length === 1 ? "" : "s"} ` +
      `and cannot be deleted: ${asset.usedOn.join(", ")}. Remove the references first, or replace the file.`,
  );
}

export interface Transform {
  crop?: { x: number; y: number; width: number; height: number };
  resize?: { width?: number; height?: number };
}

/** The query string the image endpoint answers for a crop and resize. */
export function transformQuery(transform: Transform): string {
  const parts: string[] = [];
  if (transform.crop) {
    const { x, y, width, height } = transform.crop;
    if (width <= 0 || height <= 0) {
      throw new Error("a crop needs a width and height above zero");
    }
    parts.push(`crop=${x},${y},${width},${height}`);
  }
  if (transform.resize?.width !== undefined) parts.push(`w=${transform.resize.width}`);
  if (transform.resize?.height !== undefined) parts.push(`h=${transform.resize.height}`);
  return parts.join("&");
}

function diagnostic(code: string, message: string): Diagnostic {
  return {
    code,
    severity: "error",
    message,
    url: `https://kasecrab.github.io/liyasa/docs/errors/${code}`,
  };
}
