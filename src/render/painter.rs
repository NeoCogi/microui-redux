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
//! Local-coordinate recording into an owned display list.

use super::{
    display_list::DisplayList,
    geometry::{bounds_for_line, bounds_for_points, translate_rect},
};
use crate::{
    atlas::{FontId, IconId},
    style::{Color, TextureId},
};
use rs_math3d::{Color4b, Recti, Vec2f, Vec2i, color4b};

/// Records backend-neutral drawing operations in local coordinates.
///
/// A Painter borrows exactly one [`DisplayList`]. It translates local primitives into screen
/// space, attaches the current effective screen-space clip to every operation, and tessellates
/// custom solid geometry without consulting style, input, atlas, Renderer, or RendererBackend state.
///
/// Custom widgets obtain a painter from their [`WidgetCtx`](crate::WidgetCtx):
///
/// ```
/// use microui_redux::prelude::*;
///
/// #[derive(Clone)]
/// struct Swatch {
///     options: WidgetOption,
/// }
///
/// impl Widget for Swatch {
///     fn widget_opt(&self) -> &WidgetOption {
///         &self.options
///     }
///
///     fn measure(
///         &self,
///         _style: &Style,
///         _atlas: &AtlasHandle,
///         _available: Dimensioni,
///     ) -> Dimensioni {
///         Dimensioni::new(48, 24)
///     }
///
///     fn update(
///         &mut self,
///         _ctx: &mut WidgetCtx<'_>,
///         _events: Vec<UiInputEvent>,
///     ) -> ResourceState {
///         ResourceState::NONE
///     }
///
///     fn paint(&mut self, ctx: &mut WidgetCtx<'_>) {
///         let mut painter = ctx.painter();
///         let bounds = painter.local_rect();
///         painter.fill_rect(bounds, color(42, 48, 60, 255));
///
///         let inset = Recti::new(2, 2, bounds.width - 4, bounds.height - 4);
///         painter.with_clip(inset, |painter| {
///             painter.stroke_line(
///                 Vec2f::new(0.0, 0.0),
///                 Vec2f::new(bounds.width as f32, bounds.height as f32),
///                 2.0,
///                 color(110, 190, 255, 255),
///             );
///         });
///     }
/// }
/// ```
pub struct Painter<'a> {
    /// Display list receiving operations.
    list: &'a mut DisplayList,
    /// Translation from local to screen coordinates.
    origin: Vec2i,
    /// Local drawable extent exposed to callers.
    local_bounds: Recti,
    /// Current effective clip in screen space.
    clip: Recti,
}

impl<'a> Painter<'a> {
    /// Creates a Painter with an explicit local-to-screen origin and effective screen-space clip.
    pub fn new(list: &'a mut DisplayList, origin: Vec2i, local_bounds: Recti, screen_clip: Recti) -> Self {
        Self {
            list,
            origin,
            local_bounds,
            clip: screen_clip,
        }
    }

    /// Returns the local drawable rectangle supplied at construction.
    pub fn local_rect(&self) -> Recti {
        self.local_bounds
    }

    /// Returns the current effective clip translated into local coordinates.
    pub fn current_clip_rect(&self) -> Recti {
        translate_rect(self.clip, Vec2i::new(self.origin.x.saturating_neg(), self.origin.y.saturating_neg()))
    }

    /// Records a semantic filled rectangle.
    pub fn fill_rect(&mut self, rect: Recti, color: Color) {
        if !drawable_rect(rect, color) {
            return;
        }
        let screen_rect = self.screen_rect(rect);
        if rects_overlap(screen_rect, self.clip) {
            self.list.push_fill_rect(self.clip, screen_rect, color);
        }
    }

    /// Records an inside-aligned rectangle outline with the requested integer width.
    pub fn stroke_rect(&mut self, rect: Recti, width: i32, color: Color) {
        if !drawable_rect(rect, color) || width <= 0 {
            return;
        }
        if width.saturating_mul(2) >= rect.width || width.saturating_mul(2) >= rect.height {
            self.fill_rect(rect, color);
            return;
        }

        let middle_height = rect.height.saturating_sub(width.saturating_mul(2));
        self.fill_rect(Recti::new(rect.x, rect.y, rect.width, width), color);
        self.fill_rect(
            Recti::new(rect.x, rect.y.saturating_add(rect.height).saturating_sub(width), rect.width, width),
            color,
        );
        self.fill_rect(Recti::new(rect.x, rect.y.saturating_add(width), width, middle_height), color);
        self.fill_rect(
            Recti::new(
                rect.x.saturating_add(rect.width).saturating_sub(width),
                rect.y.saturating_add(width),
                width,
                middle_height,
            ),
            color,
        );
    }

    /// Records one UTF-8 text run at a local position.
    ///
    /// Text measurement remains outside Painter; final glyph clipping is performed by Renderer.
    pub fn text(&mut self, font: FontId, text: &str, pos: Vec2i, color: Color) {
        if text.is_empty() || color.a == 0 || !clip_has_area(self.clip) {
            return;
        }
        self.list.push_text(self.clip, font, self.screen_pos(pos), color, text);
    }

    /// Records one atlas icon in a local rectangle.
    pub fn icon(&mut self, id: IconId, rect: Recti, color: Color) {
        if !drawable_rect(rect, color) {
            return;
        }
        let screen_rect = self.screen_rect(rect);
        if rects_overlap(screen_rect, self.clip) {
            self.list.push_icon(self.clip, id, screen_rect, color);
        }
    }

    /// Records one backend-owned external texture in a local rectangle.
    pub fn image(&mut self, id: TextureId, rect: Recti, color: Color) {
        if !drawable_rect(rect, color) {
            return;
        }
        let screen_rect = self.screen_rect(rect);
        if rects_overlap(screen_rect, self.clip) {
            self.list.push_image(self.clip, id, screen_rect, color);
        }
    }

    /// Tessellates and records one thick local line without clipping its generated triangles.
    pub fn stroke_line(&mut self, from: Vec2f, to: Vec2f, width: f32, color: Color) {
        if color.a == 0 || !clip_has_area(self.clip) {
            return;
        }

        let Some(local_bounds) = bounds_for_line(from, to, width) else {
            return;
        };
        if !rects_overlap(self.screen_rect(local_bounds), self.clip) {
            return;
        }
        let offset = Vec2f::new(self.origin.x as f32, self.origin.y as f32);
        self.list.push_line(self.clip, from, to, width, packed_color(color), offset);
    }

    /// Tessellates and records one simple local polygon without clipping its generated triangles.
    ///
    /// Convex polygons use a triangle fan; concave polygons use ear clipping. Degenerate,
    /// non-finite, and self-invalidating inputs safely emit no operation.
    pub fn fill_polygon(&mut self, points: &[Vec2f], color: Color) {
        if points.len() < 3 || color.a == 0 || !clip_has_area(self.clip) {
            return;
        }
        let Some(local_bounds) = bounds_for_points(points) else {
            return;
        };
        if !rects_overlap(self.screen_rect(local_bounds), self.clip) {
            return;
        }

        let offset = Vec2f::new(self.origin.x as f32, self.origin.y as f32);
        self.list.push_polygon(self.clip, points, packed_color(color), offset);
    }

    /// Executes `paint` with a clip narrowed by a local rectangle.
    ///
    /// The parent Painter is unchanged after the closure returns. There is no mutable clip stack,
    /// public push/pop pair, or Drop-time restoration behavior.
    pub fn with_clip(&mut self, rect: Recti, paint: impl FnOnce(&mut Painter<'_>)) {
        let screen_clip = self.screen_rect(rect);
        let effective =
            intersect_rects(self.clip, screen_clip).unwrap_or_else(|| Recti::new(self.clip.x.max(screen_clip.x), self.clip.y.max(screen_clip.y), 0, 0));
        let mut child = Painter {
            list: &mut *self.list,
            origin: self.origin,
            local_bounds: self.local_bounds,
            clip: effective,
        };
        paint(&mut child);
    }

    /// Converts a local integer position into screen space.
    fn screen_pos(&self, pos: Vec2i) -> Vec2i {
        Vec2i::new(pos.x.saturating_add(self.origin.x), pos.y.saturating_add(self.origin.y))
    }

    /// Converts a local rectangle into screen space.
    fn screen_rect(&self, rect: Recti) -> Recti {
        translate_rect(rect, self.origin)
    }
}

/// Returns a packed color for solid geometry.
fn packed_color(color: Color) -> Color4b {
    color4b(color.r, color.g, color.b, color.a)
}

/// Returns whether a rectangle and color describe visible geometry.
fn drawable_rect(rect: Recti, color: Color) -> bool {
    rect.width > 0 && rect.height > 0 && color.a > 0
}

/// Returns whether a clip contains positive area.
fn clip_has_area(clip: Recti) -> bool {
    clip.width > 0 && clip.height > 0
}

/// Returns whether two positive-area integer rectangles overlap.
fn rects_overlap(left: Recti, right: Recti) -> bool {
    if !clip_has_area(left) || !clip_has_area(right) {
        return false;
    }
    let left_x0 = left.x as i64;
    let left_y0 = left.y as i64;
    let left_x1 = left_x0 + left.width as i64;
    let left_y1 = left_y0 + left.height as i64;
    let right_x0 = right.x as i64;
    let right_y0 = right.y as i64;
    let right_x1 = right_x0 + right.width as i64;
    let right_y1 = right_y0 + right.height as i64;
    left_x0 < right_x1 && left_x1 > right_x0 && left_y0 < right_y1 && left_y1 > right_y0
}

/// Returns the positive-area intersection of two rectangles using overflow-safe edge arithmetic.
fn intersect_rects(left: Recti, right: Recti) -> Option<Recti> {
    if !clip_has_area(left) || !clip_has_area(right) {
        return None;
    }
    let x0 = (left.x as i64).max(right.x as i64);
    let y0 = (left.y as i64).max(right.y as i64);
    let x1 = (left.x as i64 + left.width as i64).min(right.x as i64 + right.width as i64);
    let y1 = (left.y as i64 + left.height as i64).min(right.y as i64 + right.height as i64);
    if x0 >= x1 || y0 >= y1 {
        return None;
    }
    Some(Recti::new(
        x0 as i32,
        y0 as i32,
        (x1 - x0).min(i32::MAX as i64) as i32,
        (y1 - y0).min(i32::MAX as i64) as i32,
    ))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{color, TextureId};
    use super::super::display_list::DrawKind;

    fn rect_tuple(rect: Recti) -> (i32, i32, i32, i32) {
        (rect.x, rect.y, rect.width, rect.height)
    }

    #[test]
    fn fill_rect_records_a_translated_semantic_operation() {
        let mut list = DisplayList::new();
        {
            let mut painter = Painter::new(&mut list, Vec2i::new(10, 20), Recti::new(0, 0, 100, 80), Recti::new(12, 22, 50, 40));
            painter.fill_rect(Recti::new(1, 2, 10, 12), color(1, 2, 3, 255));
        }

        assert_eq!(list.ops.len(), 1);
        assert!(list.solid_geometry.is_empty());
        assert_eq!(rect_tuple(list.ops[0].clip), (12, 22, 50, 40));
        let DrawKind::FillRect { rect, color } = &list.ops[0].kind else {
            panic!("expected a semantic rectangle");
        };
        assert_eq!(rect_tuple(*rect), (11, 22, 10, 12));
        assert_eq!((color.r, color.g, color.b, color.a), (1, 2, 3, 255));
    }

    #[test]
    fn scoped_nested_clips_narrow_without_mutating_the_parent() {
        let mut list = DisplayList::new();
        {
            let mut painter = Painter::new(&mut list, Vec2i::new(10, 20), Recti::new(0, 0, 50, 50), Recti::new(10, 20, 50, 50));
            assert_eq!(rect_tuple(painter.current_clip_rect()), (0, 0, 50, 50));
            painter.with_clip(Recti::new(5, 5, 20, 20), |painter| {
                assert_eq!(rect_tuple(painter.current_clip_rect()), (5, 5, 20, 20));
                painter.with_clip(Recti::new(10, 0, 20, 20), |painter| {
                    assert_eq!(rect_tuple(painter.current_clip_rect()), (10, 5, 15, 15));
                    painter.fill_rect(Recti::new(0, 0, 50, 50), color(255, 0, 0, 255));
                });
            });
            assert_eq!(rect_tuple(painter.current_clip_rect()), (0, 0, 50, 50));
            painter.fill_rect(Recti::new(0, 0, 2, 2), color(0, 255, 0, 255));
        }

        assert_eq!(list.ops.len(), 2);
        assert_eq!(rect_tuple(list.ops[0].clip), (20, 25, 15, 15));
        assert_eq!(rect_tuple(list.ops[1].clip), (10, 20, 50, 50));
    }

    #[test]
    fn disjoint_clip_scope_cannot_expand_visibility() {
        let mut list = DisplayList::new();
        {
            let mut painter = Painter::new(&mut list, Vec2i::new(10, 20), Recti::new(0, 0, 50, 50), Recti::new(10, 20, 50, 50));
            painter.with_clip(Recti::new(100, 100, 10, 10), |painter| {
                assert_eq!(painter.current_clip_rect().width, 0);
                painter.with_clip(Recti::new(0, 0, 500, 500), |painter| {
                    assert_eq!(painter.current_clip_rect().width, 0);
                    painter.fill_rect(Recti::new(0, 0, 500, 500), color(255, 0, 0, 255));
                });
            });
        }
        assert!(list.is_empty());
    }

    #[test]
    fn line_triangles_remain_unclipped_and_carry_the_effective_clip() {
        let mut list = DisplayList::new();
        {
            let mut painter = Painter::new(&mut list, Vec2i::new(0, 0), Recti::new(0, 0, 20, 20), Recti::new(0, 0, 5, 5));
            painter.stroke_line(Vec2f::new(-10.0, 2.0), Vec2f::new(20.0, 2.0), 2.0, color(255, 0, 0, 255));
        }

        assert_eq!(list.ops.len(), 1);
        assert_eq!(list.solid_geometry.triangles().len(), 2);
        assert_eq!(rect_tuple(list.ops[0].clip), (0, 0, 5, 5));
        let positions = list
            .solid_geometry
            .triangles()
            .iter()
            .flat_map(|triangle| triangle.vertices())
            .map(|vertex| vertex.position)
            .collect::<Vec<_>>();
        assert!(positions.iter().any(|position| position.x < 0.0));
        assert!(positions.iter().any(|position| position.x > 5.0));
    }

    #[test]
    fn polygon_and_line_geometry_use_triangle_ranges() {
        let mut list = DisplayList::new();
        {
            let mut painter = Painter::new(&mut list, Vec2i::new(5, 7), Recti::new(0, 0, 100, 100), Recti::new(0, 0, 200, 200));
            painter.fill_polygon(
                &[Vec2f::new(0.0, 0.0), Vec2f::new(10.0, 0.0), Vec2f::new(10.0, 10.0), Vec2f::new(0.0, 10.0)],
                color(255, 255, 255, 255),
            );
            painter.stroke_line(Vec2f::new(0.0, 20.0), Vec2f::new(10.0, 20.0), 2.0, color(255, 255, 255, 255));
        }

        assert_eq!(list.ops.len(), 1, "adjacent compatible geometry should merge");
        assert_eq!(list.solid_geometry.triangles().len(), 4);
        let DrawKind::SolidTriangles { triangles } = &list.ops[0].kind else {
            panic!("expected a solid-triangle range");
        };
        assert_eq!(triangles.as_range(), 0..4);
        assert!(
            list.solid_geometry
                .triangles()
                .iter()
                .flat_map(|triangle| triangle.vertices())
                .all(|vertex| vertex.position.x >= 5.0 && vertex.position.y >= 7.0)
        );
    }

    #[test]
    fn stroke_rect_records_fill_rectangles_not_triangle_geometry() {
        let mut list = DisplayList::new();
        {
            let mut painter = Painter::new(&mut list, Vec2i::new(0, 0), Recti::new(0, 0, 20, 20), Recti::new(0, 0, 20, 20));
            painter.stroke_rect(Recti::new(2, 2, 10, 10), 2, color(255, 255, 255, 255));
        }
        assert_eq!(list.ops.len(), 4);
        assert!(list.ops.iter().all(|operation| matches!(operation.kind, DrawKind::FillRect { .. })));
        assert!(list.solid_geometry.is_empty());
    }

    #[test]
    fn semantic_primitives_translate_and_reject_fully_hidden_bounds() {
        let mut list = DisplayList::new();
        {
            let mut painter = Painter::new(&mut list, Vec2i::new(10, 20), Recti::new(0, 0, 100, 100), Recti::new(10, 20, 20, 20));
            painter.text(FontId::default(), "label", Vec2i::new(1, 2), color(255, 255, 255, 255));
            painter.icon(IconId::default(), Recti::new(2, 3, 4, 5), color(255, 255, 255, 255));
            painter.image(TextureId::new(1, 10, 10), Recti::new(100, 100, 10, 10), color(255, 255, 255, 255));
        }

        assert_eq!(list.ops.len(), 2);
        let DrawKind::Text { pos, .. } = &list.ops[0].kind else {
            panic!("expected text");
        };
        assert_eq!((pos.x, pos.y), (11, 22));
        let DrawKind::Icon { rect, .. } = &list.ops[1].kind else {
            panic!("expected icon");
        };
        assert_eq!(rect_tuple(*rect), (12, 23, 4, 5));
    }

    #[test]
    fn fully_hidden_and_degenerate_solid_geometry_is_rejected() {
        let mut list = DisplayList::new();
        {
            let mut painter = Painter::new(&mut list, Vec2i::new(0, 0), Recti::new(0, 0, 20, 20), Recti::new(0, 0, 20, 20));
            assert_eq!(rect_tuple(painter.local_rect()), (0, 0, 20, 20));
            painter.stroke_line(Vec2f::new(100.0, 100.0), Vec2f::new(120.0, 100.0), 2.0, color(255, 255, 255, 255));
            painter.fill_polygon(
                &[Vec2f::new(100.0, 100.0), Vec2f::new(120.0, 100.0), Vec2f::new(110.0, 120.0)],
                color(255, 255, 255, 255),
            );
            painter.fill_polygon(&[Vec2f::new(0.0, 0.0), Vec2f::new(1.0, 1.0), Vec2f::new(2.0, 2.0)], color(255, 255, 255, 255));
        }
        assert!(list.is_empty());
    }
}
