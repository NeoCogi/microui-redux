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

use crate::atlas::{AtlasHandle, FontId, SlotId};

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

#[derive(Copy, Clone, Debug, PartialEq, Eq, Default)]
/// Semantic font roles used by the built-in widgets and default style.
pub enum FontRole {
    /// Default body text used by most widgets.
    #[default]
    Body,
    /// Compact supporting text.
    Small,
    /// Window titles and similar chrome text.
    Title,
    /// Larger display text.
    Heading,
    /// Monospace-style text.
    Mono,
}

impl FontRole {
    /// Returns the conventional atlas font name used by [`Style::bind_named_fonts`].
    pub fn atlas_name(self) -> &'static str {
        match self {
            Self::Body => "body",
            Self::Small => "small",
            Self::Title => "title",
            Self::Heading => "heading",
            Self::Mono => "mono",
        }
    }
}

#[derive(Copy, Clone, Debug, PartialEq, Eq)]
/// Selects either a semantic font role from [`Style`] or a specific [`FontId`].
pub enum FontChoice {
    /// Resolve through a [`FontRole`] stored on the style.
    Role(FontRole),
    /// Use the provided concrete font directly.
    Id(FontId),
}

impl Default for FontChoice {
    fn default() -> Self {
        Self::Role(FontRole::Body)
    }
}

impl From<FontRole> for FontChoice {
    fn from(role: FontRole) -> Self {
        Self::Role(role)
    }
}

impl From<FontId> for FontChoice {
    fn from(font: FontId) -> Self {
        Self::Id(font)
    }
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
    /// Default body font used for general text rendering.
    pub font: FontId,
    /// Font used for compact supporting text.
    pub small_font: FontId,
    /// Font used for window titles and similar chrome text.
    pub title_font: FontId,
    /// Font used for larger display text.
    pub heading_font: FontId,
    /// Font used for monospace-style text.
    pub mono_font: FontId,
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
            small_font: FontId::default(),
            title_font: FontId::default(),
            heading_font: FontId::default(),
            mono_font: FontId::default(),
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

impl FontChoice {
    /// Creates a semantic font selection.
    pub fn role(role: FontRole) -> Self {
        Self::Role(role)
    }

    /// Creates a concrete font selection.
    pub fn id(font: FontId) -> Self {
        Self::Id(font)
    }

    /// Resolves this choice against `style`.
    pub fn resolve(self, style: &Style) -> FontId {
        style.resolve_font_choice(self)
    }
}

impl Style {
    /// Returns the concrete font ID for the provided semantic role.
    pub fn resolve_font_role(&self, role: FontRole) -> FontId {
        match role {
            FontRole::Body => self.font,
            FontRole::Small => self.small_font,
            FontRole::Title => self.title_font,
            FontRole::Heading => self.heading_font,
            FontRole::Mono => self.mono_font,
        }
    }

    /// Returns the concrete font ID for `choice`.
    pub fn resolve_font_choice(&self, choice: FontChoice) -> FontId {
        match choice {
            FontChoice::Role(role) => self.resolve_font_role(role),
            FontChoice::Id(font) => font,
        }
    }

    /// Binds semantic font roles only for fields that still use default/unset font IDs.
    ///
    /// This is intended for compatibility paths such as [`Context::set_style`], where callers
    /// often start from [`Style::default`] and only tweak colors or spacing. Explicit non-default
    /// font IDs are preserved.
    pub fn bind_default_named_fonts(&mut self, atlas: &AtlasHandle) {
        let default_font = FontId::default();
        if self.font == default_font {
            if let Some(font) = atlas.font_id(FontRole::Body.atlas_name()) {
                self.font = font;
            }
        }
        if self.small_font == default_font {
            self.small_font = atlas.font_id(FontRole::Small.atlas_name()).unwrap_or(self.font);
        }
        if self.title_font == default_font {
            self.title_font = atlas.font_id(FontRole::Title.atlas_name()).unwrap_or(self.font);
        }
        if self.heading_font == default_font {
            self.heading_font = atlas.font_id(FontRole::Heading.atlas_name()).unwrap_or(self.font);
        }
        if self.mono_font == default_font {
            self.mono_font = atlas.font_id(FontRole::Mono.atlas_name()).unwrap_or(self.font);
        }
    }

    /// Returns a copy of the style with semantic font roles rebound from `atlas`.
    pub fn with_named_fonts(mut self, atlas: &AtlasHandle) -> Self {
        self.bind_named_fonts(atlas);
        self
    }

    /// Binds semantic font roles from conventional atlas names when they exist.
    ///
    /// The lookup names are:
    /// - [`FontRole::Body`] => `body`
    /// - [`FontRole::Small`] => `small`
    /// - [`FontRole::Title`] => `title`
    /// - [`FontRole::Heading`] => `heading`
    /// - [`FontRole::Mono`] => `mono`
    ///
    /// Missing roles fall back to the resolved body font.
    pub fn bind_named_fonts(&mut self, atlas: &AtlasHandle) {
        if let Some(font) = atlas.font_id(FontRole::Body.atlas_name()) {
            self.font = font;
        }
        self.small_font = atlas.font_id(FontRole::Small.atlas_name()).unwrap_or(self.font);
        self.title_font = atlas.font_id(FontRole::Title.atlas_name()).unwrap_or(self.font);
        self.heading_font = atlas.font_id(FontRole::Heading.atlas_name()).unwrap_or(self.font);
        self.mono_font = atlas.font_id(FontRole::Mono.atlas_name()).unwrap_or(self.font);
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

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{AtlasHandle, AtlasSource, CharEntry, FontEntry, SourceFormat};

    fn make_test_atlas(fonts: &[(&str, usize)]) -> AtlasHandle {
        let pixels = [0xFF, 0xFF, 0xFF, 0xFF];
        let entries = [
            (
                '_',
                CharEntry {
                    offset: Vec2i::new(0, 0),
                    advance: Vec2i::new(8, 0),
                    rect: Recti::new(0, 0, 1, 1),
                },
            ),
            (
                'a',
                CharEntry {
                    offset: Vec2i::new(0, 0),
                    advance: Vec2i::new(8, 0),
                    rect: Recti::new(0, 0, 1, 1),
                },
            ),
        ];
        let font_entries: Vec<_> = fonts
            .iter()
            .map(|(name, size)| {
                (
                    *name,
                    FontEntry {
                        line_size: *size,
                        baseline: *size as i32 - 2,
                        font_size: *size,
                        entries: &entries,
                    },
                )
            })
            .collect();
        let source = AtlasSource {
            width: 1,
            height: 1,
            pixels: &pixels,
            icons: &[],
            fonts: font_entries.as_slice(),
            format: SourceFormat::Raw,
            slots: &[],
        };
        AtlasHandle::from(&source)
    }

    #[test]
    fn font_choice_conversions_preserve_selected_font() {
        let atlas = make_test_atlas(&[(FontRole::Body.atlas_name(), 12), (FontRole::Heading.atlas_name(), 18)]);
        let heading = atlas.font_id(FontRole::Heading.atlas_name()).unwrap();

        assert_eq!(FontChoice::from(FontRole::Heading), FontChoice::role(FontRole::Heading));
        assert_eq!(FontChoice::from(heading), FontChoice::id(heading));
    }

    #[test]
    fn bind_named_fonts_uses_conventional_role_names() {
        let atlas = make_test_atlas(&[
            (FontRole::Body.atlas_name(), 12),
            (FontRole::Small.atlas_name(), 10),
            (FontRole::Title.atlas_name(), 16),
            (FontRole::Heading.atlas_name(), 18),
        ]);

        let style = Style::default().with_named_fonts(&atlas);

        assert_eq!(style.font, atlas.font_id(FontRole::Body.atlas_name()).unwrap());
        assert_eq!(style.small_font, atlas.font_id(FontRole::Small.atlas_name()).unwrap());
        assert_eq!(style.title_font, atlas.font_id(FontRole::Title.atlas_name()).unwrap());
        assert_eq!(style.heading_font, atlas.font_id(FontRole::Heading.atlas_name()).unwrap());
        assert_eq!(style.mono_font, style.font);
    }

    #[test]
    fn bind_default_named_fonts_replaces_unset_font_fields_only() {
        let atlas = make_test_atlas(&[
            (FontRole::Small.atlas_name(), 10),
            (FontRole::Body.atlas_name(), 12),
            (FontRole::Title.atlas_name(), 16),
            (FontRole::Heading.atlas_name(), 18),
        ]);

        let mut style = Style::default();
        style.bind_default_named_fonts(&atlas);
        assert_eq!(style.font, atlas.font_id(FontRole::Body.atlas_name()).unwrap());
        assert_eq!(style.small_font, atlas.font_id(FontRole::Small.atlas_name()).unwrap());
        assert_eq!(style.title_font, atlas.font_id(FontRole::Title.atlas_name()).unwrap());
        assert_eq!(style.heading_font, atlas.font_id(FontRole::Heading.atlas_name()).unwrap());

        let explicit_title = atlas.font_id(FontRole::Title.atlas_name()).unwrap();
        style.font = explicit_title;
        style.bind_default_named_fonts(&atlas);
        assert_eq!(style.font, explicit_title);
    }
}
