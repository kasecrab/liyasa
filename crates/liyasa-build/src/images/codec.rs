//! The real encoder behind the image tier (PRD §6.2.1 "Image decode, resize,
//! encode").
//!
//! Every setting here is pinned rather than chosen per call: two builds of the
//! same source must produce the same bytes (§6.6.2 rule 4), so the speed and
//! quality of each encoder are constants and the AVIF encoder is held to one
//! thread, whose tiling would otherwise depend on the machine.

use std::io::Cursor;

use image::codecs::avif::AvifEncoder;
use image::codecs::jpeg::JpegEncoder;
use image::codecs::png::{CompressionType, FilterType as PngFilter, PngEncoder};
use image::codecs::webp::WebPEncoder;
use image::imageops::FilterType;
use image::{DynamicImage, ImageEncoder, ImageReader};

use super::{Encoder, ImageError, Variant};

/// Width of the inline blur-up preview (CM-132). Small enough to sit in the
/// HTML, large enough to suggest the image.
const PLACEHOLDER_WIDTH: u32 = 16;

const AVIF_SPEED: u8 = 4;
const AVIF_QUALITY: u8 = 70;
const JPEG_QUALITY: u8 = 82;

#[derive(Debug, Clone, Copy, Default)]
pub struct ImageCodec;

impl ImageCodec {
    fn decode(original: &[u8]) -> Result<DynamicImage, ImageError> {
        ImageReader::new(Cursor::new(original))
            .with_guessed_format()
            .map_err(|error| ImageError::Decode(error.to_string()))?
            .decode()
            .map_err(|error| ImageError::Decode(error.to_string()))
    }

    fn resized(image: &DynamicImage, width: u32) -> DynamicImage {
        if image.width() <= width {
            return image.clone();
        }
        // Lanczos3 for downscaling: the only filter here that does not visibly
        // soften a screenshot, which is most of what documentation shows.
        image.resize(width, u32::MAX, FilterType::Lanczos3)
    }

    fn write(image: &DynamicImage, variant: &Variant) -> Result<Vec<u8>, ImageError> {
        let mut out = Vec::new();
        let encode = |result: Result<(), image::ImageError>| {
            result.map_err(|error| ImageError::Encode(error.to_string()))
        };
        match variant.format {
            super::Format::Avif => {
                let encoder =
                    AvifEncoder::new_with_speed_quality(&mut out, AVIF_SPEED, AVIF_QUALITY)
                        .with_num_threads(Some(1));
                encode(encoder.write_image(
                    image.to_rgba8().as_raw(),
                    image.width(),
                    image.height(),
                    image::ExtendedColorType::Rgba8,
                ))?;
            }
            super::Format::Webp => {
                let encoder = WebPEncoder::new_lossless(&mut out);
                encode(encoder.write_image(
                    image.to_rgba8().as_raw(),
                    image.width(),
                    image.height(),
                    image::ExtendedColorType::Rgba8,
                ))?;
            }
            super::Format::Png => {
                let encoder = PngEncoder::new_with_quality(
                    &mut out,
                    CompressionType::Best,
                    PngFilter::Adaptive,
                );
                encode(encoder.write_image(
                    image.to_rgba8().as_raw(),
                    image.width(),
                    image.height(),
                    image::ExtendedColorType::Rgba8,
                ))?;
            }
            super::Format::Jpeg => {
                let encoder = JpegEncoder::new_with_quality(&mut out, JPEG_QUALITY);
                encode(encoder.write_image(
                    image.to_rgb8().as_raw(),
                    image.width(),
                    image.height(),
                    image::ExtendedColorType::Rgb8,
                ))?;
            }
        }
        Ok(out)
    }
}

impl Encoder for ImageCodec {
    fn dimensions(&self, original: &[u8]) -> Option<(u32, u32)> {
        ImageReader::new(Cursor::new(original))
            .with_guessed_format()
            .ok()?
            .into_dimensions()
            .ok()
    }

    fn encode(&self, original: &[u8], variant: &Variant) -> Result<Vec<u8>, ImageError> {
        let decoded = Self::decode(original)?;
        Self::write(&Self::resized(&decoded, variant.width), variant)
    }

    fn blur_placeholder(&self, original: &[u8]) -> Option<String> {
        let decoded = Self::decode(original).ok()?;
        let small = Self::resized(&decoded, PLACEHOLDER_WIDTH);
        let bytes = Self::write(
            &small,
            &Variant {
                width: PLACEHOLDER_WIDTH,
                format: super::Format::Webp,
            },
        )
        .ok()?;
        Some(format!("data:image/webp;base64,{}", base64(&bytes)))
    }
}

/// Base64 for one data URI. A dependency for 20 lines would need a row in
/// §6.2.1 that no other caller wants.
fn base64(bytes: &[u8]) -> String {
    const ALPHABET: &[u8; 64] = b"ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789+/";
    let mut out = String::with_capacity(bytes.len().div_ceil(3) * 4);
    for chunk in bytes.chunks(3) {
        let mut buffer = [0u8; 3];
        buffer[..chunk.len()].copy_from_slice(chunk);
        let packed = u32::from(buffer[0]) << 16 | u32::from(buffer[1]) << 8 | u32::from(buffer[2]);
        for index in 0..4 {
            if index <= chunk.len() {
                let at = (packed >> (18 - index * 6)) & 0x3f;
                out.push(char::from(ALPHABET[at as usize]));
            } else {
                out.push('=');
            }
        }
    }
    out
}

#[cfg(test)]
mod tests {
    use super::super::Format;
    use super::*;

    /// A 4x2 PNG, encoded here rather than checked in, so the fixture cannot
    /// drift from what the decoder expects.
    fn png(width: u32, height: u32) -> Vec<u8> {
        let mut image = image::RgbaImage::new(width, height);
        for (x, y, pixel) in image.enumerate_pixels_mut() {
            *pixel = image::Rgba([(x * 8) as u8, (y * 8) as u8, 128, 255]);
        }
        let mut out = Vec::new();
        DynamicImage::ImageRgba8(image)
            .write_to(&mut Cursor::new(&mut out), image::ImageFormat::Png)
            .expect("the fixture encodes");
        out
    }

    #[test]
    fn dimensions_are_read_without_decoding_the_whole_image() {
        assert_eq!(ImageCodec.dimensions(&png(40, 20)), Some((40, 20)));
        assert_eq!(ImageCodec.dimensions(b"not an image"), None);
    }

    #[test]
    fn a_variant_is_re_encoded_at_the_requested_width() {
        let original = png(40, 20);
        let bytes = ImageCodec
            .encode(
                &original,
                &Variant {
                    width: 20,
                    format: Format::Webp,
                },
            )
            .expect("the webp encodes");
        assert_eq!(ImageCodec.dimensions(&bytes), Some((20, 10)));
    }

    #[test]
    fn avif_is_produced_and_is_an_avif() {
        let bytes = ImageCodec
            .encode(
                &png(32, 16),
                &Variant {
                    width: 16,
                    format: Format::Avif,
                },
            )
            .expect("the avif encodes");
        assert!(bytes.len() > 12);
        assert_eq!(&bytes[4..12], b"ftypavif");
    }

    #[test]
    fn an_image_is_never_upscaled() {
        let bytes = ImageCodec
            .encode(
                &png(10, 10),
                &Variant {
                    width: 640,
                    format: Format::Webp,
                },
            )
            .expect("the webp encodes");
        assert_eq!(ImageCodec.dimensions(&bytes), Some((10, 10)));
    }

    #[test]
    fn the_same_source_encodes_to_the_same_bytes() {
        let original = png(40, 20);
        let variant = Variant {
            width: 20,
            format: Format::Avif,
        };
        let first = ImageCodec.encode(&original, &variant).expect("first");
        let second = ImageCodec.encode(&original, &variant).expect("second");
        assert_eq!(first, second);
    }

    #[test]
    fn a_placeholder_is_a_small_data_uri() {
        let uri = ImageCodec
            .blur_placeholder(&png(200, 100))
            .expect("a placeholder");
        assert!(uri.starts_with("data:image/webp;base64,"), "{uri}");
        assert!(uri.len() < 2048, "{} bytes", uri.len());
    }

    #[test]
    fn base64_matches_the_worked_examples() {
        assert_eq!(base64(b""), "");
        assert_eq!(base64(b"f"), "Zg==");
        assert_eq!(base64(b"fo"), "Zm8=");
        assert_eq!(base64(b"foo"), "Zm9v");
        assert_eq!(base64(b"foobar"), "Zm9vYmFy");
    }

    #[test]
    fn bytes_that_are_not_an_image_are_a_diagnostic_not_a_panic() {
        let error = ImageCodec
            .encode(
                b"nope",
                &Variant {
                    width: 10,
                    format: Format::Webp,
                },
            )
            .expect_err("refused");
        assert!(matches!(error, ImageError::Decode(_)));
    }
}
