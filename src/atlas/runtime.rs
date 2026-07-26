//! Runtime immutable atlas lookup and text metrics.

use super::*;

impl AtlasHandle {
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

    /// Returns glyph metrics for the specified character, if available.
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
}
