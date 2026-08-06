//! Image source descriptions, decoding, and RGBA validation.

use crate::{Color4b, color4b};
#[cfg(any(feature = "builder", feature = "png_source"))]
use png::{BitDepth, ColorType, Decoder, Transformations};
#[cfg(any(feature = "builder", feature = "png_source"))]
use std::io::Cursor;
use std::io::{Error, ErrorKind};

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
    /// PNG-compressed byte slice (requires the `builder` or `png_source` feature).
    /// Grayscale and RGB images are expanded to opaque RGBA (alpha = 255).
    Png {
        /// Compressed PNG payload.
        bytes: &'a [u8],
    },
}

/// Decodes image data into 32-bit pixels according to `source`.
/// Grayscale and RGB PNG inputs are expanded to opaque RGBA (alpha = 255).
pub fn load_image_bytes(source: ImageSource) -> std::io::Result<(usize, usize, Vec<Color4b>)> {
    match source {
        ImageSource::Raw { width, height, pixels } => {
            let expected = validate_rgba_buffer(width, height, pixels.len()).map_err(|err| Error::new(ErrorKind::Other, err))?;
            let width_usize = width as usize;
            let height_usize = height as usize;
            let mut colors = Vec::with_capacity(expected / 4);
            for chunk in pixels.chunks_exact(4) {
                colors.push(color4b(chunk[0], chunk[1], chunk[2], chunk[3]));
            }
            Ok((width_usize, height_usize, colors))
        }
        #[cfg(any(feature = "builder", feature = "png_source"))]
        ImageSource::Png { bytes } => decode_png_to_colors(bytes),
    }
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
/// Decodes a PNG stream into normalized RGBA pixels.
fn decode_png_to_colors(bytes: &[u8]) -> std::io::Result<(usize, usize, Vec<Color4b>)> {
    let mut cursor = Cursor::new(bytes);
    let mut decoder = Decoder::new(&mut cursor);
    decoder.set_transformations(Transformations::normalize_to_color8());
    let mut reader = decoder
        .read_info()
        .map_err(|e| Error::new(ErrorKind::Other, format!("PNG decode error: {}", e)))?;
    let buf_size = reader
        .output_buffer_size()
        .ok_or_else(|| Error::new(ErrorKind::Other, "PNG decoder did not report output size"))?;
    let mut img_data = vec![0; buf_size];
    let info = reader.next_frame(&mut img_data)?;

    if info.bit_depth != BitDepth::Eight {
        return Err(Error::new(ErrorKind::Other, format!("Unsupported PNG bit depth: {:?}", info.bit_depth)));
    }

    let pixel_size = match info.color_type {
        ColorType::Grayscale => 1,
        ColorType::GrayscaleAlpha => 2,
        ColorType::Indexed => 1,
        ColorType::Rgb => 3,
        ColorType::Rgba => 4,
    };

    let pixel_count = (info.width as usize)
        .checked_mul(info.height as usize)
        .ok_or_else(|| Error::new(ErrorKind::Other, "PNG dimensions overflow pixel count"))?;
    let mut pixels = vec![Color4b::default(); pixel_count];
    let line_size = info.line_size;
    // The decoder can normalize bit depth, but not all color models become RGBA directly. Expand
    // each source row explicitly so atlas construction has one canonical pixel format.
    for y in 0..info.height {
        let line = &img_data[(y as usize * line_size)..((y as usize + 1) * line_size)];

        for x in 0..info.width {
            let xx = (x * pixel_size) as usize;
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
                    return Err(Error::new(ErrorKind::Other, "Indexed PNGs are not supported"));
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
            pixels[(x + y * info.width) as usize] = color;
        }
    }

    Ok((info.width as _, info.height as _, pixels))
}
