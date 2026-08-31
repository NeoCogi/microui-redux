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
        // Every exported capability copies this atlas's provenance instead of exposing a slot.
        self.0
            .icons
            .iter()
            .enumerate()
            .map(|(slot, icon)| (icon.0.clone(), IconId::new(self.0.id, slot)))
            .collect()
    }

    /// Returns a mapping from font names to their identifiers.
    pub fn clone_font_table(&self) -> Vec<(String, FontId)> {
        // Every exported capability copies this atlas's provenance instead of exposing a slot.
        self.0
            .fonts
            .iter()
            .enumerate()
            .map(|(slot, font)| (font.0.clone(), FontId::new(self.0.id, slot)))
            .collect()
    }

    /// Looks up a font by its stored atlas name.
    pub fn font_id(&self, name: &str) -> Option<FontId> {
        self.0
            .fonts
            .iter()
            .enumerate()
            .find_map(|(slot, (font_name, _))| (font_name == name).then_some(FontId::new(self.0.id, slot)))
    }

    /// Looks up an icon by its stored atlas name.
    pub fn icon_id(&self, name: &str) -> Option<IconId> {
        self.0
            .icons
            .iter()
            .enumerate()
            .find_map(|(slot, (icon_name, _))| (icon_name == name).then_some(IconId::new(self.0.id, slot)))
    }

    /// Returns the named opaque white tile used for solid rendering.
    ///
    /// # Panics
    ///
    /// Panics when this atlas does not contain the required exact `white` name.
    pub fn white_icon(&self) -> IconId {
        // Resolve by semantic name rather than relying on a positional global constant.
        self.icon_id("white").expect("atlas must contain an icon named `white`")
    }

    /// Reports whether `font` names a live table entry owned by this atlas allocation.
    pub(crate) fn contains_font(&self, font: FontId) -> bool {
        // Checking both provenance and bounds prevents same-slot IDs from another atlas aliasing.
        font.atlas == self.0.id && font.slot < self.0.fonts.len()
    }

    /// Reports whether `icon` names a live table entry owned by this atlas allocation.
    pub(crate) fn contains_icon(&self, icon: IconId) -> bool {
        // Checking both provenance and bounds prevents same-slot IDs from another atlas aliasing.
        icon.atlas == self.0.id && icon.slot < self.0.icons.len()
    }

    /// Resolves a font capability after enforcing atlas ownership and slot bounds.
    fn font(&self, font: FontId) -> &Font {
        // Centralizing this assertion ensures no public metrics path can accidentally index only by
        // the local slot and silently accept a foreign capability.
        assert!(self.contains_font(font), "font ID does not belong to this atlas: {font:?}");
        &self.0.fonts[font.slot].1
    }

    /// Resolves an icon capability after enforcing atlas ownership and slot bounds.
    fn icon(&self, icon: IconId) -> &Icon {
        // Centralizing this assertion gives direct atlas users the same non-aliasing guarantee as
        // renderer preflight, while keeping ordinary metric getters allocation-free.
        assert!(self.contains_icon(icon), "icon ID does not belong to this atlas: {icon:?}");
        &self.0.icons[icon.slot].1
    }

    /// Returns exact glyph metrics for the specified character, if available.
    ///
    /// This lookup does not apply the underscore fallback used by [`AtlasHandle::draw_string`] and
    /// [`AtlasHandle::get_text_size`].
    ///
    /// # Panics
    ///
    /// Panics when `font` was minted by another atlas allocation or does not identify a live font
    /// in this atlas.
    pub fn get_char_entry(&self, font: FontId, c: char) -> Option<CharEntry> {
        self.font(font).entries.get(&c).cloned()
    }

    /// Returns the line height for the specified font.
    ///
    /// # Panics
    ///
    /// Panics when `font` was minted by another atlas allocation or does not identify a live font
    /// in this atlas.
    pub fn get_font_height(&self, font: FontId) -> usize {
        self.font(font).line_size
    }

    /// Returns the baseline offset (in pixels) for the specified font.
    ///
    /// # Panics
    ///
    /// Panics when `font` was minted by another atlas allocation or does not identify a live font
    /// in this atlas.
    pub fn get_font_baseline(&self, font: FontId) -> i32 {
        self.font(font).baseline
    }

    /// Returns the baked pixel size requested for the specified font.
    ///
    /// # Panics
    ///
    /// Panics when `font` was minted by another atlas allocation or does not identify a live font
    /// in this atlas.
    pub fn get_font_size(&self, font: FontId) -> usize {
        self.font(font).font_size
    }

    /// Returns the dimensions of an icon.
    ///
    /// # Panics
    ///
    /// Panics when `icon` was minted by another atlas allocation or does not identify a live icon
    /// in this atlas.
    pub fn get_icon_size(&self, icon: IconId) -> Dimensioni {
        let r = self.icon(icon).rect;
        Dimensioni::new(r.width, r.height)
    }

    /// Returns the atlas rectangle storing an icon.
    ///
    /// # Panics
    ///
    /// Panics when `icon` was minted by another atlas allocation or does not identify a live icon
    /// in this atlas.
    pub fn get_icon_rect(&self, icon: IconId) -> Recti {
        self.icon(icon).rect
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
        // Validate atlas provenance once before the loop, then reuse the concrete font record for
        // every glyph lookup and line metric.
        let font = self.font(font);
        let mut dst = Recti { x: 0, y: 0, width: 0, height: 0 };
        let line_height = font.line_size as i32;
        let baseline = font.baseline;
        let mut baseline_y = baseline;
        let mut pen_x = 0;

        for chr in text.chars() {
            if chr == '\n' || chr == '\r' {
                pen_x = 0;
                baseline_y += line_height;
                continue;
            }

            let src = font.entries.get(&chr).or_else(|| font.entries.get(&'_')).cloned().unwrap_or(CharEntry {
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
    ///
    /// # Panics
    ///
    /// Panics when `font` was minted by another atlas allocation or does not identify a live font
    /// in this atlas.
    pub fn draw_string<DrawFunction: FnMut(char, Vec2i, Recti, Recti)>(&self, font: FontId, text: &str, mut f: DrawFunction) {
        self.walk_glyphs(font, text, |chr, advance, dst, src, _| f(chr, advance, dst, src));
    }

    /// Measures the bounding box of the provided UTF-8 text.
    ///
    /// Measurement uses the same newline and missing-character fallback rules as
    /// [`AtlasHandle::draw_string`].
    ///
    /// # Panics
    ///
    /// Panics when `font` was minted by another atlas allocation or does not identify a live font
    /// in this atlas.
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

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{AtlasSource, FontEntry, SourceFormat};

    /// Rehydrates one minimal valid atlas so each call creates a distinct ownership domain.
    fn make_atlas() -> AtlasHandle {
        let pixels = [0xFF, 0xFF, 0xFF, 0xFF];
        let icons = [("white", Recti::new(0, 0, 1, 1))];
        let entries = [(
            '_',
            CharEntry {
                offset: Vec2i::new(0, 0),
                advance: Vec2i::new(1, 0),
                rect: Recti::new(0, 0, 1, 1),
            },
        )];
        let fonts = [(
            "body",
            FontEntry {
                line_size: 1,
                baseline: 1,
                font_size: 1,
                entries: &entries,
            },
        )];
        // AtlasHandle copies all borrowed source data before these local tables leave scope.
        AtlasHandle::from(&AtlasSource {
            width: 1,
            height: 1,
            pixels: &pixels,
            icons: &icons,
            fonts: &fonts,
            format: SourceFormat::Raw,
        })
    }

    /// Verifies same-slot resource IDs are accepted only by their originating atlas allocation.
    #[test]
    fn resource_ids_are_bound_to_one_atlas_allocation() {
        let first = make_atlas();
        let first_clone = first.clone();
        let second = make_atlas();

        // Independently loaded identical metadata has matching table positions but distinct owners.
        let first_font = first.font_id("body").unwrap();
        let second_font = second.font_id("body").unwrap();
        let first_icon = first.white_icon();
        let second_icon = second.white_icon();
        assert_ne!(first_font, second_font);
        assert_ne!(first_icon, second_icon);

        // Clones share the exact immutable allocation and therefore accept equal capabilities.
        assert_eq!(first_clone.font_id("body"), Some(first_font));
        assert_eq!(first_clone.white_icon(), first_icon);
        assert!(first_clone.contains_font(first_font));
        assert!(first_clone.contains_icon(first_icon));
        assert!(!second.contains_font(first_font));
        assert!(!second.contains_icon(first_icon));

        // Table clones must stamp the same owner rather than exposing ownerless numeric slots.
        assert_eq!(first.clone_font_table(), vec![(String::from("body"), first_font)]);
        assert_eq!(first.clone_icon_table(), vec![(String::from("white"), first_icon)]);
    }

    /// Verifies direct metric lookup rejects foreign capabilities instead of aliasing their slots.
    #[test]
    fn foreign_resource_lookup_panics_before_indexing_a_same_slot_entry() {
        let first = make_atlas();
        let second = make_atlas();
        let foreign_font = first.font_id("body").unwrap();
        let foreign_icon = first.white_icon();

        // The central accessors use release assertions, so every direct lookup path rejects the
        // foreign owner even though the second atlas has entries at both local slot zeroes.
        let font_result = std::panic::catch_unwind(|| second.get_font_size(foreign_font));
        let icon_result = std::panic::catch_unwind(|| second.get_icon_rect(foreign_icon));
        assert!(font_result.is_err());
        assert!(icon_result.is_err());
    }
}
