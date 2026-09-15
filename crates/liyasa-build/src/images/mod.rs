//! The lazy image tier (PRD §6.6 performance table, §7.14 CM-132, §11.1
//! RX-02).
//!
//! Resizing and re-encoding never happen inside a clean build's time budget.
//! The build plans the variants, writes the `srcset` that references them, and
//! keys each one by `hash(source fingerprint, width, format, encoder)`. The
//! bytes are produced on first request in server mode, by the `--images`
//! pre-pass for a static export, or eagerly when `build.images.eager` is on.

pub mod codec;

use std::time::Duration;

use liyasa_core::build::{ArtifactCache, CacheError};
use liyasa_core::ids::Fingerprint;
use liyasa_core::vfs::{Bytes, VfsPath};
use serde::{Deserialize, Serialize};

/// Where generated variants live under `dist/` and on the server. The digest
/// makes the path immutable, so the whole prefix can be cached forever.
pub const VARIANT_PREFIX: &str = "_image";

/// Widths rendered when config names none (§7.14).
pub const DEFAULT_BREAKPOINTS: &[u32] = &[640, 960, 1280, 1920];

/// The encoder generation, mixed into every key: changing how a variant is
/// produced must not serve the previous encoder's bytes.
pub const ENCODER_VERSION: &str = "1";

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum Format {
    Avif,
    Webp,
    Png,
    Jpeg,
}

impl Format {
    pub fn extension(self) -> &'static str {
        match self {
            Format::Avif => "avif",
            Format::Webp => "webp",
            Format::Png => "png",
            Format::Jpeg => "jpeg",
        }
    }

    pub fn content_type(self) -> &'static str {
        match self {
            Format::Avif => "image/avif",
            Format::Webp => "image/webp",
            Format::Png => "image/png",
            Format::Jpeg => "image/jpeg",
        }
    }

    pub fn parse(text: &str) -> Option<Self> {
        match text.to_ascii_lowercase().as_str() {
            "avif" => Some(Format::Avif),
            "webp" => Some(Format::Webp),
            "png" => Some(Format::Png),
            "jpg" | "jpeg" => Some(Format::Jpeg),
            _ => None,
        }
    }

    /// Whether Liyasa resizes and re-encodes this source at all. An SVG is
    /// resolution-independent and a GIF may be animated; both are served whole.
    pub fn is_processable(extension: &str) -> bool {
        matches!(
            extension.to_ascii_lowercase().as_str(),
            "png" | "jpg" | "jpeg" | "webp" | "avif"
        )
    }
}

#[derive(Debug, Clone)]
pub struct Settings {
    /// `content.images.breakpoints`.
    pub breakpoints: Vec<u32>,
    /// `content.images.formats`.
    pub formats: Vec<Format>,
    /// `build.images.eager`: generate during the build instead of on demand.
    pub eager: bool,
    /// `build.basePath`.
    pub base_path: String,
}

impl Default for Settings {
    fn default() -> Self {
        Self {
            breakpoints: DEFAULT_BREAKPOINTS.to_vec(),
            formats: vec![Format::Avif, Format::Webp],
            eager: false,
            base_path: String::new(),
        }
    }
}

/// One `(width, format)` coordinate of one source image.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub struct Variant {
    pub width: u32,
    pub format: Format,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Derived {
    pub variant: Variant,
    /// `hash(source fingerprint, width, format, encoder version)`.
    pub key: Fingerprint,
    pub url: String,
}

/// What a page writes for one image, and what the tier generates for it.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Plan {
    pub source: VfsPath,
    pub source_fingerprint: Fingerprint,
    /// The original is always retained and always the `<img src>` fallback
    /// (CM-132).
    pub original_url: String,
    pub natural_width: Option<u32>,
    pub derived: Vec<Derived>,
}

impl Plan {
    pub fn is_empty(&self) -> bool {
        self.derived.is_empty()
    }

    pub fn by_key(&self, key: &Fingerprint) -> Option<&Derived> {
        self.derived.iter().find(|derived| &derived.key == key)
    }

    /// `srcset` for one format, narrowest first, as the HTML expects.
    pub fn srcset(&self, format: Format) -> String {
        let mut widths: Vec<(u32, &str)> = self
            .derived
            .iter()
            .filter(|derived| derived.variant.format == format)
            .map(|derived| (derived.variant.width, derived.url.as_str()))
            .collect();
        widths.sort_unstable();
        widths
            .iter()
            .map(|(width, url)| format!("{url} {width}w"))
            .collect::<Vec<_>>()
            .join(", ")
    }

    pub fn formats(&self) -> Vec<Format> {
        let mut out: Vec<Format> = self
            .derived
            .iter()
            .map(|derived| derived.variant.format)
            .collect();
        out.sort();
        out.dedup();
        out
    }
}

/// Plans every variant of one image. A source narrower than a breakpoint is
/// never upscaled, and a source Liyasa does not resize has no variants at all.
pub fn plan(
    source: &VfsPath,
    fingerprint: Fingerprint,
    natural_width: Option<u32>,
    settings: &Settings,
) -> Plan {
    let base = settings.base_path.trim_end_matches('/');
    let original_url = format!("{base}/{source}");
    let extension = source.extension().unwrap_or_default();

    let mut derived = Vec::new();
    if Format::is_processable(extension) {
        for format in &settings.formats {
            for width in widths(&settings.breakpoints, natural_width) {
                let variant = Variant {
                    width,
                    format: *format,
                };
                let key = key_of(fingerprint, &variant);
                derived.push(Derived {
                    url: format!("{base}/{}", variant_path(key, &variant)),
                    key,
                    variant,
                });
            }
        }
    }
    Plan {
        source: source.clone(),
        source_fingerprint: fingerprint,
        original_url,
        natural_width,
        derived,
    }
}

/// The breakpoints that make sense for a source of this width: never wider than
/// the original, plus the original's own width.
fn widths(breakpoints: &[u32], natural_width: Option<u32>) -> Vec<u32> {
    let mut out: Vec<u32> = match natural_width {
        Some(natural) => breakpoints
            .iter()
            .copied()
            .filter(|width| *width < natural)
            .chain(std::iter::once(natural))
            .collect(),
        None => breakpoints.to_vec(),
    };
    out.sort_unstable();
    out.dedup();
    out
}

pub fn key_of(fingerprint: Fingerprint, variant: &Variant) -> Fingerprint {
    Fingerprint::of_parts([
        b"image".as_slice(),
        ENCODER_VERSION.as_bytes(),
        &fingerprint.0,
        &variant.width.to_le_bytes(),
        variant.format.extension().as_bytes(),
    ])
}

/// `_image/<digest>/<width>.<ext>`: immutable, so a host may cache it forever.
pub fn variant_path(key: Fingerprint, variant: &Variant) -> String {
    format!(
        "{VARIANT_PREFIX}/{}/{}.{}",
        &key.to_hex()[..16],
        variant.width,
        variant.format.extension()
    )
}

#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
pub enum ImageError {
    #[error("{0}")]
    Decode(String),
    #[error("{0}")]
    Encode(String),
    #[error("no encoder is configured for {0}")]
    Unsupported(String),
    #[error(transparent)]
    Cache(#[from] CacheError),
}

/// What turns original bytes into one variant's bytes. The build engine owns
/// the tier; the codec is injected so a caller that cannot afford an image
/// decoder (a test, a wasm host) can still plan and serve.
pub trait Encoder: Send + Sync {
    fn dimensions(&self, original: &[u8]) -> Option<(u32, u32)>;
    fn encode(&self, original: &[u8], variant: &Variant) -> Result<Vec<u8>, ImageError>;
    /// A tiny inline preview for the blur-up placeholder (CM-132).
    fn blur_placeholder(&self, original: &[u8]) -> Option<String>;
}

/// A tier with no codec: every variant is [`ImageError::Unsupported`], which is
/// what a build without image support reports rather than failing.
#[derive(Debug, Clone, Copy, Default)]
pub struct NoEncoder;

impl Encoder for NoEncoder {
    fn dimensions(&self, _original: &[u8]) -> Option<(u32, u32)> {
        None
    }

    fn encode(&self, _original: &[u8], variant: &Variant) -> Result<Vec<u8>, ImageError> {
        Err(ImageError::Unsupported(
            variant.format.extension().to_owned(),
        ))
    }

    fn blur_placeholder(&self, _original: &[u8]) -> Option<String> {
        None
    }
}

/// The fingerprint-keyed tier itself: the cache answers first, the encoder runs
/// only on a miss (§6.6, "a separate, lazily populated, fingerprint-keyed
/// tier").
pub struct Tier<'a> {
    pub cache: &'a dyn ArtifactCache,
    pub encoder: &'a dyn Encoder,
}

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct Report {
    pub generated: u64,
    pub reused: u64,
    pub failed: u64,
}

impl Tier<'_> {
    /// The bytes of one variant, generating and caching them on a miss.
    pub fn variant(
        &self,
        original: &[u8],
        derived: &Derived,
        source_fingerprint: Fingerprint,
    ) -> Result<Bytes, ImageError> {
        if let Some(cached) = self.cache.get(&derived.key) {
            return Ok(cached);
        }
        let bytes = Bytes::from(self.encoder.encode(original, &derived.variant)?);
        self.cache
            .put(&derived.key, bytes.clone(), &[source_fingerprint])?;
        Ok(bytes)
    }

    pub fn is_cached(&self, derived: &Derived) -> bool {
        self.cache.get(&derived.key).is_some()
    }

    /// `liyasa build --images`: resumable, because a variant already in the
    /// cache is counted and skipped rather than re-encoded.
    pub fn pre_pass(&self, original: &[u8], plan: &Plan) -> Report {
        let mut report = Report::default();
        for derived in &plan.derived {
            if self.is_cached(derived) {
                report.reused += 1;
                continue;
            }
            match self.variant(original, derived, plan.source_fingerprint) {
                Ok(_) => report.generated += 1,
                Err(_) => report.failed += 1,
            }
        }
        report
    }

    /// Variants are not build inputs, so they are collected on their own
    /// schedule rather than against the artifact budget.
    pub fn gc(&self, max_bytes: u64, max_age: Duration) -> Result<(), CacheError> {
        self.cache.gc(max_bytes, max_age).map(|_| ())
    }
}

#[cfg(test)]
mod tests {
    use std::collections::BTreeMap;
    use std::sync::Mutex;

    use liyasa_core::build::GcReport;

    use super::*;

    #[derive(Default)]
    struct Memory(Mutex<BTreeMap<Fingerprint, Bytes>>);

    impl ArtifactCache for Memory {
        fn get(&self, key: &Fingerprint) -> Option<Bytes> {
            self.0.lock().ok()?.get(key).cloned()
        }

        fn put(
            &self,
            key: &Fingerprint,
            value: Bytes,
            _inputs: &[Fingerprint],
        ) -> Result<(), CacheError> {
            self.0
                .lock()
                .map_err(|_| CacheError("poisoned".to_owned()))?
                .insert(*key, value);
            Ok(())
        }

        fn gc(&self, _max_bytes: u64, _max_age: Duration) -> Result<GcReport, CacheError> {
            Ok(GcReport::default())
        }
    }

    /// Counts its calls, so a test can prove the second request did not encode.
    #[derive(Default)]
    struct Counting(Mutex<u32>);

    impl Encoder for Counting {
        fn dimensions(&self, _original: &[u8]) -> Option<(u32, u32)> {
            Some((2000, 1000))
        }

        fn encode(&self, original: &[u8], variant: &Variant) -> Result<Vec<u8>, ImageError> {
            if let Ok(mut calls) = self.0.lock() {
                *calls += 1;
            }
            Ok(format!(
                "{}@{}:{}",
                variant.format.extension(),
                variant.width,
                original.len()
            )
            .into_bytes())
        }

        fn blur_placeholder(&self, _original: &[u8]) -> Option<String> {
            Some("data:image/webp;base64,AAAA".to_owned())
        }
    }

    fn planned(natural: Option<u32>) -> Plan {
        plan(
            &VfsPath::new("assets/hero.png"),
            Fingerprint::of("original bytes"),
            natural,
            &Settings::default(),
        )
    }

    #[test]
    fn a_plan_covers_every_format_at_every_breakpoint() {
        let plan = planned(None);
        assert_eq!(plan.derived.len(), 8);
        assert_eq!(plan.formats(), vec![Format::Avif, Format::Webp]);
    }

    #[test]
    fn an_image_is_never_upscaled_and_its_own_width_is_kept() {
        let plan = planned(Some(800));
        let widths: Vec<u32> = plan
            .derived
            .iter()
            .filter(|derived| derived.variant.format == Format::Avif)
            .map(|derived| derived.variant.width)
            .collect();
        assert_eq!(widths, vec![640, 800]);
    }

    #[test]
    fn an_svg_is_served_whole() {
        let plan = plan(
            &VfsPath::new("assets/diagram.svg"),
            Fingerprint::of("<svg/>"),
            None,
            &Settings::default(),
        );
        assert!(plan.is_empty());
        assert_eq!(plan.original_url, "/assets/diagram.svg");
    }

    #[test]
    fn a_srcset_is_sorted_and_carries_widths() {
        let plan = planned(Some(1000));
        let srcset = plan.srcset(Format::Webp);
        assert!(srcset.starts_with("/_image/"), "{srcset}");
        assert!(srcset.contains("/640.webp 640w"), "{srcset}");
        assert!(srcset.ends_with("/1000.webp 1000w"), "{srcset}");
        assert!(!srcset.contains(".avif"), "{srcset}");
    }

    #[test]
    fn a_key_changes_with_the_bytes_the_width_and_the_format() {
        let one = Fingerprint::of("a");
        let two = Fingerprint::of("b");
        let small = Variant {
            width: 640,
            format: Format::Avif,
        };
        let large = Variant {
            width: 960,
            format: Format::Avif,
        };
        let webp = Variant {
            width: 640,
            format: Format::Webp,
        };
        assert_ne!(key_of(one, &small), key_of(two, &small));
        assert_ne!(key_of(one, &small), key_of(one, &large));
        assert_ne!(key_of(one, &small), key_of(one, &webp));
        assert_eq!(key_of(one, &small), key_of(one, &small));
    }

    #[test]
    fn the_second_request_for_a_variant_is_served_from_the_cache() {
        let cache = Memory::default();
        let encoder = Counting::default();
        let tier = Tier {
            cache: &cache,
            encoder: &encoder,
        };
        let plan = planned(Some(1000));
        let derived = plan.derived.first().expect("a variant");

        let first = tier
            .variant(b"original", derived, plan.source_fingerprint)
            .expect("the first request encodes");
        assert!(tier.is_cached(derived));
        let second = tier
            .variant(b"original", derived, plan.source_fingerprint)
            .expect("the second request hits the cache");
        assert_eq!(first, second);
        assert_eq!(*encoder.0.lock().expect("the counter"), 1);
    }

    #[test]
    fn the_pre_pass_resumes_instead_of_re_encoding() {
        let cache = Memory::default();
        let encoder = Counting::default();
        let tier = Tier {
            cache: &cache,
            encoder: &encoder,
        };
        let plan = planned(Some(1000));

        let first = tier.pre_pass(b"original", &plan);
        assert_eq!(first.generated, plan.derived.len() as u64);
        assert_eq!(first.reused, 0);

        let second = tier.pre_pass(b"original", &plan);
        assert_eq!(second.generated, 0);
        assert_eq!(second.reused, plan.derived.len() as u64);
    }

    #[test]
    fn a_build_with_no_codec_reports_rather_than_fails() {
        let cache = Memory::default();
        let tier = Tier {
            cache: &cache,
            encoder: &NoEncoder,
        };
        let plan = planned(Some(1000));
        let report = tier.pre_pass(b"original", &plan);
        assert_eq!(report.failed, plan.derived.len() as u64);
        assert_eq!(report.generated, 0);
    }
}
