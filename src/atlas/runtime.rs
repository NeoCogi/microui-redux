//! Runtime atlas handle lookup, text metrics, and mutable slot rendering.

use super::*;

impl AtlasHandle {
    /// Returns the atlas texture width in pixels.
    pub fn width(&self) -> usize {
        self.0.borrow().width
    }
    /// Returns the atlas texture height in pixels.
    pub fn height(&self) -> usize {
        self.0.borrow().height
    }
    /// Returns a clone of the atlas pixel data.
    pub fn pixels_clone(&self) -> Vec<Color4b> {
        self.0.borrow().pixels.clone()
    }

    /// Executes a closure with shared access to the atlas pixels.
    pub fn apply_pixels<F: FnMut(usize, usize, &Vec<Color4b>)>(&self, mut f: F) {
        let s = self.0.borrow();
        f(s.width, s.height, &s.pixels);
    }

    /// Returns a mapping from icon names to their identifiers.
    pub fn clone_icon_table(&self) -> Vec<(String, IconId)> {
        self.0.borrow().icons.iter().enumerate().map(|(i, icon)| (icon.0.clone(), IconId(i))).collect()
    }

    /// Returns a mapping from font names to their identifiers.
    pub fn clone_font_table(&self) -> Vec<(String, FontId)> {
        self.0.borrow().fonts.iter().enumerate().map(|(i, font)| (font.0.clone(), FontId(i))).collect()
    }

    /// Looks up a font by its stored atlas name.
    pub fn font_id(&self, name: &str) -> Option<FontId> {
        self.0
            .borrow()
            .fonts
            .iter()
            .enumerate()
            .find_map(|(idx, (font_name, _))| (font_name == name).then_some(FontId(idx)))
    }

    /// Returns a list of available slot identifiers.
    pub fn clone_slot_table(&self) -> Vec<SlotId> {
        self.0.borrow().slots.iter().enumerate().map(|(i, _)| SlotId(i)).collect()
    }

    /// Returns glyph metrics for the specified character, if available.
    pub fn get_char_entry(&self, font: FontId, c: char) -> Option<CharEntry> {
        self.0.borrow().fonts[font.0].1.entries.get(&c).cloned()
    }

    /// Returns the line height for the specified font.
    pub fn get_font_height(&self, font: FontId) -> usize {
        self.0.borrow().fonts[font.0].1.line_size
    }

    /// Returns the baseline offset (in pixels) for the specified font.
    pub fn get_font_baseline(&self, font: FontId) -> i32 {
        self.0.borrow().fonts[font.0].1.baseline
    }

    /// Returns the baked pixel size requested for the specified font.
    pub fn get_font_size(&self, font: FontId) -> usize {
        self.0.borrow().fonts[font.0].1.font_size
    }

    /// Returns the dimensions of an icon.
    pub fn get_icon_size(&self, icon: IconId) -> Dimensioni {
        let r = self.0.borrow().icons[icon.0].1.rect;
        Dimensioni::new(r.width, r.height)
    }

    /// Returns the atlas rectangle storing an icon.
    pub fn get_icon_rect(&self, icon: IconId) -> Recti {
        self.0.borrow().icons[icon.0].1.rect
    }

    /// Returns the dimensions of a slot.
    pub fn get_slot_size(&self, slot: SlotId) -> Dimensioni {
        let r = self.0.borrow().slots[slot.0];
        Dimension::new(r.width, r.height)
    }

    /// Returns the atlas rectangle storing a slot.
    pub(crate) fn get_slot_rect(&self, slot: SlotId) -> Recti {
        self.0.borrow().slots[slot.0]
    }

    /// Returns the atlas texture dimensions.
    pub fn get_texture_dimension(&self) -> Dimensioni {
        Dimension::new(self.0.borrow().width as _, self.0.borrow().height as _)
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

    /// Walks each glyph in the string and invokes the closure with draw information.
    pub fn draw_string<DrawFunction: FnMut(char, Vec2i, Recti, Recti)>(&self, font: FontId, text: &str, mut f: DrawFunction) {
        self.walk_glyphs(font, text, |chr, advance, dst, src, _| f(chr, advance, dst, src));
    }

    /// Measures the bounding box of the provided text.
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

    /// Renders into a slot outside an active frame and reserves a new atlas version first.
    pub fn render_slot(&self, slot: SlotId, f: Rc<dyn Fn(usize, usize) -> Color4b>) -> Result<(), AtlasMutationError> {
        if self.0.active_frame_readers.get() != 0 {
            return Err(AtlasMutationError::FrameActive);
        }

        let mut atlas = self.0.data.try_borrow_mut().map_err(|_| AtlasMutationError::BorrowConflict)?;
        let slot_rect = atlas.slots.get(slot.0).copied().ok_or(AtlasMutationError::UnknownSlot(slot))?;
        atlas.last_update_id = atlas.last_update_id.checked_add(1).ok_or(AtlasMutationError::VersionExhausted)?;

        let width = atlas.width;
        let height = atlas.height;
        let max_y = (slot_rect.y + slot_rect.height).min(height as i32);
        let max_x = (slot_rect.x + slot_rect.width).min(width as i32);
        for y in slot_rect.y.max(0)..max_y {
            for x in slot_rect.x.max(0)..max_x {
                let index = (x + y * (width as i32)) as usize;
                if index < atlas.pixels.len() {
                    atlas.pixels[index] = f(x as _, y as _)
                }
            }
        }
        Ok(())
    }

    /// Returns a monotonically increasing value that changes whenever slot pixels are modified.
    pub fn get_last_update_id(&self) -> u64 {
        self.0.borrow().last_update_id
    }
}
