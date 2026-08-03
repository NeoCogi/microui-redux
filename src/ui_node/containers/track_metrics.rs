use crate::Style;

/// Returns the content-independent fallback width used by explicit empty Grid tracks.
pub(super) fn default_cell_width(style: &Style) -> i32 {
    style.default_cell_width.saturating_add(style.padding.max(0) * 2).max(0)
}

/// Returns the font-derived fallback height used by empty Row and Grid tracks.
pub(super) fn default_cell_height(style: &Style, atlas: &crate::AtlasHandle) -> i32 {
    let padding = style.padding.max(0);
    (atlas.get_font_height(style.font) as i32).saturating_add(padding * 2).max(padding * 2)
}
