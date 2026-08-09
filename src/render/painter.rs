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

use super::display_list::DisplayList;
use crate::{
    atlas::{FontId, IconId},
    math::RectExt,
    render::{Color, TextureId},
};
use rs_math3d::{Recti, Vec2f, Vec2i, color4b};

/// Records backend-neutral drawing operations in local coordinates.
///
/// A Painter borrows the crate-owned display list for the current frame. It translates local
/// primitives into screen space, attaches the current effective screen-space clip to every
/// operation, and tessellates custom solid geometry without consulting style, input, atlas,
/// Renderer, or RendererBackend state.
///
/// Custom widgets obtain a painter from their [`WidgetPaintCtx`](crate::WidgetPaintCtx):
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
///     fn update(
///         &mut self,
///         _ctx: &mut WidgetUpdateCtx<'_>,
///         _event: Option<&UiInputEvent>,
///     ) {}
///
///     fn paint(&mut self, ctx: &mut WidgetPaintCtx<'_>) {
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
///
/// impl LeafWidget for Swatch {
///     fn measure(
///         &self,
///         _style: &Style,
///         _atlas: &AtlasHandle,
///         _available: Dimensioni,
///     ) -> Dimensioni {
///         Dimensioni::new(48, 24)
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
    /// Creates a widget-local Painter from one authoritative screen-space content rectangle.
    pub(crate) fn for_widget(list: &'a mut DisplayList, screen_content_bounds: Recti, screen_clip: Recti) -> Self {
        let origin = Vec2i::new(screen_content_bounds.x, screen_content_bounds.y);
        let local_bounds = Recti::new(0, 0, screen_content_bounds.width, screen_content_bounds.height);
        Self {
            list,
            origin,
            local_bounds,
            clip: screen_clip,
        }
    }

    /// Creates an internal Painter whose input coordinates are already in screen space.
    pub(crate) fn screen_space(list: &'a mut DisplayList, screen_clip: Recti) -> Self {
        Self {
            list,
            origin: Vec2i::default(),
            local_bounds: screen_clip,
            clip: screen_clip,
        }
    }

    /// Returns the drawable rectangle in this Painter's coordinate space.
    pub fn local_rect(&self) -> Recti {
        self.local_bounds
    }

    /// Returns the current effective clip translated into local coordinates.
    pub fn current_clip_rect(&self) -> Recti {
        // local_clip_origin = screen_clip_origin - painter_origin.
        self.clip
            .saturating_translated(Vec2i::new(self.origin.x.saturating_neg(), self.origin.y.saturating_neg()))
    }

    /// Records a semantic filled rectangle.
    pub fn fill_rect(&mut self, rect: Recti, color: Color) {
        self.record_rect(rect, color, |list, clip, screen_rect| list.push_fill_rect(clip, screen_rect, color));
    }

    /// Records an inside-aligned rectangle outline with the requested integer width.
    pub fn stroke_rect(&mut self, rect: Recti, width: i32, color: Color) {
        if !rect.has_positive_area() || color.a == 0 || width <= 0 {
            return;
        }
        // border_extent = leading_border_width + trailing_border_width = width * 2.
        let border_extent = width.saturating_mul(2);
        if border_extent >= rect.width || border_extent >= rect.height {
            self.fill_rect(rect, color);
            return;
        }

        // middle_height = rectangle_height - top_border - bottom_border.
        let middle_height = rect.height.saturating_sub(border_extent);
        // bottom_y = rectangle_y + rectangle_height - border_width.
        let bottom_y = rect.y.saturating_add(rect.height).saturating_sub(width);
        // middle_y = rectangle_y + border_width.
        let middle_y = rect.y.saturating_add(width);
        // right_x = rectangle_x + rectangle_width - border_width.
        let right_x = rect.x.saturating_add(rect.width).saturating_sub(width);
        self.fill_rect(Recti::new(rect.x, rect.y, rect.width, width), color);
        self.fill_rect(Recti::new(rect.x, bottom_y, rect.width, width), color);
        self.fill_rect(Recti::new(rect.x, middle_y, width, middle_height), color);
        self.fill_rect(Recti::new(right_x, middle_y, width, middle_height), color);
    }

    /// Records one UTF-8 text run at a local position.
    ///
    /// Text measurement remains outside Painter; final glyph clipping is performed by Renderer.
    pub fn text(&mut self, font: FontId, text: &str, pos: Vec2i, color: Color) {
        if text.is_empty() || color.a == 0 || !self.clip.has_positive_area() {
            return;
        }
        self.list.push_text(self.clip, font, self.screen_pos(pos), color, text);
    }

    /// Records one atlas icon in a local rectangle.
    pub fn icon(&mut self, id: IconId, rect: Recti, color: Color) {
        self.record_rect(rect, color, |list, clip, screen_rect| list.push_icon(clip, id, screen_rect, color));
    }

    /// Records one backend-owned external texture in a local rectangle.
    pub fn image(&mut self, id: TextureId, rect: Recti, color: Color) {
        self.record_rect(rect, color, |list, clip, screen_rect| list.push_image(clip, id, screen_rect, color));
    }

    /// Tessellates and records one thick local line without clipping its generated triangles.
    pub fn stroke_line(&mut self, from: Vec2f, to: Vec2f, width: f32, color: Color) {
        if color.a == 0 || !self.clip.has_positive_area() {
            return;
        }

        let Some(local_bounds) = Recti::from_thick_line(from, to, width) else {
            return;
        };
        if !self.screen_rect(local_bounds).overlaps(self.clip) {
            return;
        }
        let offset = Vec2f::new(self.origin.x as f32, self.origin.y as f32);
        self.list
            .push_line(self.clip, from, to, width, color4b(color.r, color.g, color.b, color.a), offset);
    }

    /// Tessellates and records one simple local polygon without clipping its generated triangles.
    ///
    /// Convex polygons use a triangle fan; concave polygons use ear clipping. Degenerate,
    /// non-finite, and self-invalidating inputs safely emit no operation.
    pub fn fill_polygon(&mut self, points: &[Vec2f], color: Color) {
        if points.len() < 3 || color.a == 0 || !self.clip.has_positive_area() {
            return;
        }
        let Some(local_bounds) = Recti::from_points(points) else {
            return;
        };
        if !self.screen_rect(local_bounds).overlaps(self.clip) {
            return;
        }

        let offset = Vec2f::new(self.origin.x as f32, self.origin.y as f32);
        self.list.push_polygon(self.clip, points, color4b(color.r, color.g, color.b, color.a), offset);
    }

    /// Executes `paint` with a clip narrowed by a local rectangle.
    ///
    /// The parent Painter is unchanged after the closure returns. There is no mutable clip stack,
    /// public push/pop pair, or Drop-time restoration behavior.
    pub fn with_clip(&mut self, rect: Recti, paint: impl FnOnce(&mut Painter<'_>)) {
        let screen_clip = self.screen_rect(rect);
        let effective = self.clip.positive_intersection(screen_clip).unwrap_or_else(|| {
            // empty_origin = componentwise_max(parent_clip_origin, requested_clip_origin).
            Recti::new(self.clip.x.max(screen_clip.x), self.clip.y.max(screen_clip.y), 0, 0)
        });
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
        // screen_position = local_position + painter_origin.
        Vec2i::new(pos.x.saturating_add(self.origin.x), pos.y.saturating_add(self.origin.y))
    }

    /// Converts a local rectangle into screen space.
    fn screen_rect(&self, rect: Recti) -> Recti {
        rect.saturating_translated(self.origin)
    }

    /// Applies the common rectangle visibility policy and records one semantic rectangle operation.
    fn record_rect(&mut self, rect: Recti, color: Color, record: impl FnOnce(&mut DisplayList, Recti, Recti)) {
        if !rect.has_positive_area() || color.a == 0 {
            return;
        }
        let screen_rect = self.screen_rect(rect);
        if screen_rect.overlaps(self.clip) {
            record(self.list, self.clip, screen_rect);
        }
    }
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
            let mut painter = Painter::for_widget(&mut list, Recti::new(10, 20, 100, 80), Recti::new(12, 22, 50, 40));
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
            let mut painter = Painter::for_widget(&mut list, Recti::new(10, 20, 50, 50), Recti::new(10, 20, 50, 50));
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
            let mut painter = Painter::for_widget(&mut list, Recti::new(10, 20, 50, 50), Recti::new(10, 20, 50, 50));
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
            let mut painter = Painter::for_widget(&mut list, Recti::new(0, 0, 20, 20), Recti::new(0, 0, 5, 5));
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
            let mut painter = Painter::for_widget(&mut list, Recti::new(5, 7, 100, 100), Recti::new(0, 0, 200, 200));
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
            let mut painter = Painter::screen_space(&mut list, Recti::new(0, 0, 20, 20));
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
            let mut painter = Painter::for_widget(&mut list, Recti::new(10, 20, 100, 100), Recti::new(10, 20, 20, 20));
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
            let mut painter = Painter::screen_space(&mut list, Recti::new(0, 0, 20, 20));
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
