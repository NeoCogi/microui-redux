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
//! Standalone retained one-axis scrollbar widget and its shared thumb geometry.
//!
//! The widget owns range state, input, events, and paint while its parent remains responsible for
//! allocating the track. Shared geometry keeps paint, dragging, and track clicks consistent.
use std::{cell::RefCell, rc::Rc};

use crate::math::{clamp_i64_to_i32, RectExt};
use crate::{
    ControlColor, Dimensioni, MouseButton, Node, Recti, TypedWidgetHandle, UiInputEvent, Vec2i, Widget, WidgetOption, WidgetPaintCtx, WidgetParameters,
    WidgetUpdateCtx,
};

#[derive(Copy, Clone, Debug, Eq, PartialEq)]
/// Axis selector used by shared scrollbar geometry helpers.
pub enum ScrollbarAxis {
    /// Vertical scrollbar operating on y offsets.
    Vertical,
    /// Horizontal scrollbar operating on x offsets.
    Horizontal,
}

impl ScrollbarAxis {
    /// Returns the rectangle extent along this scrollbar's movement axis.
    fn rect_len(self, rect: Recti) -> i32 {
        match self {
            Self::Vertical => rect.height,
            Self::Horizontal => rect.width,
        }
    }

    /// Returns one point component along this scrollbar's movement axis.
    fn point(self, point: Vec2i) -> i32 {
        match self {
            Self::Vertical => point.y,
            Self::Horizontal => point.x,
        }
    }

    /// Returns the rectangle origin along this scrollbar's movement axis.
    fn origin(self, rect: Recti) -> i32 {
        match self {
            Self::Vertical => rect.y,
            Self::Horizontal => rect.x,
        }
    }

    /// Replaces the rectangle extent along this scrollbar's movement axis.
    fn with_len(self, mut rect: Recti, len: i32) -> Recti {
        match self {
            Self::Vertical => rect.height = len,
            Self::Horizontal => rect.width = len,
        }
        rect
    }

    /// Translates the rectangle origin along this scrollbar's movement axis.
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
    /// Axis used to select rectangle and pointer components.
    axis: ScrollbarAxis,
    /// Complete local track rectangle.
    track: Recti,
    /// Proportional, minimum-bounded thumb rectangle inside `track`.
    thumb: Recti,
    /// Largest valid content offset.
    max_offset: i32,
    /// Number of pixels through which the thumb can move.
    thumb_travel: i32,
}

/// Multiplies before dividing in a wider domain and clamps the signed result to one coordinate.
fn scaled_ratio(value: i32, numerator: i32, denominator: i32) -> i32 {
    debug_assert!(numerator >= 0 && denominator > 0);
    // Every i32 product fits i64. Dividing before narrowing preserves proportional mapping at
    // large content ranges instead of saturating the intermediate product and destroying its ratio.
    clamp_i64_to_i32(i64::from(value) * i64::from(numerator) / i64::from(denominator))
}

impl ScrollbarGeometry {
    /// Resolves one track and thumb for the supplied visible/content lengths and offset.
    pub(crate) fn new(axis: ScrollbarAxis, track: Recti, view_len: i32, content_len: i32, offset: i32, min_thumb_len: i32) -> Self {
        // Normalize every external length once so later mapping functions operate on valid ranges.
        let track_len = axis.rect_len(track).max(0);
        let view_len = view_len.max(0);
        let content_len = content_len.max(0);
        let max_offset = scrollbar_max_scroll(content_len, view_len);

        // Proportional length represents the visible fraction; the style minimum preserves usability.
        let proportional = if content_len > 0 {
            // proportional_thumb_length = track_length * visible_length / content_length.
            scaled_ratio(track_len, view_len, content_len)
        } else {
            track_len
        };
        let thumb_len = proportional.max(min_thumb_len.max(0)).min(track_len);
        // thumb_travel = track_length - thumb_length.
        let thumb_travel = track_len.saturating_sub(thumb_len).max(0);
        // Map clamped content offset into thumb travel using the same integer ratio inverted by drag.
        let thumb_offset = if max_offset > 0 && thumb_travel > 0 {
            // thumb_offset = clamped_content_offset * thumb_travel / maximum_content_offset.
            scaled_ratio(offset.clamp(0, max_offset), thumb_travel, max_offset)
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
        scaled_ratio(self.axis.point(delta), self.max_offset, self.thumb_travel)
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
        scaled_ratio(centered, self.max_offset, self.thumb_travel)
    }
}

/// Returns the largest scroll offset needed to reveal all content.
pub(crate) fn scrollbar_max_scroll(content_len: i32, view_len: i32) -> i32 {
    // maximum_scroll = max(content_length - visible_length, 0).
    content_len.saturating_sub(view_len).max(0)
}

/// One-shot construction input for a standalone [`Scrollbar`].
pub struct ScrollbarParameters {
    /// Axis along which the scrollbar changes its content offset.
    pub axis: ScrollbarAxis,
    /// Initial visible content length represented by the complete track.
    pub viewport_len: i32,
    /// Initial total content length represented by the track and thumb.
    pub content_len: i32,
    /// Initial content offset, clamped to the configured range.
    pub offset: i32,
    /// Base widget options applied in addition to focus-preserving pointer interaction.
    pub opt: WidgetOption,
}

impl WidgetParameters for ScrollbarParameters {}

impl ScrollbarParameters {
    /// Creates an empty scrollbar on `axis`; callers or a containing layout can install lengths later.
    pub fn new(axis: ScrollbarAxis) -> Self {
        // An empty range keeps a newly mounted standalone scrollbar inert until its owner provides
        // meaningful viewport and content lengths.
        Self {
            axis,
            viewport_len: 0,
            content_len: 0,
            offset: 0,
            opt: WidgetOption::NONE,
        }
    }

    /// Replaces the initial viewport length, content length, and requested offset.
    pub const fn range(mut self, viewport_len: i32, content_len: i32, offset: i32) -> Self {
        // Preserve the caller's raw request here; Scrollbar construction performs one authoritative
        // normalization and clamp after all parameters have been assembled.
        self.viewport_len = viewport_len;
        self.content_len = content_len;
        self.offset = offset;
        self
    }

    /// Replaces the base widget options while retaining scrollbar-specific focus behavior.
    pub const fn with_opt(mut self, opt: WidgetOption) -> Self {
        // Store only caller-authored options. Scrollbar::create adds the mandatory preserve-focus
        // behavior so an option replacement cannot accidentally make a bar steal editor focus.
        self.opt = opt;
        self
    }
}

/// Offset snapshot emitted after a user-originated scrollbar movement.
#[derive(Copy, Clone, Debug, Eq, PartialEq)]
pub struct ScrollbarChanged {
    /// Clamped content offset after applying the triggering pointer event.
    pub offset: i32,
}

impl crate::WidgetEvent for ScrollbarChanged {}

/// Standalone one-axis scrollbar whose allocation supplies its complete track rectangle.
pub struct Scrollbar {
    axis: ScrollbarAxis,
    viewport_len: i32,
    content_len: i32,
    offset: i32,
    opt: WidgetOption,
    changed_event: Rc<RefCell<crate::event::WidgetEventPort<ScrollbarChanged>>>,
}

impl Scrollbar {
    /// Creates a standalone scrollbar node and a weak typed handle to its retained state.
    pub fn create(parameters: ScrollbarParameters) -> (TypedWidgetHandle<Self>, Node) {
        // Normalize all range inputs before mounting so the first measure, update, and paint phases
        // observe one coherent range and offset.
        let viewport_len = parameters.viewport_len.max(0);
        let content_len = parameters.content_len.max(0);
        let maximum = scrollbar_max_scroll(content_len, viewport_len);
        let widget = Self {
            axis: parameters.axis,
            viewport_len,
            content_len,
            offset: parameters.offset.clamp(0, maximum),
            opt: parameters.opt,
            changed_event: Rc::new(RefCell::new(crate::event::WidgetEventPort::new())),
        };
        Node::typed_widget(widget)
    }

    /// Returns the axis configured when this scrollbar was constructed.
    pub const fn axis(&self) -> ScrollbarAxis {
        // Axis is immutable semantic configuration, so returning it never depends on layout state.
        self.axis
    }

    /// Installs non-negative viewport and content lengths and clamps the previous offset.
    pub fn set_lengths(&mut self, viewport_len: i32, content_len: i32) {
        // Normalize both external lengths together before deriving a maximum. This prevents paint
        // and pointer input from observing a partially updated range.
        self.viewport_len = viewport_len.max(0);
        self.content_len = content_len.max(0);
        // offset = clamp(previous_offset, 0, maximum_offset).
        self.offset = self.offset.clamp(0, self.max_offset());
    }

    /// Clears the range and offset before a containing layout hides this retained widget.
    pub(crate) fn deactivate(&mut self) {
        // Reset semantic range state synchronously. Runtime-owned pointer capture is invalidated by
        // the subsequent hidden participation decision and needs no widget-local drag flag.
        self.viewport_len = 0;
        self.content_len = 0;
        self.offset = 0;
    }

    /// Returns the current clamped content offset on this axis.
    pub const fn offset(&self) -> i32 {
        // Offset is clamped by construction, range updates, and user interaction.
        self.offset
    }

    /// Requests a non-negative offset that the next range update can clamp further.
    pub fn set_offset(&mut self, offset: i32) {
        // Preserve an above-range request so a containing layout can set an offset before installing
        // newly measured lengths. The next set_lengths call performs the authoritative upper clamp.
        self.offset = offset.max(0);
    }

    /// Returns the largest offset supported by the current viewport and content lengths.
    pub fn max_offset(&self) -> i32 {
        // Derive the maximum instead of retaining a second range value that could become stale.
        scrollbar_max_scroll(self.content_len, self.viewport_len)
    }

    /// Returns the native event endpoint emitted after every user-originated offset change.
    pub fn changed(&self) -> crate::WidgetEventPortHandle<ScrollbarChanged> {
        // Delegate to the typed-event implementation so direct widgets and typed handles expose the
        // same runtime-owned endpoint.
        <Self as crate::TypedWidget<ScrollbarChanged>>::event(self)
    }

    /// Resolves track and thumb geometry from the current allocation and style-owned minimum.
    fn geometry(&self, track: Recti, min_thumb_len: i32) -> ScrollbarGeometry {
        // The retained node allocation is the standalone widget's authoritative track. Range and
        // offset remain semantic state, so no containing layout must write widget-local rectangles.
        ScrollbarGeometry::new(self.axis, track, self.viewport_len, self.content_len, self.offset, min_thumb_len)
    }
}

impl TypedWidgetHandle<Scrollbar> {
    /// Returns the current offset while the scrollbar remains mounted.
    pub fn offset(&self) -> Option<i32> {
        // Read through the weak typed capability without extending the node's ownership lifetime.
        self.try_read(Scrollbar::offset)
    }

    /// Replaces the requested offset without emitting a user-originated change event.
    pub fn set_offset(&self, offset: i32) -> Option<()> {
        // Programmatic synchronization is silent so composite owners do not receive feedback loops.
        self.try_update_without_measurement(|scrollbar| scrollbar.set_offset(offset))
    }

    /// Replaces the current viewport and content lengths.
    pub fn set_lengths(&self, viewport_len: i32, content_len: i32) -> Option<()> {
        // Range changes affect thumb geometry but not the scrollbar's intrinsic thickness.
        self.try_update_without_measurement(|scrollbar| scrollbar.set_lengths(viewport_len, content_len))
    }

    /// Returns the scrollbar's native offset-change endpoint.
    pub fn changed(&self) -> crate::WidgetEventPortHandle<ScrollbarChanged> {
        // Resolve the event endpoint through the generic typed-widget helper.
        self.widget_event()
    }
}

impl crate::TypedWidget<ScrollbarChanged> for Scrollbar {
    /// Returns the runtime-owned event endpoint for user-originated offset changes.
    fn event(&self) -> crate::WidgetEventPortHandle<ScrollbarChanged> {
        // Clone only a weak view of the shared port; the retained scrollbar remains its sole owner.
        crate::WidgetEventPortHandle::new(&self.changed_event)
    }
}

impl Widget for Scrollbar {
    /// Returns the base options used by generic retained input routing.
    fn widget_opt(&self) -> &WidgetOption {
        // The stored options already include focus preservation established during construction.
        &self.opt
    }

    /// Disables pointer interaction while the scrollbar has no movable content range.
    fn effective_widget_opt(&self) -> WidgetOption {
        // A zero range can still paint a complete thumb when mounted standalone, but accepting a
        // press would create meaningless pointer capture.
        if self.max_offset() > 0 {
            self.opt
        } else {
            self.opt | WidgetOption::NO_INTERACT
        }
    }

    /// Applies track clicks and captured thumb dragging to the current offset.
    fn update(&mut self, ctx: &mut WidgetUpdateCtx<'_>, input: Option<&UiInputEvent>) {
        // Resolve one geometry snapshot from this widget's local allocation so hit testing and drag
        // conversion use the exact same track and thumb for the complete event.
        let geometry = self.geometry(ctx.local_rect(), ctx.style().thumb_size.max(0));
        let previous = self.offset;
        match input {
            Some(UiInputEvent::MouseDown { pos, button }) if button.intersects(MouseButton::LEFT) && geometry.track().contains_point(*pos) => {
                // Clicking outside the thumb recenters it. Runtime capture established for this
                // press supplies the complete drag lease through `ctx.active()` below.
                if !geometry.thumb().contains_point(*pos) {
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
        if self.offset != previous {
            // Publish only user-originated movement. Programmatic setters intentionally remain
            // silent so owners can synchronize range and offset without recursive notifications.
            self.changed_event.borrow_mut().emit(ScrollbarChanged { offset: self.offset });
        }
    }

    /// Paints the track and proportional thumb inside the complete local allocation.
    fn paint(&mut self, ctx: &mut WidgetPaintCtx<'_>) {
        // Rebuild geometry from the paint allocation and the same semantic range used by update.
        // Hidden composite children never reach this phase, while mounted zero-range bars display a
        // full-length thumb that communicates the absence of overflow.
        let geometry = self.geometry(ctx.local_rect(), ctx.style().thumb_size.max(0));
        ctx.draw_rect(geometry.track(), ctx.style().colors[ControlColor::ScrollBase as usize]);
        ctx.draw_rect(geometry.thumb(), ctx.style().colors[ControlColor::ScrollThumb as usize]);
    }
}

impl crate::LeafWidget for Scrollbar {
    /// Reports zero length along the scroll axis and style-owned thickness across it.
    fn measure(&self, style: &crate::Style, _atlas: &crate::AtlasHandle, _constraints: crate::Constraints) -> Dimensioni {
        // A containing layout supplies track length explicitly; intrinsic measurement contributes
        // only the cross-axis thickness required by standalone layout composition.
        let thickness = style.scrollbar_size.max(0);
        match self.axis {
            ScrollbarAxis::Horizontal => Dimensioni::new(0, thickness),
            ScrollbarAxis::Vertical => Dimensioni::new(thickness, 0),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn minimum_thumb_drag_uses_the_same_inverse_range_as_paint() {
        let track = Recti::new(10, 20, 8, 100);
        let geometry = ScrollbarGeometry::new(ScrollbarAxis::Vertical, track, 20, 200, 90, 40);

        let thumb = geometry.thumb();
        assert_eq!((thumb.x, thumb.y, thumb.width, thumb.height), (10, 50, 8, 40));
        assert_eq!(geometry.drag_delta(Vec2i::new(0, 30)), 90);

        let moved = ScrollbarGeometry::new(ScrollbarAxis::Vertical, track, 20, 200, 180, 40);
        assert_eq!(moved.thumb().y, geometry.thumb().y + 30);
    }

    #[test]
    fn centered_track_click_clamps_to_both_ends() {
        let track = Recti::new(4, 6, 100, 8);
        let geometry = ScrollbarGeometry::new(ScrollbarAxis::Horizontal, track, 25, 100, 0, 20);

        assert_eq!(geometry.centered_offset(Vec2i::new(-100, 8)), 0);
        assert_eq!(geometry.centered_offset(Vec2i::new(104, 8)), 75);
    }

    /// Verifies large content ranges retain their proportional thumb position and inverse drag
    /// mapping instead of saturating the multiplication before division.
    #[test]
    fn extreme_content_range_preserves_scroll_ratios() {
        let track = Recti::new(10, 20, 300, 8);
        let view_len = 300;
        let content_len = i32::MAX;
        let max_offset = scrollbar_max_scroll(content_len, view_len);
        let offset = max_offset / 2;
        let geometry = ScrollbarGeometry::new(ScrollbarAxis::Horizontal, track, view_len, content_len, offset, 8);
        let thumb = geometry.thumb();

        assert_eq!(thumb.width, 8);
        assert!(
            (i64::from(thumb.x) - i64::from(10 + 146)).abs() <= 1,
            "integer division may place the half-range thumb one pixel before the exact midpoint"
        );
        let inverse = geometry.drag_delta(Vec2i::new(146, 0));
        assert!(
            (i64::from(inverse) - i64::from(offset)).abs() <= 1,
            "half-track drag must map back to half the content range"
        );
        assert!((i64::from(geometry.centered_offset(Vec2i::new(10 + 146 + 4, 20))) - i64::from(offset)).abs() <= 1);
    }
}
