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

use crate::render::Painter;
use crate::theme::FrameBorder;
use crate::{AvailableSpace, Color, Constraints, Dimensioni, Recti, Style};

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

/// Removes a frame from each finite measurement bound while preserving unbounded axes.
pub(crate) fn content_constraints(constraints: Constraints, border_width: i32) -> Constraints {
    // border_extent = leading_border_width + trailing_border_width = border_width * 2.
    let border_extent = border_width.saturating_mul(2);
    Constraints::new(
        inset_available(constraints.width, border_extent),
        inset_available(constraints.height, border_extent),
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
        // bottom_y = outer_y + outer_height - border_width.
        let bottom_y = checked_add(outer.y, outer.height - width)?;
        let bottom = Recti::new(outer.x, bottom_y, outer.width, width);
        // middle_y = outer_y + border_width.
        let middle_y = checked_add(outer.y, width)?;
        let left = Recti::new(outer.x, middle_y, width, content.height);
        // right_x = outer_x + outer_width - border_width.
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
    // border_extent = leading_border_width + trailing_border_width = width * 2.
    let twice = width.checked_mul(2)?;
    if twice >= outer.width || twice >= outer.height {
        return None;
    }
    // content_origin = outer_origin + border_width.
    let x = checked_add(outer.x, width)?;
    let y = checked_add(outer.y, width)?;
    // content_extent = outer_extent - leading_border - trailing_border.
    let content_width = outer.width.checked_sub(twice)?;
    let content_height = outer.height.checked_sub(twice)?;
    Some(Recti::new(x, y, content_width, content_height))
}

/// Removes a non-negative inset without conflating a bounded zero with unbounded space.
fn inset_available(space: AvailableSpace, inset: i32) -> AvailableSpace {
    match space {
        AvailableSpace::Unbounded => AvailableSpace::Unbounded,
        AvailableSpace::Bounded(value) => AvailableSpace::bounded(value.saturating_sub(inset.max(0))),
    }
}

fn expand_positive_axis(value: i32, border_width: i32) -> i32 {
    if value <= 0 {
        value
    } else {
        // border_outset = leading_border_width + trailing_border_width.
        let outset = i64::from(border_width.max(0)) * 2;
        // outer_extent = content_extent + border_outset.
        i32::try_from(i64::from(value) + outset).expect("framed preferred size overflowed i32")
    }
}

fn checked_add(left: i32, right: i32) -> Option<i32> {
    // sum = left + right, rejected when it is outside the i32 range.
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
    fn frame_inset_preserves_zero_and_unbounded_as_distinct_constraints() {
        let constraints = Constraints::new(AvailableSpace::Bounded(1), AvailableSpace::Unbounded);
        assert_eq!(
            content_constraints(constraints, 2),
            Constraints::new(AvailableSpace::Bounded(0), AvailableSpace::Unbounded)
        );
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
