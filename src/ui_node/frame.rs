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
use crate::{AppearanceRole, AvailableSpace, Constraints, Dimensioni, NinePatch, Recti, SliceInsets, Skin, VisualState};

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
pub(crate) fn frame_geometry(outer: Recti, role: Option<AppearanceRole>, style: &Skin) -> FrameGeometry {
    // A non-framed node uses zero insets. A framed node uses its semantic normal-state geometry;
    // every interaction state for one role is required to preserve those destination insets.
    let insets = role
        .map(|role| style.visual(role, VisualState::Normal).patch.insets.normalized())
        .unwrap_or(SliceInsets::ZERO);
    frame_geometry_with_insets(outer, insets)
}

/// Resolves a border box using structural insets supplied independently from appearance artwork.
pub(crate) fn frame_geometry_with_insets(outer: Recti, insets: SliceInsets) -> FrameGeometry {
    // Window chrome uses this path because its fixed corner artwork may extend much farther along
    // an edge than the narrow client inset represented by that edge's real border thickness.
    let insets = insets.normalized();
    let content = if outer.width <= 0 || outer.height <= 0 {
        None
    } else if insets.horizontal_extent() == 0 && insets.vertical_extent() == 0 {
        Some(outer)
    } else {
        checked_inset(outer, insets)
    };
    FrameGeometry { outer, content }
}

/// Removes a frame from each finite measurement bound while preserving unbounded axes.
pub(crate) fn content_constraints(constraints: Constraints, insets: SliceInsets) -> Constraints {
    // Each axis removes its independently configured pair of patch cells. Unbounded space stays
    // unbounded because a finite frame cannot constrain an intrinsically open measurement axis.
    Constraints::new(
        inset_available(constraints.width, insets.horizontal_extent()),
        inset_available(constraints.height, insets.vertical_extent()),
    )
}

/// Adds a resolved frame to positive preferred content dimensions.
pub(crate) fn outer_preferred(preferred: Dimensioni, insets: SliceInsets) -> Dimensioni {
    // Positive content receives both fixed patch sides on each axis. Non-positive sentinel
    // dimensions retain their established meaning instead of becoming visible solely due to chrome.
    Dimensioni::new(
        expand_positive_axis(preferred.width, insets.horizontal_extent()),
        expand_positive_axis(preferred.height, insets.vertical_extent()),
    )
}

/// Paints a checked inside-aligned internal frame and returns its usable interior.
pub(crate) fn paint_internal_frame(painter: &mut Painter<'_>, outer: Recti, patch: NinePatch) -> Option<Recti> {
    // Invalid outer geometry produces neither layout content nor paint. Positive rectangles share
    // the same normalized inset calculation used by node layout before one compact patch record.
    if outer.width <= 0 || outer.height <= 0 {
        return None;
    }
    let insets = patch.insets.normalized();
    let content = if insets.horizontal_extent() == 0 && insets.vertical_extent() == 0 {
        Some(outer)
    } else {
        checked_inset(outer, insets)
    };
    painter.with_clip(outer, |painter| {
        // NinePatch itself decides which of its nine cells are visible. Structural insets therefore
        // remain effective even when border artwork or colors are transparent.
        painter.nine_patch(outer, patch);
    });
    content
}

/// Removes four normalized patch insets from one positive outer rectangle.
fn checked_inset(outer: Recti, insets: SliceInsets) -> Option<Recti> {
    // Reject a fully consumed axis so callers represent the lack of usable content explicitly.
    let insets = insets.normalized();
    let horizontal = insets.left.checked_add(insets.right)?;
    let vertical = insets.top.checked_add(insets.bottom)?;
    if horizontal >= outer.width || vertical >= outer.height {
        return None;
    }
    // Leading insets move the content origin while both opposing sides reduce its dimensions.
    let x = checked_add(outer.x, insets.left)?;
    let y = checked_add(outer.y, insets.top)?;
    let content_width = outer.width.checked_sub(horizontal)?;
    let content_height = outer.height.checked_sub(vertical)?;
    Some(Recti::new(x, y, content_width, content_height))
}

/// Removes a non-negative inset without conflating a bounded zero with unbounded space.
fn inset_available(space: AvailableSpace, inset: i32) -> AvailableSpace {
    match space {
        AvailableSpace::Unbounded => AvailableSpace::Unbounded,
        AvailableSpace::Bounded(value) => AvailableSpace::bounded(value.saturating_sub(inset.max(0))),
    }
}

fn expand_positive_axis(value: i32, inset_extent: i32) -> i32 {
    if value <= 0 {
        value
    } else {
        // Preferred geometry is allowed to saturate; exact placement will still be bounded by its
        // parent constraint, and an extreme font or style must not turn measurement into a panic.
        value.saturating_add(inset_extent.max(0))
    }
}

fn checked_add(left: i32, right: i32) -> Option<i32> {
    // sum = left + right, rejected when it is outside the i32 range.
    i32::try_from(i64::from(left) + i64::from(right)).ok()
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::SurfaceRole;
    use crate::{Color, color};
    use crate::render::DisplayList;
    use crate::test_support::{replace_skin_patches, test_atlas, test_skin};

    /// Replaces the generic frame role used by the frame geometry under test.
    fn with_frame(mut style: Skin, patch: NinePatch) -> Skin {
        // Tests mutate the same concrete catalog entry that production generic framing resolves.
        replace_skin_patches(
            &mut style,
            crate::AppearanceRole::Surface(SurfaceRole::GenericFrame),
            crate::StateTable::filled(patch),
        );
        style
    }

    #[test]
    fn frame_geometry_derives_inside_content() {
        let atlas = test_atlas();
        let style = with_frame(test_skin(&atlas), NinePatch::framed(SliceInsets::uniform(2), color(1, 2, 3, 255), None));
        let geometry = frame_geometry(
            Recti::new(10, 20, 30, 40),
            Some(crate::AppearanceRole::Surface(SurfaceRole::GenericFrame)),
            &style,
        );
        assert_eq!(rect_tuple(geometry.outer), (10, 20, 30, 40));
        assert_eq!(geometry.content.map(rect_tuple), Some((12, 22, 26, 36)));
    }

    #[test]
    fn zero_width_frame_keeps_the_complete_content_rect() {
        let atlas = test_atlas();
        let style = with_frame(test_skin(&atlas), NinePatch::framed(SliceInsets::ZERO, color(1, 2, 3, 255), None));
        let outer = Recti::new(10, 20, 30, 40);
        let geometry = frame_geometry(outer, Some(crate::AppearanceRole::Surface(SurfaceRole::GenericFrame)), &style);
        assert_eq!(geometry.content.map(rect_tuple), Some(rect_tuple(outer)));
    }

    #[test]
    fn transparent_border_keeps_structural_inset() {
        let atlas = test_atlas();
        let style = with_frame(test_skin(&atlas), NinePatch::framed(SliceInsets::uniform(1), color(0, 0, 0, 0), None));
        assert_eq!(
            frame_geometry(Recti::new(4, 5, 8, 7), Some(crate::AppearanceRole::Surface(SurfaceRole::GenericFrame)), &style)
                .content
                .map(rect_tuple),
            Some((5, 6, 6, 5))
        );
    }

    #[test]
    fn tiny_frame_has_no_content() {
        let atlas = test_atlas();
        let style = test_skin(&atlas);
        assert!(
            frame_geometry(Recti::new(7, 8, 2, 10), Some(crate::AppearanceRole::Surface(SurfaceRole::GenericFrame)), &style)
                .content
                .is_none()
        );
    }

    #[test]
    fn preferred_measurement_preserves_non_positive_results() {
        let preferred = outer_preferred(Dimensioni::new(10, -1), SliceInsets::uniform(2));
        assert_eq!((preferred.width, preferred.height), (14, -1));
    }

    #[test]
    fn frame_inset_preserves_zero_and_unbounded_as_distinct_constraints() {
        let constraints = Constraints::new(AvailableSpace::Bounded(1), AvailableSpace::Unbounded);
        assert_eq!(
            content_constraints(constraints, SliceInsets::uniform(2)),
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

        let content = paint_internal_frame(&mut painter, outer, NinePatch::framed(SliceInsets::uniform(1), border, Some(fill)));
        assert_eq!(content.map(rect_tuple), Some((11, 21, 6, 5)));

        let recorded = list.debug_fill_rects();
        assert_eq!(recorded.len(), 9);
        assert_eq!(
            recorded.iter().map(|(rect, _, _)| rect_tuple(*rect)).collect::<Vec<_>>(),
            vec![
                (10, 20, 1, 1),
                (11, 20, 6, 1),
                (17, 20, 1, 1),
                (10, 21, 1, 5),
                (11, 21, 6, 5),
                (17, 21, 1, 5),
                (10, 26, 1, 1),
                (11, 26, 6, 1),
                (17, 26, 1, 1),
            ]
        );
        assert!(
            recorded
                .iter()
                .enumerate()
                .filter(|(index, _)| *index != 4)
                .all(|(_, (_, _, color))| color_tuple(*color) == color_tuple(border))
        );
        assert_eq!(color_tuple(recorded[4].2), color_tuple(fill));
    }

    #[test]
    fn tiny_internal_frame_records_border_once_without_fill() {
        let outer = Recti::new(3, 4, 2, 9);
        let viewport = Recti::new(0, 0, 100, 100);
        let border = color(7, 8, 9, 255);
        let mut list = DisplayList::new();
        let mut painter = Painter::screen_space(&mut list, viewport);

        assert!(
            paint_internal_frame(
                &mut painter,
                outer,
                NinePatch::framed(SliceInsets::uniform(1), border, Some(color(10, 11, 12, 255))),
            )
            .is_none()
        );
        let recorded = list.debug_fill_rects();
        assert_eq!(recorded.len(), 6);
        assert!(recorded.iter().all(|(_, _, color)| color_tuple(*color) == color_tuple(border)));
    }

    #[test]
    fn transparent_internal_border_insets_without_recording_border_pixels() {
        let outer = Recti::new(5, 6, 9, 8);
        let viewport = Recti::new(0, 0, 100, 100);
        let fill = color(20, 30, 40, 255);
        let mut list = DisplayList::new();
        let mut painter = Painter::screen_space(&mut list, viewport);

        let content = paint_internal_frame(&mut painter, outer, NinePatch::framed(SliceInsets::uniform(2), color(0, 0, 0, 0), Some(fill)));
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
