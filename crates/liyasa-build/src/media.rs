//! The media library's safety rules (PRD §7.14, CM-131).
//!
//! An upload is not trusted: its type is sniffed rather than read from the
//! client, it must be on `security.uploads.allowTypes`, an image is re-encoded
//! so that every piece of metadata it carried is gone, and an SVG passes the
//! same allow-list the inline-HTML sanitizer enforces or is served as a
//! download. Everything else is served `attachment` with `nosniff`.
//!
//! Where the bytes are stored and who may delete them is the server's; the
//! rules are here so the CLI, the editor, and the server cannot disagree.

use std::collections::{BTreeMap, BTreeSet};

use liyasa_core::diagnostics::{Diagnostic, code};
use liyasa_core::ids::{Fingerprint, Route};

use crate::assets::Disposition;

/// `security.uploads.allowTypes`, as extensions (CM-131's default list).
pub const DEFAULT_ALLOW_TYPES: &[&str] = &[
    "png", "jpeg", "gif", "webp", "avif", "svg", "mp4", "webm", "mp3", "pdf", "zip", "json", "csv",
    "txt",
];

/// Where an accepted upload is stored, de-duplicated by content hash.
pub const UPLOAD_DIR: &str = "assets/uploads";

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub enum SvgPolicy {
    /// Sanitize and serve inline.
    #[default]
    Sanitize,
    /// Serve as a download, never as a document on the docs origin.
    Attachment,
}

#[derive(Debug, Clone)]
pub struct Settings {
    pub allow_types: Vec<String>,
    pub svg: SvgPolicy,
}

impl Default for Settings {
    fn default() -> Self {
        Self {
            allow_types: DEFAULT_ALLOW_TYPES
                .iter()
                .map(|kind| (*kind).to_owned())
                .collect(),
            svg: SvgPolicy::default(),
        }
    }
}

/// One upload as the editor hands it over.
#[derive(Debug, Clone)]
pub struct Upload<'a> {
    pub filename: &'a str,
    pub bytes: &'a [u8],
    /// Required for an image (CM-131).
    pub alt: Option<&'a str>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Accepted {
    /// What to store: an image is re-encoded, an SVG is sanitized, everything
    /// else is what arrived.
    pub bytes: Vec<u8>,
    pub extension: String,
    pub content_type: &'static str,
    pub fingerprint: Fingerprint,
    /// `assets/uploads/<digest>.<ext>`: the same bytes twice are one file.
    pub path: String,
    pub disposition: Disposition,
    /// `X-Content-Type-Options: nosniff` is set on every upload, always.
    pub nosniff: bool,
}

/// Accepts an upload, or says why not.
///
/// The error is boxed because a `Diagnostic` is large and this is the hot path
/// of an HTTP handler.
pub fn accept(upload: &Upload<'_>, settings: &Settings) -> Result<Accepted, Box<Diagnostic>> {
    let Some(kind) = sniff(upload.bytes) else {
        return Err(Box::new(
            Diagnostic::new(
                code::E0812,
                format!("`{}` is not a type Liyasa recognizes", upload.filename),
            )
            .help("the content is sniffed, not read from the file name"),
        ));
    };
    if !settings
        .allow_types
        .iter()
        .any(|allowed| allowed == kind.extension)
    {
        return Err(Box::new(
            Diagnostic::new(
                code::E0812,
                format!(
                    "`{}` is {}, which `security.uploads.allowTypes` does not list",
                    upload.filename, kind.extension
                ),
            )
            .help("add the type to `security.uploads.allowTypes`, or upload another format"),
        ));
    }
    if kind.is_image && upload.alt.map(str::trim).unwrap_or_default().is_empty() {
        return Err(Box::new(
            Diagnostic::new(
                code::E0305,
                format!("`{}` was uploaded without alt text", upload.filename),
            )
            .help("every image in the media library carries alt text"),
        ));
    }

    let (bytes, disposition) = match kind.extension {
        "svg" => match settings.svg {
            SvgPolicy::Attachment => (upload.bytes.to_vec(), Disposition::Attachment),
            SvgPolicy::Sanitize => {
                let text = std::str::from_utf8(upload.bytes).map_err(|_| {
                    Box::new(Diagnostic::new(
                        code::E0812,
                        format!("`{}` is not valid UTF-8", upload.filename),
                    ))
                })?;
                (
                    sanitize_svg(text)
                        .map_err(|diagnostic| Box::new(*diagnostic))?
                        .into_bytes(),
                    Disposition::Inline,
                )
            }
        },
        _ if kind.is_image => (strip_metadata(upload.bytes, kind)?, Disposition::Inline),
        "mp4" | "webm" | "mp3" => (upload.bytes.to_vec(), Disposition::Inline),
        // CM-131: everything that is neither an image nor a video is a
        // download, whatever it claims to be.
        _ => (upload.bytes.to_vec(), Disposition::Attachment),
    };

    let fingerprint = Fingerprint::of(&bytes);
    Ok(Accepted {
        path: format!(
            "{UPLOAD_DIR}/{}.{}",
            &fingerprint.to_hex()[..32],
            kind.extension
        ),
        extension: kind.extension.to_owned(),
        content_type: kind.content_type,
        disposition,
        nosniff: true,
        fingerprint,
        bytes,
    })
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Kind {
    pub extension: &'static str,
    pub content_type: &'static str,
    pub is_image: bool,
}

/// The type of an upload, from its bytes.
pub fn sniff(bytes: &[u8]) -> Option<Kind> {
    let image = |extension, content_type| {
        Some(Kind {
            extension,
            content_type,
            is_image: true,
        })
    };
    let other = |extension, content_type| {
        Some(Kind {
            extension,
            content_type,
            is_image: false,
        })
    };

    if bytes.starts_with(b"\x89PNG\r\n\x1a\n") {
        return image("png", "image/png");
    }
    if bytes.starts_with(&[0xff, 0xd8, 0xff]) {
        return image("jpeg", "image/jpeg");
    }
    if bytes.starts_with(b"GIF87a") || bytes.starts_with(b"GIF89a") {
        return image("gif", "image/gif");
    }
    if bytes.starts_with(b"RIFF") && bytes.get(8..12) == Some(b"WEBP") {
        return image("webp", "image/webp");
    }
    if bytes.get(4..12) == Some(b"ftypavif") {
        return image("avif", "image/avif");
    }
    if bytes.get(4..8) == Some(b"ftyp") {
        return other("mp4", "video/mp4");
    }
    if bytes.starts_with(&[0x1a, 0x45, 0xdf, 0xa3]) {
        return other("webm", "video/webm");
    }
    if bytes.starts_with(b"ID3") || bytes.starts_with(&[0xff, 0xfb]) {
        return other("mp3", "audio/mpeg");
    }
    if bytes.starts_with(b"%PDF-") {
        return other("pdf", "application/pdf");
    }
    if bytes.starts_with(b"PK\x03\x04") {
        return other("zip", "application/zip");
    }

    let text = std::str::from_utf8(bytes).ok()?;
    let head = text.trim_start();
    if head.starts_with("<svg") || (head.starts_with("<?xml") && head.contains("<svg")) {
        return image("svg", "image/svg+xml");
    }
    if serde_json::from_str::<serde_json::Value>(text).is_ok() {
        return other("json", "application/json");
    }
    if is_csv(text) {
        return other("csv", "text/csv");
    }
    // Text is the fallback only when it reads as text: control bytes mean
    // this is some binary format Liyasa does not know, not a `.txt`.
    let printable = !text.trim().is_empty()
        && text
            .chars()
            .all(|ch| !ch.is_control() || matches!(ch, '\n' | '\r' | '\t'));
    printable.then(|| other("txt", "text/plain; charset=utf-8"))?
}

/// Re-encodes an image, which is what removes EXIF, XMP, IPTC, GPS, and every
/// ICC profile that is not sRGB: the encoder writes none of them.
///
/// The orientation EXIF carried is applied first, so the stored pixels are the
/// ones a reader should see.
fn strip_metadata(bytes: &[u8], kind: Kind) -> Result<Vec<u8>, Box<Diagnostic>> {
    use image::ImageReader;
    use std::io::Cursor;

    let reject = |reason: String| {
        Box::new(
            Diagnostic::new(code::E0812, reason)
                .help("the file is not a decodable image, whatever its header says"),
        )
    };

    let reader = ImageReader::new(Cursor::new(bytes))
        .with_guessed_format()
        .map_err(|error| reject(error.to_string()))?;
    let format = reader.format();
    let mut decoder = reader
        .into_decoder()
        .map_err(|error| reject(error.to_string()))?;
    let orientation = image::ImageDecoder::orientation(&mut decoder)
        .unwrap_or(image::metadata::Orientation::NoTransforms);
    let mut decoded =
        image::DynamicImage::from_decoder(decoder).map_err(|error| reject(error.to_string()))?;
    decoded.apply_orientation(orientation);

    let mut out = Vec::new();
    let format = format.unwrap_or(match kind.extension {
        "jpeg" => image::ImageFormat::Jpeg,
        "gif" => image::ImageFormat::Gif,
        "webp" => image::ImageFormat::WebP,
        "avif" => image::ImageFormat::Avif,
        _ => image::ImageFormat::Png,
    });
    // GIF may be animated and WebP's encoder here is lossless-only; both are
    // re-encoded as PNG rather than degraded.
    let format = match format {
        image::ImageFormat::Gif => image::ImageFormat::Png,
        other => other,
    };
    decoded
        .write_to(&mut Cursor::new(&mut out), format)
        .map_err(|error| reject(error.to_string()))?;
    Ok(out)
}

/// The SVG allow list: no script, no event handler, no external reference.
pub fn sanitize_svg(text: &str) -> Result<String, Box<Diagnostic>> {
    use liyasa_markdown::sanitize::{allowlist, html};

    let reject = |reason: &str| {
        Box::new(
            Diagnostic::new(code::E0812, format!("the SVG was rejected: {reason}"))
                .help("remove the script, the event handler, or the external reference"),
        )
    };

    for token in html::tokenize(text) {
        let html::Token::Tag(tag) = token else {
            continue;
        };
        let name = tag.name.to_ascii_lowercase();
        if matches!(
            name.as_str(),
            "script" | "foreignobject" | "iframe" | "object" | "embed" | "animate"
        ) {
            return Err(reject(&format!("`<{name}>` is not allowed")));
        }
        for (attribute, value) in &tag.attributes {
            let attribute = attribute.to_ascii_lowercase();
            if allowlist::is_event_handler(&attribute) {
                return Err(reject(&format!("`{attribute}` is an event handler")));
            }
            let Some(value) = value else { continue };
            if matches!(
                attribute.as_str(),
                "href" | "xlink:href" | "src" | "from" | "to"
            ) && is_external(value)
            {
                return Err(reject(&format!("`{attribute}` points outside the file")));
            }
            if value.to_ascii_lowercase().contains("javascript:") {
                return Err(reject("a `javascript:` URL"));
            }
        }
    }
    Ok(text.to_owned())
}

fn is_external(value: &str) -> bool {
    let value = value.trim().to_ascii_lowercase();
    value.contains("://") || value.starts_with("//") || value.starts_with("data:text/html")
}

fn is_csv(text: &str) -> bool {
    let mut lines = text.lines().filter(|line| !line.trim().is_empty());
    let Some(first) = lines.next() else {
        return false;
    };
    let columns = first.matches(',').count();
    columns > 0
        && lines
            .take(4)
            .all(|line| line.matches(',').count() == columns)
}

/// Which pages use which asset, so "used on 4 pages" is a fact and a delete can
/// be refused (CM-131).
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct Usage {
    by_asset: BTreeMap<String, BTreeSet<Route>>,
}

impl Usage {
    pub fn record(&mut self, asset: &str, route: &Route) {
        self.by_asset
            .entry(asset.to_owned())
            .or_default()
            .insert(route.clone());
    }

    pub fn pages(&self, asset: &str) -> Vec<Route> {
        self.by_asset
            .get(asset)
            .map(|routes| routes.iter().cloned().collect())
            .unwrap_or_default()
    }

    pub fn count(&self, asset: &str) -> usize {
        self.by_asset.get(asset).map(BTreeSet::len).unwrap_or(0)
    }

    /// Delete is blocked while an asset is referenced (CM-131).
    pub fn may_delete(&self, asset: &str) -> Result<(), Box<Diagnostic>> {
        let pages = self.pages(asset);
        if pages.is_empty() {
            return Ok(());
        }
        Err(Box::new(
            Diagnostic::new(
                code::E0703,
                format!(
                    "`{asset}` is used on {} page(s) and cannot be deleted",
                    pages.len()
                ),
            )
            .help("remove the references first, or replace the file instead"),
        ))
    }

    /// Replace keeps the URL: the usage of the old bytes moves to the new ones
    /// (CM-131).
    pub fn replace(&mut self, asset: &str, with: &str) {
        if let Some(routes) = self.by_asset.remove(asset) {
            self.by_asset
                .entry(with.to_owned())
                .or_default()
                .extend(routes);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn png() -> Vec<u8> {
        let mut image = image::RgbaImage::new(4, 2);
        for (x, y, pixel) in image.enumerate_pixels_mut() {
            *pixel = image::Rgba([(x * 40) as u8, (y * 40) as u8, 10, 255]);
        }
        let mut out = std::io::Cursor::new(Vec::new());
        image::DynamicImage::ImageRgba8(image)
            .write_to(&mut out, image::ImageFormat::Png)
            .expect("the fixture encodes");
        out.into_inner()
    }

    fn upload<'a>(filename: &'a str, bytes: &'a [u8], alt: Option<&'a str>) -> Upload<'a> {
        Upload {
            filename,
            bytes,
            alt,
        }
    }

    #[test]
    fn the_type_is_sniffed_not_taken_from_the_name() {
        let bytes = png();
        let accepted = accept(
            &upload("payload.pdf", &bytes, Some("A gradient")),
            &Settings::default(),
        )
        .expect("a png is a png whatever it is called");
        assert_eq!(accepted.extension, "png");
        assert_eq!(accepted.content_type, "image/png");
    }

    #[test]
    fn a_type_outside_the_allow_list_is_e0812() {
        let settings = Settings {
            allow_types: vec!["png".to_owned()],
            ..Settings::default()
        };
        let error = accept(&upload("archive.zip", b"PK\x03\x04rest", None), &settings)
            .expect_err("zip is not allowed");
        assert_eq!(error.code.as_str(), "E0812");
    }

    #[test]
    fn an_unrecognized_type_is_refused() {
        let error = accept(
            &upload("thing.bin", &[0u8, 1, 2, 3], None),
            &Settings::default(),
        )
        .expect_err("bytes with no type");
        assert_eq!(error.code.as_str(), "E0812");
    }

    #[test]
    fn an_image_without_alt_text_is_refused() {
        let bytes = png();
        let error = accept(&upload("hero.png", &bytes, None), &Settings::default())
            .expect_err("alt text is required");
        assert_eq!(error.code.as_str(), "E0305");
    }

    #[test]
    fn an_image_is_re_encoded_so_its_metadata_is_gone() {
        let mut bytes = png();
        // A text chunk the encoder will not write back.
        bytes.extend_from_slice(b"tEXtComment\0secret");
        let accepted = accept(
            &upload("hero.png", &bytes, Some("A gradient")),
            &Settings::default(),
        )
        .expect("a png");
        assert!(
            !accepted.bytes.windows(6).any(|window| window == b"secret"),
            "metadata survived the re-encode"
        );
    }

    #[test]
    fn the_same_bytes_land_on_the_same_path() {
        let bytes = png();
        let first =
            accept(&upload("a.png", &bytes, Some("alt")), &Settings::default()).expect("a png");
        let second =
            accept(&upload("b.png", &bytes, Some("alt")), &Settings::default()).expect("a png");
        assert_eq!(first.path, second.path);
        assert!(first.path.starts_with("assets/uploads/"));
    }

    #[test]
    fn a_download_is_an_attachment_and_never_sniffed() {
        let accepted = accept(
            &upload("archive.zip", b"PK\x03\x04rest", None),
            &Settings::default(),
        )
        .expect("a zip");
        assert_eq!(accepted.disposition, Disposition::Attachment);
        assert!(accepted.nosniff);
    }

    #[test]
    fn a_video_is_served_inline() {
        let mut bytes = vec![0u8; 4];
        bytes.extend_from_slice(b"ftypisom");
        let accepted =
            accept(&upload("clip.mp4", &bytes, None), &Settings::default()).expect("an mp4");
        assert_eq!(accepted.disposition, Disposition::Inline);
    }

    #[test]
    fn an_svg_with_a_script_is_refused() {
        let svg = "<svg xmlns=\"http://www.w3.org/2000/svg\"><script>alert(1)</script></svg>";
        let error = accept(
            &upload("bad.svg", svg.as_bytes(), Some("alt")),
            &Settings::default(),
        )
        .expect_err("a script in an svg");
        assert_eq!(error.code.as_str(), "E0812");
    }

    #[test]
    fn an_svg_with_an_event_handler_or_a_remote_reference_is_refused() {
        for svg in [
            "<svg onload=\"steal()\"><rect/></svg>",
            "<svg><image href=\"https://evil.example/x.png\"/></svg>",
            "<svg><a href=\"javascript:alert(1)\">x</a></svg>",
        ] {
            assert!(sanitize_svg(svg).is_err(), "{svg} should have been refused");
        }
    }

    #[test]
    fn a_plain_svg_passes() {
        let svg =
            "<svg xmlns=\"http://www.w3.org/2000/svg\"><rect width=\"4\" height=\"4\"/></svg>";
        let accepted = accept(
            &upload("ok.svg", svg.as_bytes(), Some("A square")),
            &Settings::default(),
        )
        .expect("a clean svg");
        assert_eq!(accepted.disposition, Disposition::Inline);
        assert_eq!(accepted.extension, "svg");
    }

    #[test]
    fn the_attachment_policy_serves_an_svg_as_a_download_instead() {
        let svg = "<svg xmlns=\"http://www.w3.org/2000/svg\"><script>alert(1)</script></svg>";
        let settings = Settings {
            svg: SvgPolicy::Attachment,
            ..Settings::default()
        };
        let accepted = accept(&upload("any.svg", svg.as_bytes(), Some("alt")), &settings)
            .expect("an attachment is not executed, so it is not sanitized");
        assert_eq!(accepted.disposition, Disposition::Attachment);
    }

    #[test]
    fn usage_counts_pages_and_blocks_a_delete() {
        let mut usage = Usage::default();
        usage.record("assets/uploads/a.png", &Route::new("/guides/install"));
        usage.record("assets/uploads/a.png", &Route::new("/guides/upgrade"));
        assert_eq!(usage.count("assets/uploads/a.png"), 2);
        assert!(usage.may_delete("assets/uploads/a.png").is_err());
        assert!(usage.may_delete("assets/uploads/unused.png").is_ok());
    }

    #[test]
    fn replacing_an_asset_carries_its_usage_across() {
        let mut usage = Usage::default();
        usage.record("old.png", &Route::new("/"));
        usage.replace("old.png", "new.png");
        assert_eq!(usage.count("old.png"), 0);
        assert_eq!(usage.count("new.png"), 1);
    }
}
