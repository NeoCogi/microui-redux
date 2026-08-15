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

//! Runtime immutable atlas lookup and text metrics.

use super::*;

impl AtlasHandle {
    /// Reports whether two handles reference the same immutable atlas allocation.
    pub(crate) fn ptr_eq(&self, other: &Self) -> bool {
        Rc::ptr_eq(&self.0, &other.0)
    }

    /// Returns the atlas texture width in pixels.
    pub fn width(&self) -> usize {
        self.0.width
    }
    /// Returns the atlas texture height in pixels.
    pub fn height(&self) -> usize {
        self.0.height
    }
    /// Returns a clone of the atlas pixel data.
    pub fn pixels_clone(&self) -> Vec<Color4b> {
        self.0.pixels.clone()
    }

    /// Executes a closure with shared access to the atlas pixels.
    pub fn apply_pixels<F: FnMut(usize, usize, &[Color4b])>(&self, mut f: F) {
        f(self.0.width, self.0.height, &self.0.pixels);
    }

    /// Returns a mapping from icon names to their identifiers.
    pub fn clone_icon_table(&self) -> Vec<(String, IconId)> {
        self.0.icons.iter().enumerate().map(|(i, icon)| (icon.0.clone(), IconId(i))).collect()
    }

    /// Returns a mapping from font names to their identifiers.
    pub fn clone_font_table(&self) -> Vec<(String, FontId)> {
        self.0.fonts.iter().enumerate().map(|(i, font)| (font.0.clone(), FontId(i))).collect()
    }

    /// Looks up a font by its stored atlas name.
    pub fn font_id(&self, name: &str) -> Option<FontId> {
        self.0
            .fonts
            .iter()
            .enumerate()
            .find_map(|(idx, (font_name, _))| (font_name == name).then_some(FontId(idx)))
    }

    /// Looks up an icon by its stored atlas name.
    pub fn icon_id(&self, name: &str) -> Option<IconId> {
        self.0
            .icons
            .iter()
            .enumerate()
            .find_map(|(idx, (icon_name, _))| (icon_name == name).then_some(IconId(idx)))
    }

    /// Returns exact glyph metrics for the specified character, if available.
    ///
    /// This lookup does not apply the underscore fallback used by [`AtlasHandle::draw_string`] and
    /// [`AtlasHandle::get_text_size`].
    pub fn get_char_entry(&self, font: FontId, c: char) -> Option<CharEntry> {
        self.0.fonts[font.0].1.entries.get(&c).cloned()
    }

    /// Returns the line height for the specified font.
    pub fn get_font_height(&self, font: FontId) -> usize {
        self.0.fonts[font.0].1.line_size
    }

    /// Returns the baseline offset (in pixels) for the specified font.
    pub fn get_font_baseline(&self, font: FontId) -> i32 {
        self.0.fonts[font.0].1.baseline
    }

    /// Returns the baked pixel size requested for the specified font.
    pub fn get_font_size(&self, font: FontId) -> usize {
        self.0.fonts[font.0].1.font_size
    }

    /// Returns the dimensions of an icon.
    pub fn get_icon_size(&self, icon: IconId) -> Dimensioni {
        let r = self.0.icons[icon.0].1.rect;
        Dimensioni::new(r.width, r.height)
    }

    /// Returns the atlas rectangle storing an icon.
    pub fn get_icon_rect(&self, icon: IconId) -> Recti {
        self.0.icons[icon.0].1.rect
    }

    /// Returns the atlas texture dimensions.
    pub fn get_texture_dimension(&self) -> Dimensioni {
        Dimension::new(self.0.width as _, self.0.height as _)
    }

    /// Internal helper that walks glyphs applying baseline-aware placement.
    fn walk_glyphs<F>(&self, font: FontId, text: &str, mut f: F)
    where
        F: FnMut(char, Vec2i, Recti, Recti, i32),
    {
        let mut dst = Recti { x: 0, y: 0, width: 0, height: 0 };
        let line_height = self.get_font_height(font) as i32;
        let baseline = self.get_font_baseline(font);
        let mut baseline_y = baseline;
        let mut pen_x = 0;

        for chr in text.chars() {
            if chr == '\n' || chr == '\r' {
                pen_x = 0;
                baseline_y += line_height;
                continue;
            }

            let src = self.get_char_entry(font, chr).or_else(|| self.get_char_entry(font, '_')).unwrap_or(CharEntry {
                offset: Vec2i::new(0, 0),
                advance: Vec2i::new(8, 0),
                rect: Recti::new(0, 0, 8, 8),
            });

            dst.width = src.rect.width;
            dst.height = src.rect.height;
            dst.x = pen_x + src.offset.x;
            dst.y = baseline_y - src.offset.y - src.rect.height;

            f(chr, src.advance, dst, src.rect, baseline_y);
            pen_x += src.advance.x;
        }
    }

    /// Walks each Unicode scalar value in the string and invokes the closure with draw information.
    ///
    /// Newline and carriage return advance to another line without invoking the closure. A missing
    /// character uses the selected font's `_` entry. If underscore is also absent, a synthetic
    /// 8-by-8 entry at the atlas origin is used. The callback still receives the original character.
    pub fn draw_string<DrawFunction: FnMut(char, Vec2i, Recti, Recti)>(&self, font: FontId, text: &str, mut f: DrawFunction) {
        self.walk_glyphs(font, text, |chr, advance, dst, src, _| f(chr, advance, dst, src));
    }

    /// Measures the bounding box of the provided UTF-8 text.
    ///
    /// Measurement uses the same newline and missing-character fallback rules as
    /// [`AtlasHandle::draw_string`].
    pub fn get_text_size(&self, font: FontId, text: &str) -> Dimensioni {
        let mut res = Dimensioni::new(0, 0);
        let line_height = self.get_font_height(font) as i32;
        let baseline = self.get_font_baseline(font);
        let descent = (line_height - baseline).max(0);
        let mut max_line_bottom = 0;
        let mut saw_glyph = false;

        self.walk_glyphs(font, text, |_, advance, dst, _, baseline_y| {
            saw_glyph = true;
            res.width = max(res.width, dst.x + max(advance.x, dst.width));
            res.height = max(res.height, dst.y + dst.height);
            max_line_bottom = max(max_line_bottom, baseline_y + descent);
        });

        if saw_glyph {
            res.height = max(res.height, max_line_bottom);
        }
        res
    }
}
