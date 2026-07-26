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
//! Owned render-operation and solid-geometry storage.
//!
//! Painter records through this internal operation surface and Renderer consumes it exactly once.

use super::{
    backend::CustomRenderKey,
    geometry::{SolidGeometry, SolidTriangle, SolidTriangleRange},
};
use crate::atlas::{FontId, IconId};
use crate::style::{Color, TextureId};
use rs_math3d::{Color4b, Recti, Vec2f, Vec2i};

/// An owned sequence of rendering operations and their solid geometry.
///
/// Operations are intentionally opaque. Rendering primitives are appended through the rendering
/// subsystem, while callers own the list lifecycle and may clear and reuse its allocations.
#[derive(Default)]
pub struct DisplayList {
    /// Operations in final painter order.
    pub(super) ops: Vec<DrawOp>,
    /// Retained solid geometry and its reusable tessellation workspace.
    pub(super) solid_geometry: SolidGeometry,
}

/// One recorded operation and its final screen-space clip.
pub(super) struct DrawOp {
    /// Effective clip resolved when the operation was recorded.
    pub(super) clip: Recti,
    /// Operation payload.
    pub(super) kind: DrawKind,
}

/// Private rendering operation payload.
pub(super) enum DrawKind {
    /// Draws a semantic solid rectangle.
    FillRect {
        /// Rectangle in screen space.
        rect: Recti,
        /// Fill color.
        color: Color,
    },
    /// Draws one UTF-8 text run.
    Text {
        /// Font used for the text run.
        font: FontId,
        /// Text origin in screen space.
        pos: Vec2i,
        /// Text color.
        color: Color,
        /// Owned UTF-8 text.
        text: String,
    },
    /// Draws one atlas icon.
    Icon {
        /// Atlas icon identifier.
        id: IconId,
        /// Destination rectangle in screen space.
        rect: Recti,
        /// Icon tint.
        color: Color,
    },
    /// Draws one backend-owned external texture.
    Image {
        /// External texture identifier.
        id: TextureId,
        /// Destination rectangle in screen space.
        rect: Recti,
        /// Image tint.
        color: Color,
    },
    /// Draws one contiguous range of solid triangles.
    SolidTriangles {
        /// Validated range inside [`DisplayList::solid_geometry`].
        triangles: SolidTriangleRange,
    },
    /// Invokes backend-specific drawing at this point in the operation stream.
    Custom {
        /// Renderer-owned callback key.
        renderer: CustomRenderKey,
        /// Unclipped custom-render content rectangle.
        content_area: Recti,
    },
}

impl DisplayList {
    /// Creates an empty display list.
    pub const fn new() -> Self {
        Self {
            ops: Vec::new(),
            solid_geometry: SolidGeometry::new(),
        }
    }

    /// Removes every recorded operation and triangle while retaining allocated storage for reuse.
    pub fn clear(&mut self) {
        self.ops.clear();
        self.solid_geometry.clear();
    }

    /// Returns `true` when the list contains no operations or solid geometry.
    pub fn is_empty(&self) -> bool {
        self.ops.is_empty() && self.solid_geometry.is_empty()
    }

    /// Appends one semantic rectangle operation.
    pub(super) fn push_fill_rect(&mut self, clip: Recti, rect: Recti, color: Color) {
        self.push(clip, DrawKind::FillRect { rect, color });
    }

    /// Appends one owned text operation.
    pub(super) fn push_text(&mut self, clip: Recti, font: FontId, pos: Vec2i, color: Color, text: impl Into<String>) {
        self.push(clip, DrawKind::Text { font, pos, color, text: text.into() });
    }

    /// Appends one atlas icon operation.
    pub(super) fn push_icon(&mut self, clip: Recti, id: IconId, rect: Recti, color: Color) {
        self.push(clip, DrawKind::Icon { id, rect, color });
    }

    /// Appends one image operation.
    pub(super) fn push_image(&mut self, clip: Recti, id: TextureId, rect: Recti, color: Color) {
        self.push(clip, DrawKind::Image { id, rect, color });
    }

    /// Appends one backend-specific custom drawing barrier.
    pub(crate) fn push_custom(&mut self, clip: Recti, renderer: CustomRenderKey, content_area: Recti) {
        self.push(clip, DrawKind::Custom { renderer, content_area });
    }

    /// Appends strongly typed solid triangles and records their valid contiguous range.
    #[allow(dead_code)]
    pub(super) fn push_solid_triangles(&mut self, clip: Recti, triangles: &[SolidTriangle]) {
        if let Some(range) = self.solid_geometry.append_triangles(triangles, Vec2f::new(0.0, 0.0)) {
            self.push_solid_range(clip, range);
        }
    }

    /// Tessellates and records one translated thick line.
    pub(super) fn push_line(&mut self, clip: Recti, from: Vec2f, to: Vec2f, width: f32, color: Color4b, offset: Vec2f) {
        if let Some(range) = self.solid_geometry.append_line(from, to, width, color, offset) {
            self.push_solid_range(clip, range);
        }
    }

    /// Tessellates and records one translated simple polygon.
    pub(super) fn push_polygon(&mut self, clip: Recti, points: &[Vec2f], color: Color4b, offset: Vec2f) {
        if let Some(range) = self.solid_geometry.append_polygon(points, color, offset) {
            self.push_solid_range(clip, range);
        }
    }

    /// Associates one newly appended geometry range with operation ordering and clipping.
    fn push_solid_range(&mut self, clip: Recti, range: SolidTriangleRange) {
        if let Some(DrawOp {
            clip: previous_clip,
            kind: DrawKind::SolidTriangles { triangles: previous_triangles },
        }) = self.ops.last_mut()
        {
            if same_rect(*previous_clip, clip) && previous_triangles.extend(&range) {
                return;
            }
        }

        self.push(clip, DrawKind::SolidTriangles { triangles: range });
    }

    /// Appends an opaque payload with its effective clip.
    fn push(&mut self, clip: Recti, kind: DrawKind) {
        self.ops.push(DrawOp { clip, kind });
    }

    /// Returns owned text snapshots for runtime paint assertions.
    #[cfg(test)]
    pub(crate) fn debug_texts(&self) -> Vec<String> {
        self.ops
            .iter()
            .filter_map(|operation| match &operation.kind {
                DrawKind::Text { text, .. } => Some(text.clone()),
                _ => None,
            })
            .collect()
    }

    /// Returns rectangle snapshots for runtime paint assertions.
    #[cfg(test)]
    pub(crate) fn debug_rects(&self) -> Vec<Recti> {
        self.ops
            .iter()
            .filter_map(|operation| match operation.kind {
                DrawKind::FillRect { rect, .. } => Some(rect),
                _ => None,
            })
            .collect()
    }

    /// Returns the number of opaque operations for performance assertions.
    #[cfg(test)]
    pub(crate) fn debug_operation_count(&self) -> usize {
        self.ops.len()
    }

    /// Returns the operation allocation capacity retained by this list.
    #[cfg(test)]
    pub(crate) fn debug_operation_capacity(&self) -> usize {
        self.ops.capacity()
    }

    /// Returns the number of strongly typed solid triangles retained by this list.
    #[cfg(test)]
    pub(crate) fn debug_triangle_count(&self) -> usize {
        self.solid_geometry.triangles().len()
    }

    /// Returns the solid-triangle allocation capacity retained by this list.
    #[cfg(test)]
    pub(crate) fn debug_triangle_capacity(&self) -> usize {
        self.solid_geometry.triangle_capacity()
    }

    /// Returns the polygon workspace allocation capacity retained by this list.
    #[cfg(test)]
    pub(crate) fn debug_polygon_capacity(&self) -> usize {
        self.solid_geometry.polygon_capacity()
    }
}

/// Compares rectangle components without requiring an equality implementation from `rs-math3d`.
fn same_rect(left: Recti, right: Recti) -> bool {
    (left.x, left.y, left.width, left.height) == (right.x, right.y, right.width, right.height)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{color, color4b, TextureId};

    fn triangle_at(offset: f32) -> SolidTriangle {
        SolidTriangle::new(
            [Vec2f::new(offset, 0.0), Vec2f::new(offset + 1.0, 0.0), Vec2f::new(offset, 1.0)],
            color4b(255, 0, 0, 255),
        )
    }

    fn rect_tuple(rect: Recti) -> (i32, i32, i32, i32) {
        (rect.x, rect.y, rect.width, rect.height)
    }

    #[test]
    fn every_semantic_operation_owns_its_effective_clip() {
        let mut list = DisplayList::new();
        let clips = [
            Recti::new(1, 2, 30, 40),
            Recti::new(2, 3, 29, 39),
            Recti::new(3, 4, 28, 38),
            Recti::new(4, 5, 27, 37),
        ];

        list.push_fill_rect(clips[0], Recti::new(0, 0, 2, 2), color(1, 2, 3, 4));
        list.push_text(clips[1], FontId::default(), Vec2i::new(4, 5), color(5, 6, 7, 8), "text");
        list.push_icon(clips[2], IconId::default(), Recti::new(6, 7, 8, 9), color(9, 10, 11, 12));
        list.push_image(clips[3], TextureId::new(1, 12, 13), Recti::new(10, 11, 12, 13), color(13, 14, 15, 16));

        assert_eq!(list.ops.len(), clips.len());
        for (operation, expected) in list.ops.iter().zip(clips) {
            assert_eq!(rect_tuple(operation.clip), rect_tuple(expected));
        }
        assert!(matches!(list.ops[0].kind, DrawKind::FillRect { .. }));
        assert!(matches!(list.ops[1].kind, DrawKind::Text { .. }));
        assert!(matches!(list.ops[2].kind, DrawKind::Icon { .. }));
        assert!(matches!(list.ops[3].kind, DrawKind::Image { .. }));
    }

    #[test]
    fn clear_retains_operation_and_triangle_allocations() {
        let mut list = DisplayList::new();
        let clip = Recti::new(0, 0, 100, 100);
        for offset in 0..32 {
            list.push_fill_rect(clip, Recti::new(offset, offset, 1, 1), color(255, 255, 255, 255));
        }
        list.push_solid_triangles(clip, &[triangle_at(0.0)]);
        list.solid_geometry.reserve_polygon_capacity(32);

        let operation_capacity = list.ops.capacity();
        let triangle_capacity = list.solid_geometry.triangle_capacity();
        let polygon_capacity = list.solid_geometry.polygon_capacity();
        assert!(operation_capacity > 0);
        assert!(triangle_capacity > 0);
        assert!(polygon_capacity > 0);

        list.clear();

        assert!(list.is_empty());
        assert_eq!(list.ops.capacity(), operation_capacity);
        assert_eq!(list.solid_geometry.triangle_capacity(), triangle_capacity);
        assert_eq!(list.solid_geometry.polygon_capacity(), polygon_capacity);
    }

    #[test]
    fn adjacent_solid_triangle_ranges_merge_when_clips_match() {
        let mut list = DisplayList::new();
        let clip = Recti::new(0, 0, 100, 100);

        list.push_solid_triangles(clip, &[triangle_at(0.0)]);
        list.push_solid_triangles(clip, &[triangle_at(10.0)]);

        assert_eq!(list.ops.len(), 1);
        assert_eq!(list.solid_geometry.triangles().len(), 2);
        let DrawKind::SolidTriangles { triangles } = &list.ops[0].kind else {
            panic!("expected a solid-triangle operation");
        };
        assert_eq!(triangles.as_range(), 0..2);
    }

    #[test]
    fn solid_triangle_ranges_do_not_merge_across_clip_or_operation_barriers() {
        let mut list = DisplayList::new();
        let first_clip = Recti::new(0, 0, 100, 100);
        let second_clip = Recti::new(1, 1, 99, 99);

        list.push_solid_triangles(first_clip, &[triangle_at(0.0)]);
        list.push_solid_triangles(second_clip, &[triangle_at(10.0)]);
        list.push_fill_rect(first_clip, Recti::new(0, 0, 1, 1), color(255, 255, 255, 255));
        list.push_solid_triangles(second_clip, &[triangle_at(20.0)]);

        assert_eq!(list.ops.len(), 4);
        assert_eq!(list.solid_geometry.triangles().len(), 3);

        let ranges = list
            .ops
            .iter()
            .filter_map(|operation| match &operation.kind {
                DrawKind::SolidTriangles { triangles } => Some(triangles.as_range()),
                _ => None,
            })
            .collect::<Vec<_>>();
        assert_eq!(ranges, vec![0..1, 1..2, 2..3]);
    }

    #[test]
    fn solid_triangle_storage_preserves_complete_triplets() {
        let mut list = DisplayList::new();
        let clip = Recti::new(0, 0, 100, 100);
        let triangle = triangle_at(0.0);

        list.push_solid_triangles(clip, &[triangle]);

        assert_eq!(list.solid_geometry.triangles().len(), 1);
        assert_eq!(list.solid_geometry.triangles()[0].vertices().len(), 3);
    }

    #[test]
    fn empty_solid_geometry_does_not_create_an_operation() {
        let mut list = DisplayList::new();
        list.push_solid_triangles(Recti::new(0, 0, 100, 100), &[]);
        assert!(list.is_empty());
    }
}
