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

//! Image source descriptions, decoding, and RGBA validation.

use crate::{Color4b, color4b};
#[cfg(any(feature = "builder", feature = "png_source"))]
use png::{BitDepth, ColorType, Decoder, Transformations};
#[cfg(any(feature = "builder", feature = "png_source"))]
use std::io::Cursor;
use std::{fmt, io::Error};

/// Maximum storage occupied by one decoded RGBA image or normalized color buffer.
///
/// Sixty-four mebibytes holds exactly 4,096 by 4,096 four-byte pixels, which is generous for a UI
/// atlas while preventing a compact image header from requesting multi-gigabyte allocations. PNG
/// decoding can temporarily own both its byte output and the normalized color vector; each buffer
/// is independently bounded by this value.
pub const MAX_DECODED_RGBA_BYTES: usize = 64 * 1024 * 1024;

#[derive(Copy, Clone)]
/// Describes image bytes that can be uploaded to a texture or decoded into an atlas.
pub enum ImageSource<'a> {
    /// Raw RGBA pixels laid out as width × height × 4 bytes.
    Raw {
        /// Width in pixels.
        width: i32,
        /// Height in pixels.
        height: i32,
        /// Pixel buffer in RGBA8888 format.
        pixels: &'a [u8],
    },
    #[cfg(any(feature = "builder", feature = "png_source"))]
    /// Static PNG-compressed byte slice (requires the `builder` or `png_source` feature).
    /// Grayscale and RGB images are expanded to opaque RGBA (alpha = 255); animated PNGs are
    /// rejected.
    Png {
        /// Compressed PNG payload.
        bytes: &'a [u8],
    },
}

/// Failure returned by the crate-internal image path that enforces declared dimensions.
///
/// Atlas construction needs to distinguish malformed image data from a valid image whose header
/// disagrees with its separately stored metadata. Keeping that distinction concrete lets the
/// atlas layer map each case into its own typed construction error without inspecting text from an
/// [`std::io::Error`].
#[derive(Debug)]
pub(crate) enum CheckedImageLoadError {
    /// The image bytes or raw RGBA description could not be decoded or validated.
    Decode {
        /// Concrete error produced by the shared image decoder.
        source: Error,
    },
    /// Image dimensions cannot produce an addressable, policy-bounded decoded buffer.
    Storage {
        /// Concrete dimension, arithmetic, or allocation-budget failure.
        source: ImageStorageError,
    },
    /// Raw RGBA bytes do not exactly fill their checked dimensions.
    RawPixelLengthMismatch {
        /// Exact byte count required by the checked dimensions.
        expected: usize,
        /// Actual byte count supplied by the caller.
        actual: usize,
    },
    /// The image dimensions disagree with the dimensions required by the caller.
    DimensionMismatch {
        /// Width required by the caller's metadata.
        expected_width: usize,
        /// Height required by the caller's metadata.
        expected_height: usize,
        /// Width declared by the raw source or PNG header.
        actual_width: usize,
        /// Height declared by the raw source or PNG header.
        actual_height: usize,
    },
    #[cfg(any(feature = "builder", feature = "png_source"))]
    /// The PNG contains animation metadata instead of one static image.
    AnimatedPngUnsupported,
}

impl CheckedImageLoadError {
    /// Converts the richer atlas-only classification into the public image API's I/O error.
    fn into_io_error(self) -> Error {
        // Preserve an original decoder error exactly. A dimension mismatch is unreachable through
        // the unchecked public path, but retaining a total conversion keeps the shared dispatcher
        // independent of that calling convention.
        match self {
            Self::Decode { source } => source,
            Self::Storage { source } => Error::other(source),
            mismatch @ Self::RawPixelLengthMismatch { .. } => Error::other(mismatch),
            mismatch @ Self::DimensionMismatch { .. } => Error::other(mismatch),
            #[cfg(any(feature = "builder", feature = "png_source"))]
            animated @ Self::AnimatedPngUnsupported => Error::other(animated),
        }
    }
}

impl fmt::Display for CheckedImageLoadError {
    /// Formats either the underlying decode failure or both sides of a dimension mismatch.
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        // Dimension values are included explicitly so callers can diagnose stale sidecar metadata
        // without parsing or re-decoding the image themselves.
        match self {
            Self::Decode { source } => write!(f, "{source}"),
            Self::Storage { source } => write!(f, "{source}"),
            Self::RawPixelLengthMismatch { expected, actual } => {
                write!(f, "expected {expected} RGBA bytes, received {actual}")
            }
            Self::DimensionMismatch {
                expected_width,
                expected_height,
                actual_width,
                actual_height,
            } => write!(
                f,
                "expected image dimensions {expected_width}x{expected_height}, received {actual_width}x{actual_height}"
            ),
            #[cfg(any(feature = "builder", feature = "png_source"))]
            Self::AnimatedPngUnsupported => f.write_str("animated PNG images are not supported"),
        }
    }
}

impl std::error::Error for CheckedImageLoadError {
    /// Exposes the concrete decoder error when image parsing, rather than dimensions, failed.
    fn source(&self) -> Option<&(dyn std::error::Error + 'static)> {
        // A mismatch is fully represented by this enum's integer fields and has no lower-level
        // cause. Decoder failures retain their original error chain.
        match self {
            Self::Decode { source } => Some(source),
            Self::Storage { source } => Some(source),
            Self::RawPixelLengthMismatch { .. } => None,
            Self::DimensionMismatch { .. } => None,
            #[cfg(any(feature = "builder", feature = "png_source"))]
            Self::AnimatedPngUnsupported => None,
        }
    }
}

impl From<Error> for CheckedImageLoadError {
    /// Wraps one concrete decoder or raw-buffer validation error.
    fn from(source: Error) -> Self {
        // Centralizing this conversion keeps every `?` in the shared decoder on the typed path.
        Self::Decode { source }
    }
}

impl From<ImageStorageError> for CheckedImageLoadError {
    /// Preserves a concrete checked-storage failure on the internal image-loading path.
    fn from(source: ImageStorageError) -> Self {
        // Keeping storage separate from decoder I/O lets atlas construction map the same invariant
        // to its public AtlasError without classifying an error message.
        Self::Storage { source }
    }
}

/// Concrete failure returned before allocating decoded or normalized image storage.
#[derive(Copy, Clone, Debug, Eq, PartialEq)]
pub(crate) enum ImageStorageError {
    /// One image axis is zero or exceeds the runtime rectangle coordinate domain.
    DimensionsOutOfRange {
        /// Requested image width in pixels.
        width: usize,
        /// Requested image height in pixels.
        height: usize,
    },
    /// Pixel or byte count arithmetic exceeds the platform allocation domain.
    Overflow {
        /// Requested image width in pixels.
        width: usize,
        /// Requested image height in pixels.
        height: usize,
    },
    /// Addressable storage exceeds the crate's deliberate decoded-image budget.
    TooLarge {
        /// Requested image width in pixels.
        width: usize,
        /// Requested image height in pixels.
        height: usize,
        /// Larger of the RGBA-byte and normalized-color allocations required by the image.
        required_bytes: usize,
        /// Maximum permitted size of either decoded allocation.
        maximum_bytes: usize,
    },
}

impl fmt::Display for ImageStorageError {
    /// Formats the concrete invalid dimensions, arithmetic failure, or exceeded budget.
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        // Diagnostics carry every numeric input needed to adjust an image without parsing a generic
        // allocation failure from the platform.
        match self {
            Self::DimensionsOutOfRange { width, height } => {
                write!(formatter, "decoded image dimensions must each be in 1..=i32::MAX, received {width}x{height}")
            }
            Self::Overflow { width, height } => {
                write!(formatter, "decoded image dimensions {width}x{height} overflow addressable RGBA storage")
            }
            Self::TooLarge {
                width,
                height,
                required_bytes,
                maximum_bytes,
            } => write!(
                formatter,
                "decoded image dimensions {width}x{height} require {required_bytes} bytes, exceeding the {maximum_bytes}-byte allocation limit"
            ),
        }
    }
}

impl std::error::Error for ImageStorageError {}

/// Validated image extent and exact allocation counts shared by image and atlas construction.
#[derive(Copy, Clone, Debug, Eq, PartialEq)]
pub(crate) struct CheckedImageDimensions {
    /// Positive image width representable by runtime rectangles.
    pub(crate) width: usize,
    /// Positive image height representable by runtime rectangles.
    pub(crate) height: usize,
    /// Exact number of decoded pixels.
    pub(crate) pixel_count: usize,
    /// Exact number of source or decoder RGBA8888 bytes.
    pub(crate) rgba_byte_count: usize,
}

impl CheckedImageDimensions {
    /// Checks coordinate representation, allocation arithmetic, and the decoded-image budget once.
    pub(crate) fn try_new(width: usize, height: usize) -> Result<Self, ImageStorageError> {
        if width == 0 || height == 0 || width > i32::MAX as usize || height > i32::MAX as usize {
            // Every accepted image can become a runtime texture rectangle without truncating an
            // axis. Applying this before multiplication also preserves a precise dimension error.
            return Err(ImageStorageError::DimensionsOutOfRange { width, height });
        }
        let Some(pixel_count) = width.checked_mul(height) else {
            return Err(ImageStorageError::Overflow { width, height });
        };
        let Some(rgba_byte_count) = pixel_count.checked_mul(4) else {
            return Err(ImageStorageError::Overflow { width, height });
        };
        let Some(normalized_byte_count) = pixel_count.checked_mul(std::mem::size_of::<Color4b>()) else {
            return Err(ImageStorageError::Overflow { width, height });
        };
        let required_bytes = rgba_byte_count.max(normalized_byte_count);
        if required_bytes > isize::MAX as usize {
            // Vec cannot address one allocation larger than isize::MAX even when usize arithmetic
            // itself succeeds on the target.
            return Err(ImageStorageError::Overflow { width, height });
        }
        if required_bytes > MAX_DECODED_RGBA_BYTES {
            // A fixed policy bound makes compressed-image and builder allocations deterministic
            // instead of delegating an attacker-controlled size to the global OOM handler.
            return Err(ImageStorageError::TooLarge {
                width,
                height,
                required_bytes,
                maximum_bytes: MAX_DECODED_RGBA_BYTES,
            });
        }
        Ok(Self {
            width,
            height,
            pixel_count,
            rgba_byte_count,
        })
    }
}

/// Decodes image data into 32-bit pixels according to `source`.
/// Grayscale and RGB PNG inputs are expanded to opaque RGBA (alpha = 255).
///
/// # Errors
///
/// Returns an I/O error when raw dimensions or byte counts are invalid, or when static PNG data is
/// malformed, uses an unsupported normalized representation, contains animation metadata, or
/// requires more than [`MAX_DECODED_RGBA_BYTES`] for either decoded allocation.
pub fn load_image_bytes(source: ImageSource) -> std::io::Result<(usize, usize, Vec<Color4b>)> {
    // The public API accepts the dimensions carried by the image source itself. It deliberately
    // delegates to the same implementation as checked atlas loading so format normalization cannot
    // diverge between the two entry points.
    load_image_bytes_impl(source, None).map_err(CheckedImageLoadError::into_io_error)
}

/// Decodes image data only when its declared dimensions equal the caller's expected dimensions.
///
/// For PNG input the comparison occurs immediately after reading the header and before querying or
/// allocating the decoder's output buffer. This lets atlas metadata bound compressed-image work
/// instead of discovering a mismatch after allocating for dimensions controlled by the PNG.
///
/// Returns a concrete decode or dimension-mismatch variant so the atlas layer never classifies an
/// error by inspecting its display text.
pub(crate) fn load_image_bytes_checked(source: ImageSource, expected: CheckedImageDimensions) -> Result<(usize, usize, Vec<Color4b>), CheckedImageLoadError> {
    // The caller supplies the same checked token it will retain with the decoded pixels. Matching
    // sources therefore reuse its cached counts instead of evaluating the allocation policy twice.
    load_image_bytes_impl(source, Some(expected))
}

/// Shared raw/PNG dispatcher used by both public and dimension-checked loading.
fn load_image_bytes_impl(source: ImageSource, expected: Option<CheckedImageDimensions>) -> Result<(usize, usize, Vec<Color4b>), CheckedImageLoadError> {
    // Each format validates its dimensions before allocating normalized color storage. PNG keeps
    // the expected extent through header parsing so compressed input receives the same guarantee.
    match source {
        ImageSource::Raw { width, height, pixels } => {
            if width <= 0 || height <= 0 {
                // Raw public input has signed dimensions, whereas the shared checked token stores
                // only valid positive extents. Reject invalid signs before converting to usize.
                return Err(Error::other("Image dimensions must be positive").into());
            }
            let width_usize = width as usize;
            let height_usize = height as usize;
            let dimensions = resolve_dimensions(expected, width_usize, height_usize)?;
            if pixels.len() != dimensions.rgba_byte_count {
                return Err(CheckedImageLoadError::RawPixelLengthMismatch {
                    expected: dimensions.rgba_byte_count,
                    actual: pixels.len(),
                });
            }
            let mut colors = Vec::with_capacity(dimensions.pixel_count);
            // Exact buffer validation above guarantees every byte belongs to one complete RGBA
            // pixel, so the remainder returned by fixed-size slice partitioning is empty.
            for chunk in pixels.as_chunks::<4>().0 {
                colors.push(color4b(chunk[0], chunk[1], chunk[2], chunk[3]));
            }
            Ok((dimensions.width, dimensions.height, colors))
        }
        #[cfg(any(feature = "builder", feature = "png_source"))]
        ImageSource::Png { bytes } => decode_png_to_colors(bytes, expected),
    }
}

/// Resolves one checked dimensions token without allocating pixel storage.
fn resolve_dimensions(
    expected: Option<CheckedImageDimensions>,
    actual_width: usize,
    actual_height: usize,
) -> Result<CheckedImageDimensions, CheckedImageLoadError> {
    // Atlas loading reuses its existing token after an exact extent comparison. The public loader
    // has no token, so this is its single dimension/arithmetic/budget validation point.
    if let Some(expected) = expected {
        if actual_width != expected.width || actual_height != expected.height {
            return Err(CheckedImageLoadError::DimensionMismatch {
                expected_width: expected.width,
                expected_height: expected.height,
                actual_width,
                actual_height,
            });
        }
        return Ok(expected);
    }
    CheckedImageDimensions::try_new(actual_width, actual_height).map_err(CheckedImageLoadError::from)
}

/// Returns the byte length required for an RGBA buffer with positive dimensions.
pub(crate) fn checked_rgba_byte_len(width: i32, height: i32) -> std::result::Result<usize, String> {
    if width <= 0 || height <= 0 {
        return Err(String::from("Image dimensions must be positive"));
    }
    let pixel_count = (width as usize)
        .checked_mul(height as usize)
        .ok_or_else(|| String::from("Image dimensions overflow RGBA byte count"))?;
    pixel_count
        .checked_mul(4)
        .ok_or_else(|| String::from("Image dimensions overflow RGBA byte count"))
}

/// Validates that `len` exactly matches the expected RGBA byte count.
pub(crate) fn validate_rgba_buffer(width: i32, height: i32, len: usize) -> std::result::Result<usize, String> {
    let expected = checked_rgba_byte_len(width, height)?;
    if len != expected {
        return Err(format!("Expected {} RGBA bytes, received {}", expected, len));
    }
    Ok(expected)
}

#[cfg(any(feature = "builder", feature = "png_source"))]
/// Decodes a PNG stream into normalized RGBA pixels after an optional header-size check.
fn decode_png_to_colors(bytes: &[u8], expected: Option<CheckedImageDimensions>) -> Result<(usize, usize, Vec<Color4b>), CheckedImageLoadError> {
    // Reading PNG metadata does not require an output pixel buffer. Keep the cursor and decoder
    // local so neither the public nor checked entry point can bypass the header validation below.
    let mut cursor = Cursor::new(bytes);
    let mut decoder = Decoder::new(&mut cursor);
    decoder.set_transformations(Transformations::normalize_to_color8());
    let mut reader = decoder.read_info().map_err(Error::other)?;

    // Copy header dimensions and validate them immediately after `read_info`. In particular, this
    // check must remain before `output_buffer_size` and the `Vec` allocation so a PNG cannot make
    // checked atlas loading allocate for dimensions different from its declared metadata.
    let header_width = reader.info().width as usize;
    let header_height = reader.info().height as usize;
    let dimensions = resolve_dimensions(expected, header_width, header_height)?;
    if reader.info().animation_control.is_some() {
        // `next_frame` returns an APNG frame rectangle rather than a composited canvas. Treating
        // that subframe as a static atlas would make the normalized pixel count disagree with the
        // already validated IHDR extent, so reject the unsupported format before allocation.
        return Err(CheckedImageLoadError::AnimatedPngUnsupported);
    }

    let buf_size = reader
        .output_buffer_size()
        .ok_or_else(|| Error::other("PNG decoder did not report output size"))?;
    if buf_size > dimensions.rgba_byte_count {
        // Color8 normalization cannot require more than four bytes per checked pixel. Treat a
        // dependency result outside that bound as a decode failure before allocating it.
        return Err(Error::other("PNG decoder output exceeds checked RGBA storage").into());
    }
    let mut img_data = vec![0; buf_size];
    let info = reader.next_frame(&mut img_data).map_err(Error::other)?;

    // Static PNG output must retain the IHDR extent. Keep this post-decode check even after APNG
    // rejection so a decoder behavior change cannot silently invalidate the allocation contract.
    resolve_dimensions(Some(dimensions), info.width as usize, info.height as usize)?;

    if info.bit_depth != BitDepth::Eight {
        return Err(Error::other(format!("Unsupported PNG bit depth: {:?}", info.bit_depth)).into());
    }

    let pixel_size: usize = match info.color_type {
        ColorType::Grayscale => 1,
        ColorType::GrayscaleAlpha => 2,
        ColorType::Indexed => 1,
        ColorType::Rgb => 3,
        ColorType::Rgba => 4,
    };

    let decoded_width = dimensions.width;
    let decoded_height = dimensions.height;
    let mut pixels = vec![Color4b::default(); dimensions.pixel_count];
    let line_size = info.line_size;
    let required_line_size = decoded_width
        .checked_mul(pixel_size)
        .ok_or_else(|| Error::other("PNG row dimensions overflow byte count"))?;
    if line_size < required_line_size {
        return Err(Error::other("PNG decoder returned a row shorter than its normalized color width").into());
    }
    // The decoder can normalize bit depth, but not all color models become RGBA directly. Expand
    // each source row explicitly so atlas construction has one canonical pixel format.
    for y in 0..decoded_height {
        let line_start = y
            .checked_mul(line_size)
            .ok_or_else(|| Error::other("PNG row offset overflowed output storage"))?;
        let line_end = line_start
            .checked_add(line_size)
            .ok_or_else(|| Error::other("PNG row end overflowed output storage"))?;
        let line = img_data
            .get(line_start..line_end)
            .ok_or_else(|| Error::other("PNG decoder returned incomplete normalized row storage"))?;

        for x in 0..decoded_width {
            // Both products are usize operations bounded by the checked row and pixel counts above;
            // using decoded u32 indices here used to overflow before the final cast.
            let xx = x * pixel_size;
            let color = match info.color_type {
                ColorType::Grayscale => {
                    let v = line[xx];
                    color4b(v, v, v, 0xFF)
                }
                ColorType::GrayscaleAlpha => {
                    let c = line[xx];
                    let a = line[xx + 1];
                    color4b(c, c, c, a)
                }
                ColorType::Indexed => {
                    return Err(Error::other("Indexed PNGs are not supported").into());
                }
                ColorType::Rgb => color4b(line[xx], line[xx + 1], line[xx + 2], 0xFF),
                ColorType::Rgba => {
                    let r = line[xx];
                    let g = line[xx + 1];
                    let b = line[xx + 2];
                    let a = line[xx + 3];
                    color4b(r, g, b, a)
                }
            };
            pixels[x + y * decoded_width] = color;
        }
    }

    Ok((decoded_width, decoded_height, pixels))
}

#[cfg(test)]
mod tests;
