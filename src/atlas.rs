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

//! Texture atlas handles, baked icon/font metadata, and construction helpers.
//!
//! Text is indexed by Rust [`char`] values. The built-in `builder` module bakes printable ASCII only;
//! serialized [`AtlasSource`] values can provide any Unicode scalar values. Runtime measurement
//! and drawing substitute the selected font's underscore entry for a missing character.

use std::collections::HashMap;
use std::fmt::{Debug, Formatter};

use super::*;

#[derive(Debug, Clone)]
/// Metrics and atlas coordinates for a glyph.
pub struct CharEntry {
    /// Pixel offset relative to the draw origin.
    pub offset: Vec2i,
    /// Horizontal advance after drawing this glyph.
    pub advance: Vec2i,
    /// Rectangle inside the atlas texture.
    pub rect: Recti, // coordinates in the atlas
}

#[derive(Clone)]
/// Internal font record stored in the atlas.
struct Font {
    /// Distance between text baselines in pixels.
    line_size: usize,
    /// Distance from the top of a line to its baseline.
    baseline: i32,
    /// Requested font size in pixels.
    font_size: usize,
    /// Glyph entries available in this font.
    ///
    /// The built-in builder populates printable ASCII; serialized sources may provide arbitrary
    /// Unicode scalar values.
    entries: HashMap<char, CharEntry>,
}

impl Debug for Font {
    fn fmt(&self, f: &mut Formatter<'_>) -> std::fmt::Result {
        use std::fmt::Write;
        let mut entries = String::new();
        for e in &self.entries {
            entries.write_fmt(format_args!("{:?}, ", e))?;
        }
        f.write_fmt(format_args!(
            "Font {{ line_size: {}, baseline: {}, font_size: {}, entries: [{}] }}",
            self.line_size, self.baseline, self.font_size, entries
        ))
    }
}

#[derive(Default, Copy, Clone, Debug, PartialEq, Eq, Hash)]
/// Handle referencing a font stored in the atlas.
pub struct FontId(usize);

#[derive(Default, Copy, Clone, Debug, PartialEq, Eq, Hash)]
/// Handle referencing a bitmap icon stored in the atlas.
pub struct IconId(usize);

impl Into<u32> for IconId {
    fn into(self) -> u32 {
        self.0 as _
    }
}

#[derive(Debug, Clone)]
/// Internal bitmap icon record stored in the atlas.
struct Icon {
    /// Rectangle occupied by the icon in atlas pixel coordinates.
    rect: Recti,
}

/// Immutable atlas storage shared through [`AtlasHandle`].
struct Atlas {
    /// Width of the atlas texture in pixels.
    width: usize,
    /// Height of the atlas texture in pixels.
    height: usize,
    /// RGBA pixel data in row-major order.
    pixels: Vec<Color4b>,
    /// Named fonts available to text layout and rendering.
    fonts: Vec<(String, Font)>,
    /// Named icons available to widgets.
    icons: Vec<(String, Icon)>,
}

#[derive(Clone)]
/// Shared read-only handle to a fully constructed atlas.
pub struct AtlasHandle(Rc<Atlas>);

/// Identifier of the solid white icon baked into the default atlas.
pub const WHITE_ICON: IconId = IconId(0);
/// Identifier of the close icon baked into the default atlas.
pub const CLOSE_ICON: IconId = IconId(1);
/// Identifier of the expand icon baked into the default atlas.
pub const EXPAND_ICON: IconId = IconId(2);
/// Identifier of the collapse icon baked into the default atlas.
pub const COLLAPSE_ICON: IconId = IconId(3);
/// Identifier of the checkbox icon baked into the default atlas.
pub const CHECK_ICON: IconId = IconId(4);
/// Identifier of the combo-box expand icon baked into the default atlas.
pub const EXPAND_DOWN_ICON: IconId = IconId(5);
/// Identifier of the open-folder icon baked into the default atlas.
pub const OPEN_FOLDER_16_ICON: IconId = IconId(6);
/// Identifier of the closed-folder icon baked into the default atlas.
pub const CLOSED_FOLDER_16_ICON: IconId = IconId(7);
/// Identifier of the file icon baked into the default atlas.
pub const FILE_16_ICON: IconId = IconId(8);

#[cfg(feature = "builder")]
/// Helpers for constructing atlas textures at build time.
pub mod builder;

mod source;
pub use source::{AtlasSource, FontEntry, SourceFormat};

#[cfg(feature = "save-to-rust")]
mod export;
mod runtime;
