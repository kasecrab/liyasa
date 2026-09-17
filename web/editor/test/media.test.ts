// ED-13: the media library's own decisions — what may be uploaded, what an
// asset's Markdown looks like, what a delete says, and how a light and dark
// pair is offered.

import test from "node:test";
import assert from "node:assert/strict";

import {
  darkPartner,
  deleteRefusal,
  imageDirective,
  searchAssets,
  sniff,
  transformQuery,
  validateUpload,
} from "../src/media.ts";

const PNG = new Uint8Array([0x89, 0x50, 0x4e, 0x47, 0x0d, 0x0a, 0x1a, 0x0a, 0, 0, 0, 0]);
const JPEG = new Uint8Array([0xff, 0xd8, 0xff, 0xe0, 0, 0, 0, 0]);
const SVG = new TextEncoder().encode('<svg xmlns="http://www.w3.org/2000/svg"></svg>');
const PDF = new TextEncoder().encode("%PDF-1.7\n");

test("the type is sniffed from the bytes, not taken from the name", () => {
  // An upload named `.png` that is really an SVG is how a script gets served
  // inline from the docs origin.
  assert.equal(sniff(PNG)?.extension, "png");
  assert.equal(sniff(JPEG)?.extension, "jpeg");
  assert.equal(sniff(SVG)?.extension, "svg");
  assert.equal(sniff(PDF)?.extension, "pdf");
  assert.equal(sniff(new Uint8Array([1, 2, 3, 4])), null);
});

test("an image without alt text is refused before it is sent", () => {
  const found = validateUpload({ filename: "a.png", bytes: PNG, alt: "" });
  assert.equal(found[0]?.code, "E0305");
  assert.match(found[0]?.message ?? "", /alt text/);
});

test("alt text of only spaces is not alt text", () => {
  assert.equal(validateUpload({ filename: "a.png", bytes: PNG, alt: "   " })[0]?.code, "E0305");
});

test("a deliberate empty alt is allowed when the author marks the image decorative", () => {
  // The media guide names this: `alt=""` tells assistive technology to skip
  // the image, and it is a choice rather than a shortcut, so it has to be made
  // explicitly.
  assert.deepEqual(validateUpload({ filename: "a.png", bytes: PNG, alt: "", decorative: true }), []);
});

test("a type the project does not allow is refused with the key that would allow it", () => {
  const found = validateUpload({ filename: "a.pdf", bytes: PDF, alt: "x", allowTypes: ["png", "jpeg"] });
  assert.equal(found[0]?.code, "E0812");
  assert.match(found[0]?.message ?? "", /security\.uploads\.allowTypes/);
});

test("an unrecognised file is refused rather than uploaded as something", () => {
  const found = validateUpload({ filename: "a.bin", bytes: new Uint8Array([1, 2, 3]), alt: "x" });
  assert.equal(found[0]?.code, "E0812");
});

test("a valid upload reports nothing, and says nothing about what the server strips", () => {
  // The browser cannot strip EXIF and does not; `liyasa_build::media::accept`
  // does, on the server. An editor that reported "metadata removed" here would
  // be claiming something it never did.
  assert.deepEqual(validateUpload({ filename: "a.jpg", bytes: JPEG, alt: "A photo" }), []);
});

test("an image's Markdown is the directive the build parses", () => {
  assert.equal(
    imageDirective({
      src: "/assets/uploads/abc.png",
      alt: 'The "deployment" list',
      width: 1280,
      height: 720,
      caption: "Newest first",
      dark: "/assets/uploads/abc-dark.png",
    }),
    '::image{src="/assets/uploads/abc.png" alt="The \\"deployment\\" list" dark="/assets/uploads/abc-dark.png" ' +
      'width=1280 height=720 caption="Newest first"}',
  );
});

test("an image with only the two required props writes only those", () => {
  assert.equal(
    imageDirective({ src: "/assets/a.png", alt: "A picture" }),
    '::image{src="/assets/a.png" alt="A picture"}',
  );
});

test("a decorative image writes an empty alt rather than omitting it", () => {
  assert.equal(imageDirective({ src: "/assets/a.png", alt: "" }), '::image{src="/assets/a.png" alt=""}');
});

test("a dark partner is offered by name, and only when the file is really there", () => {
  const assets = ["/assets/a.png", "/assets/a-dark.png", "/assets/b.png"];
  assert.equal(darkPartner("/assets/a.png", assets), "/assets/a-dark.png");
  assert.equal(darkPartner("/assets/b.png", assets), null);
  // The pairing is one way: a dark file is not its own light partner.
  assert.equal(darkPartner("/assets/a-dark.png", assets), null);
});

test("search matches the file name, the alt text and the page that uses it", () => {
  const assets = [
    { path: "/assets/dashboard.png", alt: "The deployment list", usedOn: ["/guides/deploy"] },
    { path: "/assets/logo.svg", alt: "The Acme mark", usedOn: [] },
  ];
  assert.deepEqual(searchAssets(assets, "dash").map((asset) => asset.path), ["/assets/dashboard.png"]);
  assert.deepEqual(searchAssets(assets, "deployment").map((asset) => asset.path), ["/assets/dashboard.png"]);
  assert.deepEqual(searchAssets(assets, "/guides/").map((asset) => asset.path), ["/assets/dashboard.png"]);
  assert.deepEqual(searchAssets(assets, "acme").map((asset) => asset.path), ["/assets/logo.svg"]);
  assert.equal(searchAssets(assets, "").length, 2);
});

test("a delete refusal names the pages, not only the count", () => {
  // "used on 4 pages" leaves the author hunting. The list is what they need.
  const refusal = deleteRefusal({
    path: "/assets/dashboard.png",
    alt: "x",
    usedOn: ["/guides/deploy", "/index"],
  });
  assert.equal(refusal?.code, "E0703");
  assert.match(refusal?.message ?? "", /\/guides\/deploy/);
  assert.match(refusal?.message ?? "", /\/index/);
});

test("an unused asset has no refusal", () => {
  assert.equal(deleteRefusal({ path: "/assets/a.png", alt: "x", usedOn: [] }), null);
});

test("a crop and resize become the query the image endpoint answers", () => {
  assert.equal(
    transformQuery({ crop: { x: 10, y: 20, width: 300, height: 200 }, resize: { width: 800 } }),
    "crop=10,20,300,200&w=800",
  );
  assert.equal(transformQuery({ resize: { width: 400, height: 300 } }), "w=400&h=300");
  assert.equal(transformQuery({}), "");
});

test("a crop with no area is refused rather than sent as a zero-pixel request", () => {
  assert.throws(() => transformQuery({ crop: { x: 0, y: 0, width: 0, height: 10 } }), /width and height/);
});
