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
// -----------------------------------------------------------------------------
// Ported to rust from https://github.com/rxi/microui/ and the original license
//
// Copyright (c) 2020 rxi
//
// Permission is hereby granted, free of charge, to any person obtaining a copy
// of this software and associated documentation files (the "Software"), to
// deal in the Software without restriction, including without limitation the
// rights to use, copy, modify, merge, publish, distribute, sublicense, and/or
// sell copies of the Software, and to permit persons to whom the Software is
// furnished to do so, subject to the following conditions:
//
// The above copyright notice and this permission notice shall be included in
// all copies or substantial portions of the Software.
//
// THE SOFTWARE IS PROVIDED "AS IS", WITHOUT WARRANTY OF ANY KIND, EXPRESS OR
// IMPLIED, INCLUDING BUT NOT LIMITED TO THE WARRANTIES OF MERCHANTABILITY,
// FITNESS FOR A PARTICULAR PURPOSE AND NONINFRINGEMENT. IN NO EVENT SHALL THE
// AUTHORS OR COPYRIGHT HOLDERS BE LIABLE FOR ANY CLAIM, DAMAGES OR OTHER
// LIABILITY, WHETHER IN AN ACTION OF CONTRACT, TORT OR OTHERWISE, ARISING
// FROM, OUT OF OR IN CONNECTION WITH THE SOFTWARE OR THE USE OR OTHER DEALINGS
// IN THE SOFTWARE.
//
//! Pure one-axis scrollbar geometry.
//!
//! Widgets and containers still own their layout, state, and input policy. This module only keeps
//! the track/thumb mapping in one place so paint, dragging, and track clicks cannot disagree.
use crate::{
    ControlColor, Dimensioni, FocusPolicy, MouseButton, Node, Recti, TypedWidgetHandle, UiInputEvent, Vec2i, Widget, WidgetOption, WidgetPaintCtx,
    WidgetUpdateCtx,
};

#[derive(Copy, Clone, Debug, Eq, PartialEq)]
/// Axis selector used by shared scrollbar geometry helpers.
pub(crate) enum ScrollAxis {
    /// Vertical scrollbar operating on y offsets.
    Vertical,
    /// Horizontal scrollbar operating on x offsets.
    Horizontal,
}

impl ScrollAxis {
    fn rect_len(self, rect: Recti) -> i32 {
        match self {
            Self::Vertical => rect.height,
            Self::Horizontal => rect.width,
        }
    }

    fn point(self, point: Vec2i) -> i32 {
        match self {
            Self::Vertical => point.y,
            Self::Horizontal => point.x,
        }
    }

    fn origin(self, rect: Recti) -> i32 {
        match self {
            Self::Vertical => rect.y,
            Self::Horizontal => rect.x,
        }
    }

    fn with_len(self, mut rect: Recti, len: i32) -> Recti {
        match self {
            Self::Vertical => rect.height = len,
            Self::Horizontal => rect.width = len,
        }
        rect
    }

    fn translate(self, mut rect: Recti, amount: i32) -> Recti {
        // translated_axis_origin = rectangle_axis_origin + amount.
        match self {
            Self::Vertical => rect.y = rect.y.saturating_add(amount),
            Self::Horizontal => rect.x = rect.x.saturating_add(amount),
        }
        rect
    }
}

/// Complete mapping between one scrollbar track and one content axis.
///
/// `thumb`, [`Self::drag_delta`], and [`Self::centered_offset`] all use the same scroll range and
/// thumb travel. Keeping those values together is important when the minimum thumb length is
/// larger than the proportional thumb.
#[derive(Copy, Clone, Debug)]
pub(crate) struct ScrollbarGeometry {
    axis: ScrollAxis,
    track: Recti,
    thumb: Recti,
    max_offset: i32,
    thumb_travel: i32,
}

impl ScrollbarGeometry {
    /// Resolves one track and thumb for the supplied visible/content lengths and offset.
    pub(crate) fn new(axis: ScrollAxis, track: Recti, view_len: i32, content_len: i32, offset: i32, min_thumb_len: i32) -> Self {
        // Normalize every external length once so later mapping functions operate on valid ranges.
        let track_len = axis.rect_len(track).max(0);
        let view_len = view_len.max(0);
        let content_len = content_len.max(0);
        let max_offset = scrollbar_max_scroll(content_len, view_len);

        // Proportional length represents the visible fraction; the style minimum preserves usability.
        let proportional = if content_len > 0 {
            // proportional_thumb_length = track_length * visible_length / content_length.
            track_len.saturating_mul(view_len) / content_len
        } else {
            track_len
        };
        let thumb_len = proportional.max(min_thumb_len.max(0)).min(track_len);
        // thumb_travel = track_length - thumb_length.
        let thumb_travel = track_len.saturating_sub(thumb_len).max(0);
        // Map clamped content offset into thumb travel using the same integer ratio inverted by drag.
        let thumb_offset = if max_offset > 0 && thumb_travel > 0 {
            // thumb_offset = clamped_content_offset * thumb_travel / maximum_content_offset.
            offset.clamp(0, max_offset).saturating_mul(thumb_travel) / max_offset
        } else {
            0
        };
        let thumb = axis.translate(axis.with_len(track, thumb_len), thumb_offset);

        Self {
            axis,
            track,
            thumb,
            max_offset,
            thumb_travel,
        }
    }

    /// Returns the full scrollbar track.
    pub(crate) fn track(self) -> Recti {
        self.track
    }

    /// Returns the painted and hit-tested thumb rectangle.
    pub(crate) fn thumb(self) -> Recti {
        self.thumb
    }

    /// Converts pointer movement into the exactly inverse content-offset movement.
    pub(crate) fn drag_delta(self, delta: Vec2i) -> i32 {
        // A non-scrollable range or immobile thumb cannot produce a meaningful content delta.
        if self.thumb_travel <= 0 || self.max_offset <= 0 {
            return 0;
        }
        // content_delta = pointer_delta * maximum_content_offset / thumb_travel.
        self.axis.point(delta).saturating_mul(self.max_offset) / self.thumb_travel
    }

    /// Returns the offset that centers the thumb on `pointer`, clamped to the track.
    pub(crate) fn centered_offset(self, pointer: Vec2i) -> i32 {
        // Track clicks use the same travel/range mapping as dragging, centered on the thumb.
        if self.thumb_travel <= 0 || self.max_offset <= 0 {
            return 0;
        }
        let thumb_len = self.axis.rect_len(self.thumb);
        // track_pointer = pointer_axis_position - track_axis_origin.
        let track_pointer = self.axis.point(pointer).saturating_sub(self.axis.origin(self.track));
        // centered_thumb = clamp(track_pointer - thumb_length / 2, 0, thumb_travel).
        let centered = track_pointer.saturating_sub(thumb_len / 2).clamp(0, self.thumb_travel);
        // content_offset = centered_thumb * maximum_content_offset / thumb_travel.
        centered.saturating_mul(self.max_offset) / self.thumb_travel
    }
}

/// Returns the scrollbar track rectangle just outside the container body on the selected axis.
pub(crate) fn scrollbar_base(axis: ScrollAxis, body: Recti, scrollbar_size: i32) -> Recti {
    // Start with the body so the cross-axis origin and extent remain identical.
    let mut base = body;
    match axis {
        ScrollAxis::Vertical => {
            // vertical_track_x = body_x + body_width.
            base.x = body.x.saturating_add(body.width);
            base.width = scrollbar_size;
        }
        ScrollAxis::Horizontal => {
            // horizontal_track_y = body_y + body_height.
            base.y = body.y.saturating_add(body.height);
            base.height = scrollbar_size;
        }
    }
    base
}

/// Returns the largest scroll offset needed to reveal all content.
pub(crate) fn scrollbar_max_scroll(content_len: i32, view_len: i32) -> i32 {
    // maximum_scroll = max(content_length - visible_length, 0).
    content_len.saturating_sub(view_len).max(0)
}

/// Layout-supplied one-axis scrollbar configuration.
#[derive(Copy, Clone)]
struct ScrollbarConfiguration {
    track: Recti,
    view_len: i32,
    content_len: i32,
    min_thumb_len: i32,
}

/// Ordinary widget used as one retained child of a composite scroll area.
pub(crate) struct RetainedScrollbar {
    axis: ScrollAxis,
    configuration: Option<ScrollbarConfiguration>,
    offset: i32,
    opt: WidgetOption,
}

impl RetainedScrollbar {
    /// Installs geometry from the parent layout and clamps any previous offset.
    pub(crate) fn configure(&mut self, track: Recti, view_len: i32, content_len: i32, min_thumb_len: i32) {
        // Replace the complete layout-authored configuration atomically before clamping offset.
        self.configuration = Some(ScrollbarConfiguration {
            track,
            view_len,
            content_len,
            min_thumb_len,
        });
        // offset = clamp(requested_offset, 0, maximum_offset).
        self.offset = self.offset.clamp(0, self.max_offset());
    }

    /// Removes the widget from interaction and resets the now-invalid content offset.
    pub(crate) fn deactivate(&mut self) {
        // Geometry and offset are the only retained scrollbar state. Drag activity is derived from
        // UiRuntime capture through WidgetUpdateCtx::active and therefore needs no local reset.
        self.configuration = None;
        self.offset = 0;
    }

    /// Returns the current clamped content offset on this axis.
    pub(crate) const fn offset(&self) -> i32 {
        self.offset
    }

    /// Requests a non-negative offset that the next layout configuration will clamp.
    pub(crate) fn set_offset(&mut self, offset: i32) {
        // Keeping the request before first placement lets builders seed a scroll position without
        // inventing a second authoritative offset in their parent layout state.
        self.offset = offset.max(0);
    }

    /// Returns the largest offset supported by committed geometry.
    pub(crate) fn max_offset(&self) -> i32 {
        self.configuration
            .map_or(0, |configuration| scrollbar_max_scroll(configuration.content_len, configuration.view_len))
    }

    /// Resolves paint and pointer geometry from current state without storing duplicate rectangles.
    fn geometry(&self) -> Option<ScrollbarGeometry> {
        // Derive rectangles on demand from one scalar configuration; paint and input cannot observe
        // independently cached thumb geometry.
        self.configuration.map(|configuration| {
            ScrollbarGeometry::new(
                self.axis,
                configuration.track,
                configuration.view_len,
                configuration.content_len,
                self.offset,
                configuration.min_thumb_len,
            )
        })
    }
}

impl RetainedScrollbar {
    /// Creates one independently targetable scrollbar and its weak layout capability.
    ///
    /// The returned node strongly owns the state through the widget. Its sibling parent layout gets
    /// only a weak handle used to configure range and visibility after measuring scroll content.
    pub(crate) fn create(axis: ScrollAxis) -> (TypedWidgetHandle<Self>, Node) {
        // Start inactive; the parent layout activates the bar only when overflow is committed.
        let widget = Self {
            axis,
            configuration: None,
            offset: 0,
            opt: WidgetOption::NONE,
        };
        Node::typed_widget(widget)
    }
}

impl Widget for RetainedScrollbar {
    fn widget_opt(&self) -> &WidgetOption {
        &self.opt
    }

    fn effective_widget_opt(&self) -> WidgetOption {
        // An unconfigured retained scrollbar is transparent to hit testing without removing it.
        if self.configuration.is_some() {
            self.opt
        } else {
            self.opt | WidgetOption::NO_INTERACT
        }
    }

    fn update(&mut self, ctx: &mut WidgetUpdateCtx<'_>, input: Option<&UiInputEvent>) {
        // Resolve one geometry snapshot for the complete event so hit testing and delta mapping use
        // identical track/thumb/range values.
        let Some(geometry) = self.geometry() else { return };
        match input {
            Some(UiInputEvent::MouseDown { pos, button }) if button.intersects(MouseButton::LEFT) && geometry.track().contains(pos) => {
                // Clicking outside the thumb recenters it. Runtime capture established for this
                // press supplies the complete drag lease through `ctx.active()` below.
                if !geometry.thumb().contains(pos) {
                    self.offset = geometry.centered_offset(*pos);
                }
            }
            Some(UiInputEvent::MouseDrag { delta, .. }) if ctx.active() => {
                // Only the router-owned capture recipient is active. This continues beyond
                // the track rectangle without retaining a second widget-local capture flag.
                // offset = clamp(previous_offset + drag_delta, 0, maximum_offset).
                self.offset = self.offset.saturating_add(geometry.drag_delta(*delta)).clamp(0, self.max_offset());
            }
            _ => {}
        }
    }

    fn paint(&mut self, ctx: &mut WidgetPaintCtx<'_>) {
        // Paint from the same derived geometry used by update; inactive bars emit no operations.
        if let Some(geometry) = self.geometry() {
            ctx.draw_rect(geometry.track(), ctx.style().colors[ControlColor::ScrollBase as usize]);
            ctx.draw_rect(geometry.thumb(), ctx.style().colors[ControlColor::ScrollThumb as usize]);
        }
    }

    fn focus_policy(&self) -> FocusPolicy {
        FocusPolicy::DragCapture
    }
}

impl crate::LeafWidget for RetainedScrollbar {
    fn measure(&self, style: &crate::Style, _atlas: &crate::AtlasHandle, _constraints: crate::Constraints) -> Dimensioni {
        let thickness = style.scrollbar_size.max(0);
        match self.axis {
            ScrollAxis::Horizontal => Dimensioni::new(0, thickness),
            ScrollAxis::Vertical => Dimensioni::new(thickness, 0),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn minimum_thumb_drag_uses_the_same_inverse_range_as_paint() {
        let track = Recti::new(10, 20, 8, 100);
        let geometry = ScrollbarGeometry::new(ScrollAxis::Vertical, track, 20, 200, 90, 40);

        let thumb = geometry.thumb();
        assert_eq!((thumb.x, thumb.y, thumb.width, thumb.height), (10, 50, 8, 40));
        assert_eq!(geometry.drag_delta(Vec2i::new(0, 30)), 90);

        let moved = ScrollbarGeometry::new(ScrollAxis::Vertical, track, 20, 200, 180, 40);
        assert_eq!(moved.thumb().y, geometry.thumb().y + 30);
    }

    #[test]
    fn centered_track_click_clamps_to_both_ends() {
        let track = Recti::new(4, 6, 100, 8);
        let geometry = ScrollbarGeometry::new(ScrollAxis::Horizontal, track, 25, 100, 0, 20);

        assert_eq!(geometry.centered_offset(Vec2i::new(-100, 8)), 0);
        assert_eq!(geometry.centered_offset(Vec2i::new(104, 8)), 75);
    }
}
