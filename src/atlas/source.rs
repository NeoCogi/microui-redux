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

//! Serializable atlas source metadata and decoding.

use super::*;
use crate::image::{ImageSource, load_image_bytes};
use std::io::{Error, ErrorKind};

/// Describes a font baked into an [`AtlasSource`].
pub struct FontEntry<'a> {
    /// Distance between baselines in pixels.
    pub line_size: usize,
    /// Offset from the top of the line to the baseline.
    pub baseline: i32,
    /// Requested pixel size.
    pub font_size: usize,
    /// Glyph metadata table.
    pub entries: &'a [(char, CharEntry)],
}

/// Encodes how atlas pixel data is stored.
pub enum SourceFormat {
    /// Raw RGBA byte array.
    Raw,
    #[cfg(feature = "png_source")]
    /// PNG-formatted byte array.
    Png,
}

/// Serializable representation of an atlas that can be shipped with the binary.
pub struct AtlasSource<'a> {
    /// Width of the atlas texture.
    pub width: usize,
    /// Height of the atlas texture.
    pub height: usize,
    /// Pixel data matching [`AtlasSource::format`].
    pub pixels: &'a [u8],
    /// Icon lookup table.
    ///
    /// Entry zero must be an opaque white rendering tile. [`crate::render::Renderer`] samples that
    /// entry, identified by [`crate::WHITE_ICON`], when drawing solid geometry.
    pub icons: &'a [(&'a str, Recti)],
    /// Fonts baked into the atlas.
    pub fonts: &'a [(&'a str, FontEntry<'a>)],
    /// Encoding of [`AtlasSource::pixels`].
    pub format: SourceFormat,
}

impl AtlasHandle {
    /// Rehydrates atlas tables from serialized metadata and already-decoded pixels.
    fn from_parts<'a>(source: &AtlasSource<'a>, pixels: Vec<Color4b>) -> Self {
        let icons: Vec<(String, Icon)> = source.icons.iter().map(|(name, rect)| (name.to_string(), Icon { rect: *rect })).collect();
        let fonts: Vec<(String, Font)> = source
            .fonts
            .iter()
            .map(|(name, f)| {
                let font = Font {
                    line_size: f.line_size,
                    baseline: f.baseline,
                    font_size: f.font_size,
                    entries: f.entries.iter().map(|(ch, e)| (*ch, e.clone())).collect(),
                };
                (name.to_string(), font)
            })
            .collect();
        Self(Rc::new(Atlas {
            width: source.width,
            height: source.height,
            icons,
            fonts,
            pixels,
        }))
    }

    /// Reconstructs an atlas from a serialized [`AtlasSource`].
    ///
    /// This method panics when decoding fails. Use [`AtlasHandle::try_from`] to handle
    /// failures explicitly, or [`AtlasHandle::from_lossy`] to preserve the previous
    /// "blank atlas on error" fallback behavior.
    pub fn from<'a>(source: &AtlasSource<'a>) -> Self {
        Self::try_from(source).unwrap_or_else(|err| panic!("Atlas decode failed: {}", err))
    }

    /// Reconstructs an atlas from a serialized [`AtlasSource`], falling back to a blank atlas
    /// if decoding fails.
    pub fn from_lossy<'a>(source: &AtlasSource<'a>) -> Self {
        match Self::try_from(source) {
            Ok(atlas) => atlas,
            Err(err) => {
                debug_assert!(false, "Atlas decode failed: {}", err);
                let pixel_count = source.width.saturating_mul(source.height);
                Self::from_parts(source, vec![Color4b::default(); pixel_count])
            }
        }
    }

    /// Attempts to reconstruct an atlas from a serialized [`AtlasSource`].
    ///
    /// This validates pixel decoding and the declared image dimensions. It does not currently
    /// validate icon/glyph rectangles, semantic asset names, font metrics, or the required opaque
    /// white tile at icon index zero. Treat metadata as trusted and satisfy the [`AtlasSource`]
    /// field contracts before constructing the handle.
    pub fn try_from<'a>(source: &AtlasSource<'a>) -> std::io::Result<Self> {
        let width = i32::try_from(source.width).map_err(|_| Error::new(ErrorKind::Other, "Atlas width exceeds i32::MAX"))?;
        let height = i32::try_from(source.height).map_err(|_| Error::new(ErrorKind::Other, "Atlas height exceeds i32::MAX"))?;
        let pixels = match source.format {
            SourceFormat::Raw => {
                let (raw_width, raw_height, pixels) = load_image_bytes(ImageSource::Raw { width, height, pixels: source.pixels })?;
                if raw_width != source.width || raw_height != source.height {
                    return Err(Error::new(ErrorKind::Other, "Atlas dimensions do not match raw data"));
                }
                pixels
            }
            #[cfg(feature = "png_source")]
            SourceFormat::Png => {
                let (png_width, png_height, pixels) = load_image_bytes(ImageSource::Png { bytes: source.pixels })?;
                if png_width != source.width || png_height != source.height {
                    return Err(Error::new(ErrorKind::Other, "Atlas dimensions do not match PNG data"));
                }
                pixels
            }
        };
        Ok(Self::from_parts(source, pixels))
    }
}
