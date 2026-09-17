//! ED-13's acceptance test.
//!
//! > Given an upload of a JPEG with GPS EXIF and an SVG with a script; when
//! > stored; then metadata is stripped, the script is removed, alt text is
//! > required, and deleting a referenced asset is refused.
//!
//! The subject is `liyasa_build::media`, which is what the editor's upload
//! calls through `/_liyasa/api/v1/assets`. The fixtures are built here rather
//! than checked in because the corpus is not committed, and a JPEG carrying a
//! real APP1 EXIF block with a real GPS IFD is a few dozen bytes of structure
//! — writing it out is what makes the assertion mean "the GPS tag was
//! removed" rather than "a file with no GPS tag still has none".

use liyasa_build::media::{self, Settings, Upload, Usage};
use liyasa_core::ids::Route;

/// A one-pixel JPEG with an APP1 EXIF block naming a GPS latitude.
fn jpeg_with_gps() -> Vec<u8> {
    let mut bytes = Vec::new();
    {
        use image::ImageEncoder;
        let encoder = image::codecs::jpeg::JpegEncoder::new(&mut bytes);
        encoder
            .write_image(&[200u8, 30, 30], 1, 1, image::ExtendedColorType::Rgb8)
            .expect("a one-pixel jpeg encodes");
    }
    let exif = exif_with_gps();
    let mut app1 = Vec::new();
    app1.extend_from_slice(&[0xFF, 0xE1]);
    app1.extend_from_slice(&((exif.len() + 2) as u16).to_be_bytes());
    app1.extend_from_slice(&exif);
    // APP1 goes straight after the SOI marker, which is the first two bytes.
    let mut out = Vec::with_capacity(bytes.len() + app1.len());
    out.extend_from_slice(&bytes[..2]);
    out.extend_from_slice(&app1);
    out.extend_from_slice(&bytes[2..]);
    out
}

/// `Exif\0\0` plus a little-endian TIFF header with one GPS IFD pointer.
fn exif_with_gps() -> Vec<u8> {
    let mut out = Vec::new();
    out.extend_from_slice(b"Exif\0\0");
    let tiff_at = out.len();
    out.extend_from_slice(b"II");
    out.extend_from_slice(&42u16.to_le_bytes());
    out.extend_from_slice(&8u32.to_le_bytes()); // IFD0 at offset 8

    // IFD0: one entry, tag 0x8825 (GPSInfoIFDPointer) -> offset 26.
    out.extend_from_slice(&1u16.to_le_bytes());
    out.extend_from_slice(&0x8825u16.to_le_bytes());
    out.extend_from_slice(&4u16.to_le_bytes()); // LONG
    out.extend_from_slice(&1u32.to_le_bytes());
    out.extend_from_slice(&26u32.to_le_bytes());
    out.extend_from_slice(&0u32.to_le_bytes()); // no next IFD

    // GPS IFD at 26: one entry, tag 0x0001 (GPSLatitudeRef) = "N".
    debug_assert_eq!(out.len() - tiff_at, 26);
    out.extend_from_slice(&1u16.to_le_bytes());
    out.extend_from_slice(&0x0001u16.to_le_bytes());
    out.extend_from_slice(&2u16.to_le_bytes()); // ASCII
    out.extend_from_slice(&2u32.to_le_bytes());
    out.extend_from_slice(b"N\0\0\0");
    out.extend_from_slice(&0u32.to_le_bytes());
    out
}


#[test]
fn a_jpeg_keeps_its_pixels_and_loses_its_gps() {
    let bytes = jpeg_with_gps();
    // The fixture is the defect if it cannot reach the state the assertion is
    // about: prove the GPS tag is in what goes *in*, before asserting it is
    // gone from what comes out.
    assert!(
        contains(&bytes, b"Exif\0\0"),
        "the fixture jpeg carries no EXIF block, so stripping it proves nothing"
    );
    assert!(
        contains(&bytes, &[0x25, 0x88]),
        "the fixture's EXIF carries no GPS pointer tag"
    );

    let accepted = media::accept(
        &Upload {
            filename: "photo.jpg",
            bytes: &bytes,
            alt: Some("A red pixel"),
        },
        &Settings::default(),
    )
    .expect("a jpeg with alt text is accepted");

    assert!(!contains(&accepted.bytes, b"Exif\0\0"), "the EXIF block survived");
    assert!(!contains(&accepted.bytes, &[0x25, 0x88]), "the GPS pointer tag survived");
    assert_eq!(accepted.content_type, "image/jpeg");
    assert!(accepted.nosniff, "every upload is served nosniff");

    // Still a decodable image of the right size, so "stripped" did not mean
    // "emptied".
    let decoded = image::load_from_memory(&accepted.bytes).expect("the stored bytes decode");
    assert_eq!((decoded.width(), decoded.height()), (1, 1));
}

/// Each hostile construct on its own, so a pass cannot come from one rule
/// catching everything.
const HOSTILE_SVG: &[(&str, &str)] = &[
    (
        "a script element",
        r#"<svg xmlns="http://www.w3.org/2000/svg"><script>fetch("x")</script></svg>"#,
    ),
    (
        "an event handler",
        r##"<svg xmlns="http://www.w3.org/2000/svg"><rect fill="#0af" onclick="alert(1)"/></svg>"##,
    ),
    (
        "a javascript url",
        r#"<svg xmlns="http://www.w3.org/2000/svg"><a xlink:href="javascript:alert(2)">go</a></svg>"#,
    ),
    (
        "an external reference",
        r#"<svg xmlns="http://www.w3.org/2000/svg"><image href="https://example.invalid/x.png"/></svg>"#,
    ),
    (
        "a foreignObject",
        r#"<svg xmlns="http://www.w3.org/2000/svg"><foreignObject><b>x</b></foreignObject></svg>"#,
    ),
];

const CLEAN_SVG: &str =
    r##"<svg xmlns="http://www.w3.org/2000/svg" viewBox="0 0 10 10"><rect width="10" height="10" fill="#0af"/></svg>"##;

#[test]
fn an_svg_carrying_a_script_never_reaches_storage() {
    // ED-13 says "the script is removed". `sanitize_svg` refuses the file
    // instead, and returns the text untouched when it is clean — it is a
    // validator under a stripper's name. Refusing is the stronger reading and
    // `plan/rfcs/2431-an-svg-is-refused-not-stripped.md` records why this
    // package took it; what both readings owe is that a script does not reach
    // the stored asset, which is what this asserts.
    for (what, svg) in HOSTILE_SVG {
        let error = media::accept(
            &Upload {
                filename: "diagram.svg",
                bytes: svg.as_bytes(),
                alt: Some("A blue square"),
            },
            &Settings::default(),
        )
        .expect_err(&format!("an svg with {what} is refused"));
        assert_eq!(error.code.as_str(), "E0812", "for {what}");
        assert!(
            error.message.contains("rejected"),
            "the refusal says the file was rejected, for {what}: {}",
            error.message
        );
    }
}

#[test]
fn a_clean_svg_is_stored_inline_and_unchanged() {
    // The other half: a validator that refused everything would pass the test
    // above and make the media library useless.
    let accepted = media::accept(
        &Upload {
            filename: "diagram.svg",
            bytes: CLEAN_SVG.as_bytes(),
            alt: Some("A blue square"),
        },
        &Settings::default(),
    )
    .expect("a clean svg is accepted");
    assert_eq!(String::from_utf8(accepted.bytes).as_deref(), Ok(CLEAN_SVG));
    assert_eq!(accepted.content_type, "image/svg+xml");
    assert!(accepted.nosniff);
}

#[test]
fn an_image_without_alt_text_is_refused() {
    for alt in [None, Some(""), Some("   ")] {
        let error = media::accept(
            &Upload {
                filename: "photo.jpg",
                bytes: &jpeg_with_gps(),
                alt,
            },
            &Settings::default(),
        )
        .expect_err("an image with no alt text is refused");
        assert_eq!(error.code.as_str(), "E0305", "for alt {alt:?}");
    }
}

#[test]
fn a_referenced_asset_cannot_be_deleted_and_an_unreferenced_one_can() {
    let mut usage = Usage::default();
    usage.record("assets/uploads/abc.png", &Route::new("/guides/limits"));
    usage.record("assets/uploads/abc.png", &Route::new("/guides/hosting"));

    let error = usage
        .may_delete("assets/uploads/abc.png")
        .expect_err("a referenced asset is not deletable");
    assert_eq!(error.code.as_str(), "E0703");
    assert!(
        error.message.contains('2'),
        "the refusal says how many pages use it: {}",
        error.message
    );
    assert_eq!(usage.count("assets/uploads/abc.png"), 2);

    usage
        .may_delete("assets/uploads/unused.png")
        .expect("an asset nothing references is deletable");
}

#[test]
fn replacing_an_asset_moves_its_usage_rather_than_stranding_it() {
    // The editor's "replace" is what an author reaches for when a screenshot
    // is out of date. If usage did not move, the new file would look unused
    // and be deletable while four pages still showed it.
    let mut usage = Usage::default();
    usage.record("assets/uploads/old.png", &Route::new("/guides/limits"));
    usage.replace("assets/uploads/old.png", "assets/uploads/new.png");

    assert_eq!(usage.count("assets/uploads/old.png"), 0);
    assert_eq!(usage.count("assets/uploads/new.png"), 1);
    usage
        .may_delete("assets/uploads/new.png")
        .expect_err("the replacement carries the usage");
}

#[test]
fn the_same_bytes_uploaded_twice_are_one_file() {
    let bytes = jpeg_with_gps();
    let upload = Upload {
        filename: "photo.jpg",
        bytes: &bytes,
        alt: Some("A red pixel"),
    };
    let first = media::accept(&upload, &Settings::default()).expect("accepted");
    let second = media::accept(
        &Upload {
            filename: "a-different-name.jpg",
            ..upload
        },
        &Settings::default(),
    )
    .expect("accepted");
    assert_eq!(first.path, second.path, "the path is the digest, not the name");
}

fn contains(haystack: &[u8], needle: &[u8]) -> bool {
    haystack.windows(needle.len()).any(|window| window == needle)
}
