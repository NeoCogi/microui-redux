use crate::render::Painter;
use crate::{Color, Dimensioni, Recti, Style};

/// Style-resolved border appearance for outer and internal frames.
#[derive(Copy, Clone)]
pub(crate) struct FrameBorder {
    pub(crate) width: i32,
    pub(crate) color: Color,
}

/// Geometry derived from one authoritative outer allocation.
#[derive(Copy, Clone, Debug)]
pub(crate) struct FrameGeometry {
    pub(crate) outer: Recti,
    pub(crate) content: Option<Recti>,
}

impl FrameGeometry {
    /// Returns a non-negative rectangle suitable for layout/traversal when no content survives.
    pub(crate) fn content_or_empty(self) -> Recti {
        self.content.unwrap_or_else(|| Recti::new(self.outer.x, self.outer.y, 0, 0))
    }
}

/// Resolves the border box and its derived content rectangle.
pub(crate) fn frame_geometry(outer: Recti, framed: bool, style: &Style) -> FrameGeometry {
    let border_width = if framed { style.frame_border().width } else { 0 };
    let content = if outer.width <= 0 || outer.height <= 0 {
        None
    } else if border_width == 0 {
        Some(outer)
    } else {
        checked_inset(outer, border_width)
    };
    FrameGeometry { outer, content }
}

/// Removes a frame from a positive measurement bound while preserving the intrinsic `0` marker.
pub(crate) fn content_available(available: Dimensioni, border_width: i32) -> Dimensioni {
    Dimensioni::new(
        inset_available(available.width, border_width.saturating_mul(2)),
        inset_available(available.height, border_width.saturating_mul(2)),
    )
}

/// Adds a resolved frame to positive preferred content dimensions.
pub(crate) fn outer_preferred(preferred: Dimensioni, border_width: i32) -> Dimensioni {
    Dimensioni::new(
        expand_positive_axis(preferred.width, border_width),
        expand_positive_axis(preferred.height, border_width),
    )
}

/// Paints a checked inside-aligned internal frame and returns its usable interior.
pub(crate) fn paint_internal_frame(painter: &mut Painter<'_>, outer: Recti, fill: Option<Color>, border: FrameBorder) -> Option<Recti> {
    let mut content = None;
    painter.with_clip(outer, |painter| {
        content = paint_clipped_frame(painter, outer, fill, border);
    });
    content
}

fn paint_clipped_frame(painter: &mut Painter<'_>, outer: Recti, fill: Option<Color>, border: FrameBorder) -> Option<Recti> {
    if outer.width <= 0 || outer.height <= 0 {
        return None;
    }

    let width = border.width.max(0);
    if width == 0 {
        if let Some(fill) = fill.filter(|color| color.a != 0) {
            painter.fill_rect(outer, fill);
        }
        return Some(outer);
    }

    let Some(content) = checked_inset(outer, width) else {
        if border.color.a != 0 {
            painter.fill_rect(outer, border.color);
        }
        return None;
    };

    if border.color.a != 0 {
        let top = Recti::new(outer.x, outer.y, outer.width, width);
        let bottom_y = checked_add(outer.y, outer.height - width)?;
        let bottom = Recti::new(outer.x, bottom_y, outer.width, width);
        let middle_y = checked_add(outer.y, width)?;
        let left = Recti::new(outer.x, middle_y, width, content.height);
        let right_x = checked_add(outer.x, outer.width - width)?;
        let right = Recti::new(right_x, middle_y, width, content.height);
        painter.fill_rect(top, border.color);
        painter.fill_rect(bottom, border.color);
        painter.fill_rect(left, border.color);
        painter.fill_rect(right, border.color);
    }

    if let Some(fill) = fill.filter(|color| color.a != 0) {
        painter.fill_rect(content, fill);
    }
    Some(content)
}

fn checked_inset(outer: Recti, width: i32) -> Option<Recti> {
    let width = width.max(0);
    let twice = width.checked_mul(2)?;
    if twice >= outer.width || twice >= outer.height {
        return None;
    }
    Some(Recti::new(
        checked_add(outer.x, width)?,
        checked_add(outer.y, width)?,
        outer.width.checked_sub(twice)?,
        outer.height.checked_sub(twice)?,
    ))
}

/// Removes a non-negative inset from a measurement axis without losing the `0 == unbounded` marker.
fn inset_available(value: i32, inset: i32) -> i32 {
    // Positive bounds stay positive because downstream measurement reserves zero for unbounded.
    if value > 0 { value.saturating_sub(inset.max(0)).max(1) } else { 0 }
}

fn expand_positive_axis(value: i32, border_width: i32) -> i32 {
    if value <= 0 {
        value
    } else {
        let outset = i64::from(border_width.max(0)) * 2;
        i32::try_from(i64::from(value) + outset).expect("framed preferred size overflowed i32")
    }
}

fn checked_add(left: i32, right: i32) -> Option<i32> {
    i32::try_from(i64::from(left) + i64::from(right)).ok()
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::color;
    use crate::render::DisplayList;

    #[test]
    fn frame_geometry_derives_inside_content() {
        let mut style = Style::default();
        style.frame_border_width = 2;
        let geometry = frame_geometry(Recti::new(10, 20, 30, 40), true, &style);
        assert_eq!(rect_tuple(geometry.outer), (10, 20, 30, 40));
        assert_eq!(geometry.content.map(rect_tuple), Some((12, 22, 26, 36)));
    }

    #[test]
    fn zero_width_frame_keeps_the_complete_content_rect() {
        let mut style = Style::default();
        style.frame_border_width = 0;
        let outer = Recti::new(10, 20, 30, 40);
        let geometry = frame_geometry(outer, true, &style);
        assert_eq!(geometry.content.map(rect_tuple), Some(rect_tuple(outer)));
    }

    #[test]
    fn transparent_border_keeps_structural_inset() {
        let mut style = Style::default();
        style.frame_border_width = 1;
        style.colors[crate::ControlColor::Border as usize] = color(0, 0, 0, 0);
        assert_eq!(frame_geometry(Recti::new(4, 5, 8, 7), true, &style).content.map(rect_tuple), Some((5, 6, 6, 5)));
    }

    #[test]
    fn tiny_frame_has_no_content() {
        let style = Style::default();
        assert!(frame_geometry(Recti::new(7, 8, 2, 10), true, &style).content.is_none());
    }

    #[test]
    fn preferred_measurement_preserves_non_positive_results() {
        let preferred = outer_preferred(Dimensioni::new(10, -1), 2);
        assert_eq!((preferred.width, preferred.height), (14, -1));
    }

    #[test]
    fn internal_frame_records_disjoint_inside_border_and_fill() {
        let outer = Recti::new(10, 20, 8, 7);
        let viewport = Recti::new(0, 0, 100, 100);
        let border = color(1, 2, 3, 255);
        let fill = color(4, 5, 6, 255);
        let mut list = DisplayList::new();
        let mut painter = Painter::screen_space(&mut list, viewport);

        let content = paint_internal_frame(&mut painter, outer, Some(fill), FrameBorder { width: 1, color: border });
        assert_eq!(content.map(rect_tuple), Some((11, 21, 6, 5)));

        let recorded = list.debug_fill_rects();
        assert_eq!(recorded.len(), 5);
        assert_eq!(
            recorded.iter().map(|(rect, _, _)| rect_tuple(*rect)).collect::<Vec<_>>(),
            vec![(10, 20, 8, 1), (10, 26, 8, 1), (10, 21, 1, 5), (17, 21, 1, 5), (11, 21, 6, 5)]
        );
        assert!(recorded[..4].iter().all(|(_, _, color)| color_tuple(*color) == color_tuple(border)));
        assert_eq!(color_tuple(recorded[4].2), color_tuple(fill));
    }

    #[test]
    fn tiny_internal_frame_records_border_once_without_fill() {
        let outer = Recti::new(3, 4, 2, 9);
        let viewport = Recti::new(0, 0, 100, 100);
        let border = color(7, 8, 9, 255);
        let mut list = DisplayList::new();
        let mut painter = Painter::screen_space(&mut list, viewport);

        assert!(paint_internal_frame(&mut painter, outer, Some(color(10, 11, 12, 255)), FrameBorder { width: 1, color: border }).is_none());
        let recorded = list.debug_fill_rects();
        assert_eq!(recorded.len(), 1);
        assert_eq!(rect_tuple(recorded[0].0), rect_tuple(outer));
        assert_eq!(color_tuple(recorded[0].2), color_tuple(border));
    }

    #[test]
    fn transparent_internal_border_insets_without_recording_border_pixels() {
        let outer = Recti::new(5, 6, 9, 8);
        let viewport = Recti::new(0, 0, 100, 100);
        let fill = color(20, 30, 40, 255);
        let mut list = DisplayList::new();
        let mut painter = Painter::screen_space(&mut list, viewport);

        let content = paint_internal_frame(&mut painter, outer, Some(fill), FrameBorder { width: 2, color: color(0, 0, 0, 0) });
        assert_eq!(content.map(rect_tuple), Some((7, 8, 5, 4)));
        let recorded = list.debug_fill_rects();
        assert_eq!(recorded.len(), 1);
        assert_eq!(rect_tuple(recorded[0].0), (7, 8, 5, 4));
    }

    fn rect_tuple(rect: Recti) -> (i32, i32, i32, i32) {
        (rect.x, rect.y, rect.width, rect.height)
    }

    fn color_tuple(color: Color) -> (u8, u8, u8, u8) {
        (color.r, color.g, color.b, color.a)
    }
}
