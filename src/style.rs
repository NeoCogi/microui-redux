//
// Copyright 2022-Present (c) Raja Lehtihet & Wael El Oraiby
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
// -----------------------------------------------------------------------------
// Ported to rust from https://github.com/rxi/microui/ and the original license
//
// Copyright (c) 2020 rxi
//
// Permission is hereby granted, free of charge, to any person obtaining a copy
// of this software and associated documentation files (the "Software"), to
// deal in the Software without restriction, including without limitation the
// rights to use, copy, modify, merge, publish, distribute, sublicense, and/or
// sell copies of the Software, and to permit persons to whom the Software is
// furnished to do so, subject to the following conditions:
//
// The above copyright notice and this permission notice shall be included in
// all copies or substantial portions of the Software.
//
// THE SOFTWARE IS PROVIDED "AS IS", WITHOUT WARRANTY OF ANY KIND, EXPRESS OR
// IMPLIED, INCLUDING BUT NOT LIMITED TO THE WARRANTIES OF MERCHANTABILITY,
// FITNESS FOR A PARTICULAR PURPOSE AND NONINFRINGEMENT. IN NO EVENT SHALL THE
// AUTHORS OR COPYRIGHT HOLDERS BE LIABLE FOR ANY CLAIM, DAMAGES OR OTHER
// LIABILITY, WHETHER IN AN ACTION OF CONTRACT, TORT OR OTHERWISE, ARISING
// FROM, OUT OF OR IN CONNECTION WITH THE SOFTWARE OR THE USE OR OTHER DEALINGS
// IN THE SOFTWARE.
//
//! Visual primitives and lightweight geometry helpers used across the crate.

use rs_math3d::{Recti, Vec2i};

use crate::atlas::{FontId, SlotId};

#[derive(Default, Copy, Clone)]
#[repr(C)]
/// Simple RGBA color stored with 8-bit components.
pub struct Color {
    /// Red channel.
    pub r: u8,
    /// Green channel.
    pub g: u8,
    /// Blue channel.
    pub b: u8,
    /// Alpha channel.
    pub a: u8,
}

/// Describes the interface the atlas uses to query font metadata.
pub trait Font {
    /// Returns the font's display name.
    fn name(&self) -> &str;
    /// Returns the base pixel size of the font.
    fn get_size(&self) -> usize;
    /// Returns the pixel width and height for a specific character.
    fn get_char_size(&self, c: char) -> (usize, usize);
}

#[derive(Copy, Clone)]
/// Collection of visual constants that drive widget appearance.
pub struct Style {
    /// Font used for all text rendering.
    pub font: FontId,
    /// Default width used by layouts when no preferred width is supplied.
    pub default_cell_width: i32,
    /// Inner padding applied to most widgets.
    pub padding: i32,
    /// Spacing between cells in a layout.
    pub spacing: i32,
    /// Indentation applied to nested content.
    pub indent: i32,
    /// Height of window title bars.
    pub title_height: i32,
    /// Width of scrollbars.
    pub scrollbar_size: i32,
    /// Size of slider thumbs.
    pub thumb_size: i32,
    /// Palette of [`crate::ControlColor`] entries.
    pub colors: [Color; 14],
}

/// Floating-point type used by widgets and layout calculations.
pub type Real = f32;

#[derive(Copy, Clone, Debug, PartialEq, Eq, Hash)]
/// Handle referencing a renderer-owned texture.
pub struct TextureId(pub(crate) u32);

impl TextureId {
    /// Returns the raw numeric identifier stored inside the handle.
    pub fn raw(self) -> u32 {
        self.0
    }
}

#[derive(Copy, Clone, Debug, PartialEq, Eq)]
/// Either a slot stored inside the atlas or a standalone texture.
pub enum Image {
    /// Reference to an atlas slot.
    Slot(SlotId),
    /// Reference to an external texture ID.
    Texture(TextureId),
}

#[derive(Copy, Clone)]
/// Describes image bytes that can be uploaded to a texture.
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

pub(crate) static UNCLIPPED_RECT: Recti = Recti {
    x: 0,
    y: 0,
    width: i32::MAX,
    height: i32::MAX,
};

impl Default for Style {
    fn default() -> Self {
        Self {
            font: FontId::default(),
            default_cell_width: 68,
            padding: 5,
            spacing: 4,
            indent: 24,
            title_height: 24,
            scrollbar_size: 12,
            thumb_size: 8,
            colors: [
                Color { r: 230, g: 230, b: 230, a: 255 },
                Color { r: 25, g: 25, b: 25, a: 255 },
                Color { r: 50, g: 50, b: 50, a: 255 },
                Color { r: 25, g: 25, b: 25, a: 255 },
                Color { r: 240, g: 240, b: 240, a: 255 },
                Color { r: 0, g: 0, b: 0, a: 0 },
                Color { r: 75, g: 75, b: 75, a: 255 },
                Color { r: 95, g: 95, b: 95, a: 255 },
                Color { r: 115, g: 115, b: 115, a: 255 },
                Color { r: 30, g: 30, b: 30, a: 255 },
                Color { r: 35, g: 35, b: 35, a: 255 },
                Color { r: 40, g: 40, b: 40, a: 255 },
                Color { r: 43, g: 43, b: 43, a: 255 },
                Color { r: 30, g: 30, b: 30, a: 255 },
            ],
        }
    }
}

/// Convenience constructor for [`Vec2i`].
pub fn vec2(x: i32, y: i32) -> Vec2i {
    Vec2i { x, y }
}

/// Convenience constructor for [`Recti`].
pub fn rect(x: i32, y: i32, w: i32, h: i32) -> Recti {
    Recti { x, y, width: w, height: h }
}

/// Convenience constructor for [`Color`].
pub fn color(r: u8, g: u8, b: u8, a: u8) -> Color {
    Color { r, g, b, a }
}

/// Expands (or shrinks) a rectangle uniformly on all sides.
pub fn expand_rect(r: Recti, n: i32) -> Recti {
    rect(r.x - n, r.y - n, r.width + n * 2, r.height + n * 2)
}
