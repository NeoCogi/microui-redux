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

#[cfg(any(feature = "builder", feature = "png_source"))]
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
