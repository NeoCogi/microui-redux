//
// Copyright 2023-Present (c) Raja Lehtihet & Wael El Oraiby
//
// Redistribution and use in source and binary forms, with or without
// modification, are permitted provided that the following conditions are met:
//
// 1. Redistributions of source code must retain the above copyright notice,
// this list of conditions and the following disclaimer.
//
// 2. Redistributions in binary form must reproduce the above copyright notice,
// this list of conditions and the following disclaimer in the documentation
// and/or other materials provided with the distribution.
//
// 3. Neither the name of the copyright holder nor the names of its contributors
// may be used to endorse or promote products derived from this software without
// specific prior written permission.
//
// THIS SOFTWARE IS PROVIDED BY THE COPYRIGHT HOLDERS AND CONTRIBUTORS "AS IS"
// AND ANY EXPRESS OR IMPLIED WARRANTIES, INCLUDING, BUT NOT LIMITED TO, THE
// IMPLIED WARRANTIES OF MERCHANTABILITY AND FITNESS FOR A PARTICULAR PURPOSE
// ARE DISCLAIMED. IN NO EVENT SHALL THE COPYRIGHT HOLDER OR CONTRIBUTORS BE
// LIABLE FOR ANY DIRECT, INDIRECT, INCIDENTAL, SPECIAL, EXEMPLARY, OR
// CONSEQUENTIAL DAMAGES (INCLUDING, BUT NOT LIMITED TO, PROCUREMENT OF
// SUBSTITUTE GOODS OR SERVICES; LOSS OF USE, DATA, OR PROFITS; OR BUSINESS
// INTERRUPTION) HOWEVER CAUSED AND ON ANY THEORY OF LIABILITY, WHETHER IN
// CONTRACT, STRICT LIABILITY, OR TORT (INCLUDING NEGLIGENCE OR OTHERWISE)
// ARISING IN ANY WAY OUT OF THE USE OF THIS SOFTWARE, EVEN IF ADVISED OF THE
// POSSIBILITY OF SUCH DAMAGE.
//

//! Tests for atlas image loading and source conversion helpers.

use super::*;
#[cfg(any(feature = "builder", feature = "png_source"))]
use png::{BitDepth, ColorType, Encoder};
#[cfg(any(feature = "builder", feature = "png_source"))]
use std::fmt::Write;

#[cfg(any(feature = "builder", feature = "png_source"))]
fn encode_png(color_type: ColorType, data: &[u8], width: u32, height: u32, palette: Option<&[u8]>) -> Vec<u8> {
    let mut buffer = Vec::new();
    {
        let mut encoder = Encoder::new(&mut buffer, width, height);
        encoder.set_color(color_type);
        encoder.set_depth(BitDepth::Eight);
        if let Some(palette) = palette {
            encoder.set_palette(palette);
        }
        let mut writer = encoder.write_header().unwrap();
        writer.write_image_data(data).unwrap();
    }
    buffer
}

/// Returns one checked image extent for tests that exercise the crate-private atlas path.
fn checked_dimensions(width: usize, height: usize) -> CheckedImageDimensions {
    // Small fixtures are necessarily within every coordinate and allocation bound; failure here
    // means the shared policy changed rather than that a decoder assertion should be updated.
    CheckedImageDimensions::try_new(width, height).expect("test image dimensions must satisfy the shared storage policy")
}

/// Computes the CRC-32 checksum used by PNG chunks without adding a test-only dependency.
#[cfg(any(feature = "builder", feature = "png_source"))]
fn png_crc32(bytes: &[u8]) -> u32 {
    // PNG uses the reflected IEEE polynomial. Applying its eight bit steps explicitly keeps the
    // malicious-header fixture local and avoids involving the image decoder in fixture creation.
    let mut crc = !0_u32;
    for byte in bytes {
        crc ^= u32::from(*byte);
        for _ in 0..8 {
            let low_bit_mask = (crc & 1).wrapping_neg();
            crc = (crc >> 1) ^ (0xEDB8_8320 & low_bit_mask);
        }
    }
    !crc
}

/// Rewrites a PNG fixture's IHDR extent and repairs that chunk's checksum.
#[cfg(any(feature = "builder", feature = "png_source"))]
fn rewrite_png_dimensions(bytes: &mut [u8], width: u32, height: u32) {
    // Every PNG begins with an eight-byte signature followed by the fixed 13-byte IHDR chunk. The
    // encoded fixture therefore has stable offsets for width, height, and the four-byte checksum.
    assert_eq!(&bytes[..8], b"\x89PNG\r\n\x1a\n");
    assert_eq!(&bytes[12..16], b"IHDR");
    bytes[16..20].copy_from_slice(&width.to_be_bytes());
    bytes[20..24].copy_from_slice(&height.to_be_bytes());
    let checksum = png_crc32(&bytes[12..29]);
    bytes[29..33].copy_from_slice(&checksum.to_be_bytes());
}

/// Verifies the fixed allocation budget accepts its exact RGBA boundary and rejects one extra row.
#[test]
fn checked_dimensions_enforce_the_decoded_storage_budget_without_allocating() {
    let accepted = CheckedImageDimensions::try_new(4096, 4096).expect("a 64-MiB RGBA image is the documented boundary");
    assert_eq!(accepted.pixel_count, 4096 * 4096);
    assert_eq!(accepted.rgba_byte_count, MAX_DECODED_RGBA_BYTES);

    let error = CheckedImageDimensions::try_new(4096, 4097).expect_err("one extra 4096-pixel row must exceed the fixed budget");
    assert_eq!(
        error,
        ImageStorageError::TooLarge {
            width: 4096,
            height: 4097,
            required_bytes: 4096 * 4097 * 4,
            maximum_bytes: MAX_DECODED_RGBA_BYTES,
        }
    );
}

#[cfg(any(feature = "builder", feature = "png_source"))]
#[test]
fn png_decode_error_returns_err() {
    let res = load_image_bytes(ImageSource::Png { bytes: &[] });
    assert!(res.is_err());
}

#[cfg(any(feature = "builder", feature = "png_source"))]
#[test]
fn png_decode_rgb_expands_alpha() {
    let bytes = encode_png(ColorType::Rgb, &[10, 20, 30], 1, 1, None);
    let (width, height, pixels) = load_image_bytes(ImageSource::Png { bytes: &bytes }).unwrap();

    assert_eq!(width, 1);
    assert_eq!(height, 1);
    assert_eq!(pixels.len(), 1);
    let pixel = pixels[0];
    assert_eq!((pixel.x, pixel.y, pixel.z, pixel.w), (10, 20, 30, 0xFF));
}

#[cfg(any(feature = "builder", feature = "png_source"))]
#[test]
fn png_decode_indexed_uses_palette() {
    let palette = [0x01, 0x02, 0x03];
    let bytes = encode_png(ColorType::Indexed, &[0], 1, 1, Some(&palette));
    let (width, height, pixels) = load_image_bytes(ImageSource::Png { bytes: &bytes }).unwrap();

    assert_eq!(width, 1);
    assert_eq!(height, 1);
    assert_eq!(pixels.len(), 1);

    let pixel = pixels[0];
    let mut message = String::new();
    let _ = write!(&mut message, "{},{},{},{}", pixel.x, pixel.y, pixel.z, pixel.w);
    assert_eq!(message, "1,2,3,255");
}

/// Verifies checked loading returns the same normalized pixels when PNG metadata agrees.
#[cfg(any(feature = "builder", feature = "png_source"))]
#[test]
fn checked_png_decode_shares_the_public_normalization_path() {
    let bytes = encode_png(ColorType::Rgb, &[10, 20, 30, 40, 50, 60], 2, 1, None);

    // Both APIs must use the same decoder and RGB-to-RGBA conversion; the checked path adds only
    // the independently supplied extent constraint.
    let public = load_image_bytes(ImageSource::Png { bytes: &bytes }).expect("valid PNG must decode");
    let checked = load_image_bytes_checked(ImageSource::Png { bytes: &bytes }, checked_dimensions(2, 1)).expect("matching dimensions must decode");

    assert_eq!(checked.0, public.0);
    assert_eq!(checked.1, public.1);
    assert_eq!(checked.2.len(), public.2.len());
    for (checked, public) in checked.2.iter().zip(&public.2) {
        assert_eq!((checked.x, checked.y, checked.z, checked.w), (public.x, public.y, public.z, public.w));
    }
}

/// Verifies a PNG header mismatch wins before corrupt frame data can be decoded.
#[cfg(any(feature = "builder", feature = "png_source"))]
#[test]
fn checked_png_decode_rejects_header_dimensions_before_frame_decode() {
    let mut bytes = encode_png(ColorType::Rgba, &[1, 2, 3, 4, 5, 6, 7, 8], 2, 1, None);
    let idat_type = bytes
        .windows(4)
        .position(|window| window == b"IDAT")
        .expect("encoded fixture must contain an IDAT chunk");
    // PNG chunk data starts immediately after its four-byte type. Corrupting the compressed stream
    // ensures decoding a frame would fail, while leaving the already-read IHDR dimensions intact.
    bytes[idat_type + 4] ^= 0xFF;
    assert!(
        load_image_bytes(ImageSource::Png { bytes: &bytes }).is_err(),
        "the corrupted fixture must fail if frame decoding is attempted"
    );

    let error = load_image_bytes_checked(ImageSource::Png { bytes: &bytes }, checked_dimensions(1, 1))
        .expect_err("the checked path must reject the 2x1 header against a 1x1 expectation");

    match error {
        CheckedImageLoadError::DimensionMismatch {
            expected_width,
            expected_height,
            actual_width,
            actual_height,
        } => {
            assert_eq!((expected_width, expected_height), (1, 1));
            assert_eq!((actual_width, actual_height), (2, 1));
        }
        CheckedImageLoadError::Decode { source } => {
            panic!("dimension validation ran too late and attempted frame decoding: {source}")
        }
        CheckedImageLoadError::Storage { source } => panic!("small checked dimensions unexpectedly failed storage validation: {source}"),
        CheckedImageLoadError::RawPixelLengthMismatch { .. } => panic!("PNG input was misclassified as raw RGBA bytes"),
        CheckedImageLoadError::AnimatedPngUnsupported => {
            panic!("ordinary static PNG fixture was misclassified as animated")
        }
    }
}

/// Verifies a hostile PNG extent is rejected from its header before frame-buffer allocation.
#[cfg(any(feature = "builder", feature = "png_source"))]
#[test]
fn png_decode_rejects_oversized_header_before_frame_decode() {
    let mut bytes = encode_png(ColorType::Rgba, &[1, 2, 3, 4], 1, 1, None);
    rewrite_png_dimensions(&mut bytes, 4096, 4097);

    // The one-pixel compressed stream cannot provide the rewritten 4097 rows. Receiving the
    // concrete storage variant proves the IHDR budget check won before frame decoding or either
    // decoded-pixel allocation was attempted.
    let error = load_image_bytes_impl(ImageSource::Png { bytes: &bytes }, None)
        .expect_err("a PNG exceeding the decoded-image budget must fail during header validation");
    assert!(matches!(
        error,
        CheckedImageLoadError::Storage {
            source: ImageStorageError::TooLarge {
                width: 4096,
                height: 4097,
                required_bytes: 67_125_248,
                maximum_bytes: MAX_DECODED_RGBA_BYTES,
            }
        }
    ));
}

/// Verifies raw sources report a concrete mismatch instead of requiring error-text inspection.
#[test]
fn checked_raw_decode_reports_both_dimension_pairs() {
    let bytes = [0_u8; 8];
    let error = load_image_bytes_checked(ImageSource::Raw { width: 2, height: 1, pixels: &bytes }, checked_dimensions(1, 1))
        .expect_err("raw dimensions must match the checked caller metadata");

    // Pattern matching exposes all four typed values without parsing the Display message.
    match error {
        CheckedImageLoadError::DimensionMismatch {
            expected_width,
            expected_height,
            actual_width,
            actual_height,
        } => {
            assert_eq!((expected_width, expected_height), (1, 1));
            assert_eq!((actual_width, actual_height), (2, 1));
        }
        CheckedImageLoadError::Decode { source } => panic!("valid raw bytes failed before the dimension check: {source}"),
        CheckedImageLoadError::Storage { source } => panic!("small checked dimensions unexpectedly failed storage validation: {source}"),
        CheckedImageLoadError::RawPixelLengthMismatch { .. } => panic!("dimension comparison must precede raw byte-length validation"),
        #[cfg(any(feature = "builder", feature = "png_source"))]
        CheckedImageLoadError::AnimatedPngUnsupported => panic!("raw RGBA bytes cannot describe an animated PNG"),
    }
}
