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
//! UI style values and compatibility helpers used across the crate.

use crate::atlas::{AtlasHandle, FontId};
use super::{Color, FontChoice, FontRole, ThemeIcons};

/// Style-resolved border appearance for outer and internal frames.
#[derive(Copy, Clone)]
pub(crate) struct FrameBorder {
    pub(crate) width: i32,
    pub(crate) color: Color,
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
    /// Semantic icons used by built-in widgets and chrome.
    pub icons: ThemeIcons,
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
    /// Minimum length of scrollbar thumbs and width of slider thumbs.
    pub thumb_size: i32,
    /// Width of inside-aligned widget and container borders.
    pub frame_border_width: i32,
    /// Palette of [`crate::ControlColor`] entries.
    pub colors: [Color; 14],
}

impl Default for Style {
    fn default() -> Self {
        Self {
            font: FontId::default(),
            small_font: FontId::default(),
            title_font: FontId::default(),
            heading_font: FontId::default(),
            mono_font: FontId::default(),
            icons: ThemeIcons::default(),
            default_cell_width: 68,
            padding: 5,
            spacing: 4,
            indent: 24,
            title_height: 24,
            scrollbar_size: 12,
            thumb_size: 8,
            frame_border_width: 1,
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

impl Style {
    pub(crate) fn frame_border(&self) -> FrameBorder {
        FrameBorder {
            width: self.frame_border_width.max(0),
            color: self.colors[crate::ControlColor::Border as usize],
        }
    }

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
    /// This is intended for compatibility paths such as [`crate::Context::set_style`], where callers
    /// often start from [`Style::default`] and only tweak colors or spacing. Explicit non-default
    /// font IDs are preserved.
    pub fn bind_default_named_fonts(&mut self, atlas: &AtlasHandle) {
        let default_font = FontId::default();
        if self.font == default_font
            && let Some(font) = atlas.font_id(FontRole::Body.atlas_name())
        {
            self.font = font;
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

    /// Binds default semantic icon roles from conventional atlas names.
    ///
    /// Explicit icon selections that differ from the default fixed IDs are preserved.
    pub fn bind_default_named_icons(&mut self, atlas: &AtlasHandle) {
        self.icons.bind_default_named(atlas);
    }

    /// Returns a copy of the style with semantic font roles rebound from `atlas`.
    pub fn with_named_fonts(mut self, atlas: &AtlasHandle) -> Self {
        self.bind_named_fonts(atlas);
        self
    }

    /// Returns a copy with both semantic font and icon roles rebound from `atlas`.
    pub fn with_named_assets(mut self, atlas: &AtlasHandle) -> Self {
        self.bind_named_assets(atlas);
        self
    }

    /// Binds semantic font and icon roles from their conventional atlas names.
    pub fn bind_named_assets(&mut self, atlas: &AtlasHandle) {
        self.bind_named_fonts(atlas);
        self.icons.bind_named(atlas);
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

#[cfg(test)]
mod tests {
    use super::*;
    use crate::test_support::test_atlas_with_font_sizes as make_test_atlas;

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

    #[test]
    fn frame_border_resolves_geometry_and_color_without_role_policy() {
        let style = Style {
            frame_border_width: -4,
            ..Style::default()
        };
        let border = style.frame_border();
        let expected = style.colors[crate::ControlColor::Border as usize];

        assert_eq!(border.width, 0);
        assert_eq!(
            (border.color.r, border.color.g, border.color.b, border.color.a),
            (expected.r, expected.g, expected.b, expected.a)
        );
    }
}
