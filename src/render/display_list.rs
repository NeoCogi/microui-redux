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
//! Painter and Canvas adopt this internal operation surface in subsequent subsystem steps. Keep
//! the complete representation together now without producing transitional crate-wide warnings.
#![allow(dead_code)]

use super::backend::{CustomRenderArgs, CustomRenderCommand};
use crate::atlas::{FontId, IconId, SlotId};
use crate::style::{Color, Image};
use rs_math3d::{Color4b, Recti, Vec2f, Vec2i};
use std::{ops::Range, rc::Rc};

/// An owned sequence of rendering operations and their solid geometry.
///
/// Operations are intentionally opaque. Rendering primitives are appended through the rendering
/// subsystem, while callers own the list lifecycle and may clear and reuse its allocations.
#[derive(Default)]
pub struct DisplayList {
    /// Operations in final painter order.
    ops: Vec<DrawOp>,
    /// Screen-space solid triangles referenced by operation-owned ranges.
    solid_triangles: Vec<SolidTriangle>,
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
    /// Draws one atlas slot or renderer-owned texture.
    Image {
        /// Image identifier.
        image: Image,
        /// Destination rectangle in screen space.
        rect: Recti,
        /// Image tint.
        color: Color,
    },
    /// Draws one contiguous range of solid triangles.
    SolidTriangles {
        /// Validated range inside [`DisplayList::solid_triangles`].
        triangles: SolidTriangleRange,
    },
    /// Regenerates an atlas slot immediately before drawing it.
    RedrawSlot {
        /// Atlas slot identifier.
        id: SlotId,
        /// Destination rectangle in screen space.
        rect: Recti,
        /// Slot tint.
        color: Color,
        /// Pixel generator.
        payload: Rc<dyn Fn(usize, usize) -> Color4b>,
    },
    /// Invokes backend-specific drawing at this point in the operation stream.
    Custom {
        /// Content and clip geometry for the callback.
        args: CustomRenderArgs,
        /// Backend-specific callback.
        command: Box<dyn CustomRenderCommand>,
    },
}

/// Texture-independent vertex used by solid triangle operations.
#[derive(Clone, Copy)]
pub(super) struct SolidVertex {
    /// Screen-space position.
    pub(super) position: Vec2f,
    /// Interpolated vertex color.
    pub(super) color: Color4b,
}

impl SolidVertex {
    /// Creates one texture-independent solid vertex.
    pub(super) const fn new(position: Vec2f, color: Color4b) -> Self {
        Self { position, color }
    }
}

/// Strongly typed, texture-independent solid triangle.
#[derive(Clone, Copy)]
pub(super) struct SolidTriangle([SolidVertex; 3]);

impl SolidTriangle {
    /// Creates one complete solid triangle.
    pub(super) const fn new(v0: SolidVertex, v1: SolidVertex, v2: SolidVertex) -> Self {
        Self([v0, v1, v2])
    }

    /// Returns the triangle's three vertices in winding order.
    pub(super) const fn vertices(&self) -> &[SolidVertex; 3] {
        &self.0
    }
}

/// Validated solid-triangle range created and extended only by [`DisplayList`].
pub(super) struct SolidTriangleRange {
    /// Inclusive start index.
    start: usize,
    /// Exclusive end index.
    end: usize,
}

impl SolidTriangleRange {
    /// Creates a validated non-empty range of complete triangles.
    fn new(start: usize, end: usize) -> Self {
        assert!(start < end, "solid geometry range must not be empty");
        Self { start, end }
    }

    /// Returns a standard range for read-only execution.
    pub(super) fn as_range(&self) -> Range<usize> {
        self.start..self.end
    }

    /// Extends this range over newly appended contiguous triangles.
    fn extend_to(&mut self, start: usize, end: usize) -> bool {
        if self.end != start || start >= end {
            return false;
        }
        self.end = end;
        true
    }
}

/// Operations and triangles detached from a [`DisplayList`] for execution.
pub(super) struct RecordedFrame {
    /// Operations in painter order.
    pub(super) ops: Vec<DrawOp>,
    /// Solid geometry referenced by the operations.
    pub(super) solid_triangles: Vec<SolidTriangle>,
}

impl DisplayList {
    /// Creates an empty display list.
    pub const fn new() -> Self {
        Self {
            ops: Vec::new(),
            solid_triangles: Vec::new(),
        }
    }

    /// Removes every recorded operation and triangle while retaining allocated storage for reuse.
    pub fn clear(&mut self) {
        self.ops.clear();
        self.solid_triangles.clear();
    }

    /// Returns `true` when the list contains no operations or solid geometry.
    pub fn is_empty(&self) -> bool {
        self.ops.is_empty() && self.solid_triangles.is_empty()
    }

    /// Detaches the recorded frame and leaves this list empty and ready for new recording.
    pub(super) fn take(&mut self) -> RecordedFrame {
        RecordedFrame {
            ops: std::mem::take(&mut self.ops),
            solid_triangles: std::mem::take(&mut self.solid_triangles),
        }
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
    pub(super) fn push_image(&mut self, clip: Recti, image: Image, rect: Recti, color: Color) {
        self.push(clip, DrawKind::Image { image, rect, color });
    }

    /// Appends one dynamic atlas-slot redraw operation.
    pub(super) fn push_redraw_slot(&mut self, clip: Recti, id: SlotId, rect: Recti, color: Color, payload: Rc<dyn Fn(usize, usize) -> Color4b>) {
        self.push(clip, DrawKind::RedrawSlot { id, rect, color, payload });
    }

    /// Appends one backend-specific custom drawing barrier.
    pub(super) fn push_custom(&mut self, clip: Recti, args: CustomRenderArgs, command: Box<dyn CustomRenderCommand>) {
        self.push(clip, DrawKind::Custom { args, command });
    }

    /// Appends strongly typed solid triangles and records their valid contiguous range.
    pub(super) fn push_solid_triangles(&mut self, clip: Recti, triangles: &[SolidTriangle]) {
        if triangles.is_empty() {
            return;
        }

        let start = self.solid_triangles.len();
        self.solid_triangles.extend_from_slice(triangles);
        let end = self.solid_triangles.len();

        if let Some(DrawOp {
            clip: previous_clip,
            kind: DrawKind::SolidTriangles { triangles: previous_triangles },
        }) = self.ops.last_mut()
        {
            if same_rect(*previous_clip, clip) && previous_triangles.extend_to(start, end) {
                return;
            }
        }

        self.push(
            clip,
            DrawKind::SolidTriangles {
                triangles: SolidTriangleRange::new(start, end),
            },
        );
    }

    /// Appends an opaque payload with its effective clip.
    fn push(&mut self, clip: Recti, kind: DrawKind) {
        self.ops.push(DrawOp { clip, kind });
    }
}

/// Compares rectangle components without requiring an equality implementation from `rs-math3d`.
fn same_rect(left: Recti, right: Recti) -> bool {
    (left.x, left.y, left.width, left.height) == (right.x, right.y, right.width, right.height)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{color, color4b};

    fn solid_triangle(offset: f32) -> SolidTriangle {
        SolidTriangle::new(
            SolidVertex::new(Vec2f::new(offset, 0.0), color4b(255, 0, 0, 255)),
            SolidVertex::new(Vec2f::new(offset + 1.0, 0.0), color4b(0, 255, 0, 255)),
            SolidVertex::new(Vec2f::new(offset, 1.0), color4b(0, 0, 255, 255)),
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
            Recti::new(5, 6, 26, 36),
            Recti::new(6, 7, 25, 35),
        ];

        list.push_fill_rect(clips[0], Recti::new(0, 0, 2, 2), color(1, 2, 3, 4));
        list.push_text(clips[1], FontId::default(), Vec2i::new(4, 5), color(5, 6, 7, 8), "text");
        list.push_icon(clips[2], IconId::default(), Recti::new(6, 7, 8, 9), color(9, 10, 11, 12));
        list.push_image(clips[3], Image::Slot(SlotId::default()), Recti::new(10, 11, 12, 13), color(13, 14, 15, 16));
        list.push_redraw_slot(
            clips[4],
            SlotId::default(),
            Recti::new(14, 15, 16, 17),
            color(17, 18, 19, 20),
            Rc::new(|_, _| color4b(255, 255, 255, 255)),
        );
        list.push_custom(
            clips[5],
            CustomRenderArgs {
                content_area: Recti::new(18, 19, 20, 21),
                view: clips[5],
            },
            Box::new(|_, _args: &CustomRenderArgs| {}),
        );

        assert_eq!(list.ops.len(), clips.len());
        for (operation, expected) in list.ops.iter().zip(clips) {
            assert_eq!(rect_tuple(operation.clip), rect_tuple(expected));
        }
        assert!(matches!(list.ops[0].kind, DrawKind::FillRect { .. }));
        assert!(matches!(list.ops[1].kind, DrawKind::Text { .. }));
        assert!(matches!(list.ops[2].kind, DrawKind::Icon { .. }));
        assert!(matches!(list.ops[3].kind, DrawKind::Image { .. }));
        assert!(matches!(list.ops[4].kind, DrawKind::RedrawSlot { .. }));
        assert!(matches!(list.ops[5].kind, DrawKind::Custom { .. }));
    }

    #[test]
    fn clear_retains_operation_and_triangle_allocations() {
        let mut list = DisplayList::new();
        let clip = Recti::new(0, 0, 100, 100);
        for offset in 0..32 {
            list.push_fill_rect(clip, Recti::new(offset, offset, 1, 1), color(255, 255, 255, 255));
        }
        list.push_solid_triangles(clip, &[solid_triangle(0.0)]);

        let operation_capacity = list.ops.capacity();
        let triangle_capacity = list.solid_triangles.capacity();
        assert!(operation_capacity > 0);
        assert!(triangle_capacity > 0);

        list.clear();

        assert!(list.is_empty());
        assert_eq!(list.ops.capacity(), operation_capacity);
        assert_eq!(list.solid_triangles.capacity(), triangle_capacity);
    }

    #[test]
    fn take_detaches_owned_storage_and_leaves_a_reusable_list() {
        let mut list = DisplayList::new();
        let clip = Recti::new(0, 0, 50, 50);
        list.push_fill_rect(clip, Recti::new(1, 2, 3, 4), color(10, 20, 30, 40));
        list.push_solid_triangles(clip, &[solid_triangle(0.0)]);

        let frame = list.take();

        assert!(list.is_empty());
        assert_eq!(frame.ops.len(), 2);
        assert_eq!(frame.solid_triangles.len(), 1);

        list.push_fill_rect(clip, Recti::new(5, 6, 7, 8), color(50, 60, 70, 80));
        assert!(!list.is_empty());
        assert_eq!(list.ops.len(), 1);
    }

    #[test]
    fn adjacent_solid_triangle_ranges_merge_when_clips_match() {
        let mut list = DisplayList::new();
        let clip = Recti::new(0, 0, 100, 100);

        list.push_solid_triangles(clip, &[solid_triangle(0.0)]);
        list.push_solid_triangles(clip, &[solid_triangle(10.0)]);

        assert_eq!(list.ops.len(), 1);
        assert_eq!(list.solid_triangles.len(), 2);
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

        list.push_solid_triangles(first_clip, &[solid_triangle(0.0)]);
        list.push_solid_triangles(second_clip, &[solid_triangle(10.0)]);
        list.push_fill_rect(first_clip, Recti::new(0, 0, 1, 1), color(255, 255, 255, 255));
        list.push_solid_triangles(second_clip, &[solid_triangle(20.0)]);

        assert_eq!(list.ops.len(), 4);
        assert_eq!(list.solid_triangles.len(), 3);

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
        let triangle = solid_triangle(0.0);

        list.push_solid_triangles(clip, &[triangle]);

        assert_eq!(list.solid_triangles.len(), 1);
        assert_eq!(list.solid_triangles[0].vertices().len(), 3);
    }

    #[test]
    fn empty_solid_geometry_does_not_create_an_operation() {
        let mut list = DisplayList::new();
        list.push_solid_triangles(Recti::new(0, 0, 100, 100), &[]);
        assert!(list.is_empty());
    }
}
