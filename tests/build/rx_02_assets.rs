//! RX-02 and CM-132: images are resized and re-encoded into AVIF and WebP
//! variants that a `srcset` references, the original is always retained, and
//! nothing is encoded inside the clean build — the tier is lazy and keyed by
//! fingerprint, so the second request for a variant is a cache hit.

use std::collections::BTreeMap;
use std::sync::Mutex;
use std::time::Duration;

use liyasa_build::images::codec::ImageCodec;
use liyasa_build::images::{self, Encoder, Format, Settings, Tier};
use liyasa_core::build::{ArtifactCache, CacheError, GcReport};
use liyasa_core::ids::Fingerprint;
use liyasa_core::vfs::{Bytes, VfsPath};

#[derive(Default)]
struct Memory {
    entries: Mutex<BTreeMap<Fingerprint, Bytes>>,
    reads: Mutex<u64>,
}

impl ArtifactCache for Memory {
    fn get(&self, key: &Fingerprint) -> Option<Bytes> {
        if let Ok(mut reads) = self.reads.lock() {
            *reads += 1;
        }
        self.entries.lock().ok()?.get(key).cloned()
    }

    fn put(&self, key: &Fingerprint, value: Bytes, _: &[Fingerprint]) -> Result<(), CacheError> {
        self.entries
            .lock()
            .map_err(|_| CacheError("poisoned".to_owned()))?
            .insert(*key, value);
        Ok(())
    }

    fn gc(&self, _: u64, _: Duration) -> Result<GcReport, CacheError> {
        Ok(GcReport::default())
    }
}

/// A gradient PNG of the given size, so the fixture is generated rather than
/// checked in (the corpus is not committed).
fn png(width: u32, height: u32) -> Vec<u8> {
    let mut image = image::RgbaImage::new(width, height);
    for (x, y, pixel) in image.enumerate_pixels_mut() {
        *pixel = image::Rgba([(x % 256) as u8, (y % 256) as u8, 96, 255]);
    }
    let mut out = std::io::Cursor::new(Vec::new());
    image::DynamicImage::ImageRgba8(image)
        .write_to(&mut out, image::ImageFormat::Png)
        .expect("the fixture encodes");
    out.into_inner()
}

/// The real breakpoints are 640..1920; the fixtures are small so that a debug
/// build's encoders stay inside the test suite's budget. Both formats are
/// named because a site may ask for AVIF today even though the encoder is one
/// licence row away (`plan/rfcs/0604-avif-and-the-licence-allow-list.md`).
fn settings() -> Settings {
    Settings {
        breakpoints: vec![64, 128],
        formats: vec![Format::Avif, Format::Webp],
        ..Settings::default()
    }
}

fn plan_of(original: &[u8]) -> images::Plan {
    let width = ImageCodec.dimensions(original).map(|(width, _)| width);
    images::plan(
        &VfsPath::new("assets/hero.png"),
        Fingerprint::of(original),
        width,
        &settings(),
    )
}

#[test]
fn a_plan_names_every_configured_format_at_every_breakpoint_under_the_original() {
    let plan = plan_of(&png(100, 50));
    assert_eq!(plan.formats(), vec![Format::Avif, Format::Webp]);
    let webp = plan.srcset(Format::Webp);
    assert!(webp.contains("/64.webp 64w"), "{webp}");
    assert!(webp.ends_with("/100.webp 100w"), "{webp}");
    assert!(!webp.contains("128"), "never upscaled: {webp}");
    assert_eq!(plan.srcset(Format::Avif).matches(".avif ").count(), 2);
}

#[test]
fn the_original_is_retained_and_stays_the_fallback() {
    let plan = plan_of(&png(100, 50));
    assert_eq!(plan.original_url, "/assets/hero.png");
    assert_eq!(plan.source.as_str(), "assets/hero.png");
}

#[test]
fn the_eager_pre_pass_writes_every_variant_it_can_encode() {
    let original = png(100, 50);
    let plan = plan_of(&original);
    let webp: Vec<_> = plan
        .derived
        .iter()
        .filter(|derived| derived.variant.format == Format::Webp)
        .collect();
    let cache = Memory::default();
    let tier = Tier {
        cache: &cache,
        encoder: &ImageCodec,
    };

    let report = tier.pre_pass(&original, &plan);
    assert_eq!(report.generated, webp.len() as u64);
    // AVIF is planned and reported rather than silently dropped (RFC 0604).
    assert_eq!(report.failed, plan.derived.len() as u64 - webp.len() as u64);

    for derived in webp {
        let bytes = tier
            .variant(&original, derived, plan.source_fingerprint)
            .expect("a variant");
        assert_eq!(&bytes[..4], b"RIFF");
        let (width, _) = ImageCodec.dimensions(&bytes).expect("a decodable webp");
        assert_eq!(width, derived.variant.width);
    }
}

#[test]
fn the_lazy_tier_generates_on_first_request_and_serves_the_cache_on_the_second() {
    let original = png(100, 50);
    let plan = plan_of(&original);
    let cache = Memory::default();
    let tier = Tier {
        cache: &cache,
        encoder: &ImageCodec,
    };
    let derived = plan
        .derived
        .iter()
        .find(|derived| derived.variant.format == Format::Webp)
        .expect("a webp variant");

    assert!(!tier.is_cached(derived), "a clean build encodes nothing");
    let first = tier
        .variant(&original, derived, plan.source_fingerprint)
        .expect("the first request encodes");
    let second = tier
        .variant(&original, derived, plan.source_fingerprint)
        .expect("the second request is a cache hit");
    assert_eq!(first, second);
    assert!(tier.is_cached(derived));
}

#[test]
fn a_changed_source_changes_every_variant_url() {
    let before = plan_of(&png(100, 50));
    let after = plan_of(&png(100, 51));
    assert_ne!(before.source_fingerprint, after.source_fingerprint);
    assert_ne!(before.srcset(Format::Webp), after.srcset(Format::Webp));
}

#[test]
fn a_blur_placeholder_is_small_enough_to_inline() {
    let uri = ImageCodec
        .blur_placeholder(&png(200, 100))
        .expect("a placeholder");
    assert!(uri.starts_with("data:image/webp;base64,"));
    assert!(uri.len() < 2048, "{} bytes", uri.len());
}
