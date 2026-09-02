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

use crate::{SurfaceRole};

use std::{cell::RefCell, rc::Rc};

use bitflags::bitflags;

use crate::ui_node::widgets::{Scrollbar, ScrollbarAxis, ScrollbarParameters};
use crate::{
    AppearanceRole, ChildParticipation, Container, ContainerWidget, Dimensioni, MeasureCtx, Recti, TypedWidgetHandle, UiInputEvent, Vec2i, Widget,
    WidgetOption, WidgetPaintCtx, WidgetParameters, WidgetUpdateCtx,
};

use super::{Children, ContainerLayoutCtx, Node};

/// Returns the child allocation for one scrollbar adjacent to the resolved content body.
fn scrollbar_track(axis: ScrollbarAxis, body: Recti, scrollbar_size: i32) -> Recti {
    // Start with the body so the track preserves its cross-axis origin and extent.
    let mut track = body;
    match axis {
        ScrollbarAxis::Vertical => {
            // vertical_track_x = body_x + body_width.
            track.x = body.x.saturating_add(body.width);
            track.width = scrollbar_size;
        }
        ScrollbarAxis::Horizontal => {
            // horizontal_track_y = body_y + body_height.
            track.y = body.y.saturating_add(body.height);
            track.height = scrollbar_size;
        }
    }
    track
}

bitflags! {
    #[derive(Copy, Clone)]
    /// Options fixed when a retained scroll area is constructed.
    pub struct ScrollAreaOption : u32 {
        /// Gives the scroll area a Skin-owned outer border and inset content area.
        const FRAME = 1024;
        /// Enables scrolling and scrollbars initially.
        const ENABLE_SCROLL = 32;
        /// No special options.
        const NONE = 0;
    }
}

/// One-shot construction input for a retained scroll area.
pub struct ScrollAreaParameters {
    /// The ordinary content node viewed through the scroll surface.
    content: Node,
    /// Immutable presentation options for the completed composite.
    opt: ScrollAreaOption,
}

impl WidgetParameters for ScrollAreaParameters {}

impl ScrollAreaParameters {
    /// Creates a scroll area that owns one ordinary content node.
    ///
    /// The content fills at least the complete viewport, while either desired extent may remain
    /// larger and create overflow. Use explicit tracks inside the content container to express fixed
    /// or flexible descendants; ScrollArea adds no additional child sizing policy.
    pub fn new(opt: ScrollAreaOption, content: Node) -> Self {
        Self { content, opt }
    }
}

/// Geometry retained only to describe the latest committed scroll surface.
#[derive(Copy, Clone, Debug, Default)]
struct ScrollAreaGeometry {
    /// Complete widget-local surface, excluding an optional generic frame.
    surface: Recti,
    /// Clamped content translation represented as positive scroll coordinates.
    offset: Vec2i,
    /// Largest committed offset on each axis.
    max_offset: Vec2i,
    /// Content-local viewport extent used to resolve scroll-into-view requests.
    viewport: Dimensioni,
    /// Parent-local allocation of the vertical scrollbar child.
    vertical: Option<Recti>,
    /// Parent-local allocation of the horizontal scrollbar child.
    horizontal: Option<Recti>,
    /// Non-interactive gap between two visible scrollbar children.
    corner: Option<Recti>,
}

/// Insets a non-negative rectangle without allowing either axis to underflow.
fn inset_rect(rect: Recti, amount: i32) -> Recti {
    let amount = amount.max(0);
    // inset_x/y = min(requested_inset, non_negative_axis_extent).
    let inset_x = amount.min(rect.width.max(0));
    let inset_y = amount.min(rect.height.max(0));
    // inset_extent = leading_inset + trailing_inset = amount * 2.
    let inset_extent = amount.saturating_mul(2);
    // content_origin = rectangle_origin + clamped_inset.
    let x = rect.x.saturating_add(inset_x);
    let y = rect.y.saturating_add(inset_y);
    // content_extent = max(rectangle_extent - inset_extent, 0).
    let width = rect.width.saturating_sub(inset_extent).max(0);
    let height = rect.height.saturating_sub(inset_extent).max(0);
    Recti::new(x, y, width, height)
}

/// Returns the nearest non-negative offset that reveals one interval inside a viewport.
fn offset_to_reveal_interval(offset: i32, viewport_len: i32, interval_start: i32, interval_len: i32) -> i32 {
    // A zero-size viewport cannot reveal content. Preserve the existing request so a later layout
    // with usable geometry remains authoritative.
    if viewport_len <= 0 {
        return offset.max(0);
    }

    // Normalize the requested interval before comparing it with the currently visible range.
    let start = interval_start.max(0);
    let end = start.saturating_add(interval_len.max(0));
    let visible_start = offset.max(0);
    let visible_end = visible_start.saturating_add(viewport_len);

    if start < visible_start {
        // Reveal content before the viewport by aligning its leading edge.
        start
    } else if end > visible_end {
        // Reveal content after the viewport by aligning its trailing edge. Do not clamp against the
        // last committed content range because an editor may have expanded before the next layout.
        end.saturating_sub(viewport_len).max(0)
    } else {
        // Keep the current offset when the complete requested interval is already visible.
        visible_start
    }
}

/// Concrete application-facing widget for the three-child ScrollArea composite.
pub struct ScrollArea {
    /// Weak typed widget capability for the horizontal scrollbar child.
    horizontal: TypedWidgetHandle<Scrollbar>,
    /// Weak typed widget capability for the vertical scrollbar child.
    vertical: TypedWidgetHandle<Scrollbar>,
    /// Dynamic participation policy shared by surface and layout.
    scrolling_enabled: bool,
    /// Latest geometry summary; interactive state remains in the child widgets.
    geometry: ScrollAreaGeometry,
    /// Optional generic frame plus the dynamic wheel option.
    opt: WidgetOption,
}

impl ScrollArea {
    /// Returns the offsets owned by the two scrollbar child widgets.
    pub fn offset(&self) -> Vec2i {
        // The two standalone scrollbar children remain the single source of interactive offset
        // state, so the parent derives its vector instead of retaining a duplicate value.
        Vec2i::new(Self::axis_offset(&self.horizontal), Self::axis_offset(&self.vertical))
    }

    /// Requests a non-negative offset; the next placement clamps it to current content geometry.
    pub fn set_offset(&mut self, offset: Vec2i) {
        // Disabled scrolling owns a stable zero offset; enabled scrolling accepts requests that the
        // next placement clamps against newly measured content ranges.
        let offset = if self.scrolling_enabled {
            Vec2i::new(offset.x.max(0), offset.y.max(0))
        } else {
            Vec2i::default()
        };
        Self::set_axis_offset(&self.horizontal, offset.x);
        Self::set_axis_offset(&self.vertical, offset.y);
    }

    /// Requests the largest vertical offset while preserving the horizontal offset.
    ///
    /// The content and viewport heights may change before the next placement, so this records an
    /// unbounded request rather than using the currently committed maximum. Placement clamps it to
    /// the maximum derived from the newly measured content.
    pub fn scroll_to_end(&mut self) {
        // Record an intentionally unbounded request so content appended before the next placement
        // determines the final vertical maximum.
        if self.scrolling_enabled {
            Self::set_axis_offset(&self.vertical, i32::MAX);
        }
    }

    /// Requests the nearest offsets that reveal `rect` in content-local coordinates.
    pub fn scroll_rect_into_view(&mut self, rect: Recti) {
        // Ignore requests while scrolling is disabled; enabling later starts from the documented
        // zero offset rather than reviving an editor's stale caret request.
        if !self.scrolling_enabled {
            return;
        }

        // Resolve both axes independently against the latest committed viewport. The scrollbar
        // children retain requests above their old maximum until the next layout installs ranges
        // derived from potentially changed content.
        let offset = self.offset();
        let viewport = self.geometry.viewport;
        self.set_offset(Vec2i::new(
            offset_to_reveal_interval(offset.x, viewport.width, rect.x, rect.width),
            offset_to_reveal_interval(offset.y, viewport.height, rect.y, rect.height),
        ));
    }

    /// Returns whether layout may activate the scrollbar children.
    pub fn scrolling_enabled(&self) -> bool {
        // Enablement belongs to the composite parent because it controls wheel routing and both
        // structural scrollbar participation decisions.
        self.scrolling_enabled
    }

    /// Enables or disables scrolling and its two interactive children.
    pub fn set_scrolling_enabled(&mut self, enabled: bool) {
        // Commit the policy before synchronizing child state so every following query observes one
        // coherent enabled or disabled composite.
        self.scrolling_enabled = enabled;
        if !enabled {
            // Reset child-owned interaction synchronously so capture cannot survive deactivation.
            Self::reset_axis(&self.horizontal);
            Self::reset_axis(&self.vertical);
            self.geometry.offset = Vec2i::default();
            self.geometry.max_offset = Vec2i::default();
            self.geometry.viewport = Dimensioni::default();
            self.geometry.horizontal = None;
            self.geometry.vertical = None;
            self.geometry.corner = None;
        }
    }

    /// Reports whether applying both components of one wheel event changes either child offset.
    fn accepts_scroll_delta(&self, delta: Vec2i) -> bool {
        if !self.scrolling_enabled {
            return false;
        }
        let offset = self.offset();
        let maximum = self.max_offset();
        // next_offset = clamp(previous_offset + wheel_delta, zero, maximum_offset).
        let next = Vec2i::new(
            offset.x.saturating_add(delta.x).clamp(0, maximum.x),
            offset.y.saturating_add(delta.y).clamp(0, maximum.y),
        );
        next.x != offset.x || next.y != offset.y
    }

    /// Applies one atomic two-axis wheel delta against committed scrollbar ranges.
    fn scroll_by(&mut self, delta: Vec2i) -> bool {
        if !self.accepts_scroll_delta(delta) {
            return false;
        }
        let offset = self.offset();
        let maximum = self.max_offset();
        // next_offset = clamp(previous_offset + wheel_delta, zero, maximum_offset).
        self.set_offset(Vec2i::new(
            offset.x.saturating_add(delta.x).clamp(0, maximum.x),
            offset.y.saturating_add(delta.y).clamp(0, maximum.y),
        ));
        true
    }

    /// Reads one live structural scrollbar without taking ownership of its state.
    fn axis_offset(handle: &TypedWidgetHandle<Scrollbar>) -> i32 {
        handle
            .try_read(Scrollbar::offset)
            .expect("ScrollArea structural scrollbar must outlive its parent state")
    }

    /// Writes a requested offset through the scrollbar's weak typed widget capability.
    fn set_axis_offset(handle: &TypedWidgetHandle<Scrollbar>, offset: i32) {
        handle
            .try_update_without_measurement(|state| state.set_offset(offset))
            .expect("ScrollArea structural scrollbar must be available outside traversal")
    }

    /// Returns both committed ranges without copying geometry into interactive state.
    fn max_offset(&self) -> Vec2i {
        Vec2i::new(
            self.horizontal.try_read(Scrollbar::max_offset).unwrap_or(0),
            self.vertical.try_read(Scrollbar::max_offset).unwrap_or(0),
        )
    }

    /// Clears one scrollbar's offset and committed geometry.
    fn reset_axis(handle: &TypedWidgetHandle<Scrollbar>) {
        // Pointer capture is runtime-owned, so deactivation has no widget-local drag lease to
        // clear; hidden participation invalidates the corresponding runtime identity during layout.
        handle
            .try_update_without_measurement(|state| {
                state.set_offset(0);
                state.deactivate();
            })
            .expect("ScrollArea structural scrollbar must be available outside traversal");
    }
}

impl Widget for ScrollArea {
    fn widget_opt(&self) -> &WidgetOption {
        // These are presentation defaults; effective options below add dynamic scroll eligibility.
        &self.opt
    }

    fn frame_appearance_role(&self) -> AppearanceRole {
        // A framed scroll area uses the same panel patch whose center fills its viewport.
        AppearanceRole::Surface(SurfaceRole::Panel)
    }

    fn effective_widget_opt(&self) -> WidgetOption {
        if self.scrolling_enabled {
            self.opt | WidgetOption::GRAB_SCROLL
        } else {
            self.opt | WidgetOption::NO_INTERACT
        }
    }

    fn update(&mut self, _ctx: &mut WidgetUpdateCtx<'_>, input: Option<&UiInputEvent>) {
        // Routing guarantees that only an accepted wheel event reaches this surface.
        let Some(UiInputEvent::Scroll { delta, .. }) = input else { return };
        self.scroll_by(*delta);
    }

    fn paint(&mut self, ctx: &mut WidgetPaintCtx<'_>) {
        // The parent paints only its panel and non-interactive corner; scrollbar children paint
        // their own tracks and thumbs afterward in ordinary child paint order.
        let fallback = ctx.local_rect();
        let (surface, corner) = (self.geometry.surface, self.geometry.corner);
        // Before first placement the summary is empty; the update/layout contract normally commits
        // geometry before paint, while the fallback keeps direct widget tests well-defined.
        let surface = if surface.width > 0 || surface.height > 0 { surface } else { fallback };
        // Panel fill is passive container structure. Scrollbars remain independently interactive,
        // but hovering or dragging anywhere inside the viewport must not recolor its background.
        ctx.draw_appearance_center_state(AppearanceRole::Surface(SurfaceRole::Panel), crate::VisualState::Normal, surface);
        if let Some(corner) = corner {
            ctx.draw_appearance_center_state(AppearanceRole::Surface(SurfaceRole::Panel), crate::VisualState::Normal, corner);
        }
    }
}

/// The transform boundary between the viewport and its ordinary content node.
struct ScrollSurface {
    /// Scroll translation is read from the real horizontal child widget.
    horizontal: TypedWidgetHandle<Scrollbar>,
    /// Scroll translation is read from the real vertical child widget.
    vertical: TypedWidgetHandle<Scrollbar>,
}

impl ContainerWidget for ScrollSurface {
    /// Measures the ordinary content with bounded width and unbounded height.
    fn measure(&self, ctx: &mut MeasureCtx<'_>, constraints: crate::Constraints) -> Dimensioni {
        // Forward the width contract so wrapping content resolves against its eventual viewport;
        // height remains intrinsic so the owning ScrollArea can detect vertical overflow.
        ctx.measure_child(0, crate::Constraints::new(constraints.width, crate::AvailableSpace::Unbounded))
            .unwrap_or_default()
    }

    /// Places content at least as large as the viewport and applies the scrollbar translation.
    fn place(&mut self, ctx: &mut ContainerLayoutCtx<'_>, children: &mut Children, rect: Recti) {
        // Re-measure with the exact viewport width selected after scrollbar convergence so wrapped
        // content and the final allocation use the same horizontal constraint.
        let preferred = ctx
            .measure_child(
                children,
                0,
                crate::Constraints::new(crate::AvailableSpace::bounded(rect.width), crate::AvailableSpace::Unbounded),
            )
            .unwrap_or_default();
        // Fill both viewport axes so an interactive child owns blank trailing space. Desired extents
        // still win when larger, preserving overflow and its associated scrollbar range.
        let content_rect = Recti::new(0, 0, rect.width.max(preferred.width).max(0), rect.height.max(preferred.height).max(0));
        let _ = ctx.layout_child(children, 0, content_rect);
        // Read clamped values only after range configuration and translate descendants without
        // changing their stable content-local allocations.
        let offset = Vec2i::new(ScrollArea::axis_offset(&self.horizontal), ScrollArea::axis_offset(&self.vertical));
        ctx.set_children_viewport(rect, Vec2i::new(-offset.x, -offset.y));
        ctx.set_content_size(Dimensioni::new(content_rect.width, content_rect.height));
        ctx.set_child_overflow_propagation(false);
    }
}

impl Widget for ScrollSurface {
    /// Marks the transform-only surface as transparent to direct pointer targeting.
    fn widget_opt(&self) -> &WidgetOption {
        // Eligible content descendants remain targetable even though this structural surface has no
        // independent interaction behavior.
        &WidgetOption::NO_INTERACT
    }

    /// Performs no semantic update because ScrollArea and its children own all scrolling state.
    fn update(&mut self, _ctx: &mut WidgetUpdateCtx<'_>, _input: Option<&UiInputEvent>) {}

    /// Emits no paint operations because clipping and translation are traversal geometry.
    fn paint(&mut self, _ctx: &mut WidgetPaintCtx<'_>) {}
}

impl ScrollArea {
    /// Transform surface containing the application-provided content node.
    const SURFACE: usize = 0;
    /// Independently targetable horizontal scrollbar widget.
    const HORIZONTAL: usize = 1;
    /// Independently targetable vertical scrollbar widget.
    const VERTICAL: usize = 2;

    /// Configures one visible scrollbar using widget-local track coordinates.
    fn configure_bar(handle: &TypedWidgetHandle<Scrollbar>, view_len: i32, content_len: i32, requested_offset: i32) {
        // Apply the requested offset before installing the new range; set_lengths performs the final
        // clamp while the scrollbar derives its track and minimum thumb from its own phase context.
        handle
            .try_update_without_measurement(|state| {
                state.set_offset(requested_offset);
                state.set_lengths(view_len, content_len);
            })
            .expect("ScrollArea layout requires its retained scrollbar child");
    }

    /// Deactivates one hidden scrollbar without removing its strong child owner.
    fn deactivate_bar(handle: &TypedWidgetHandle<Scrollbar>) {
        // Retain the child node and handle identity while clearing geometry, offset, and drag state.
        handle
            .try_update_without_measurement(Scrollbar::deactivate)
            .expect("ScrollArea layout requires its retained scrollbar child");
    }
}

impl ContainerWidget for ScrollArea {
    /// Measures content through the surface and includes Skin-owned viewport padding.
    fn measure(&self, ctx: &mut MeasureCtx<'_>, constraints: crate::Constraints) -> Dimensioni {
        // Scrollbars are responsive affordances and do not inflate intrinsic composite size.
        let padding = ctx.skin().metrics.padding.max(0);
        // inset = leading_padding + trailing_padding = padding * 2.
        let inset = padding.saturating_mul(2);
        let content_width = match constraints.width {
            crate::AvailableSpace::Bounded(width) => crate::AvailableSpace::bounded(width.saturating_sub(inset)),
            crate::AvailableSpace::Unbounded => crate::AvailableSpace::Unbounded,
        };
        let content = ctx
            .measure_child(Self::SURFACE, crate::Constraints::new(content_width, crate::AvailableSpace::Unbounded))
            .unwrap_or_default();
        // preferred_extent = content_extent + leading_padding + trailing_padding.
        Dimensioni::new(content.width.saturating_add(inset), content.height.saturating_add(inset))
    }

    /// Places Skin-padded content and any required scrollbars inside the local surface.
    fn place(&mut self, ctx: &mut ContainerLayoutCtx<'_>, children: &mut Children, rect: Recti) {
        // All committed summary geometry is parent-local; child scrollbar geometry passed to each
        // widget is normalized to that child's own local track by configure_bar.
        let surface = Recti::new(0, 0, rect.width.max(0), rect.height.max(0));
        let padding = ctx.skin().metrics.padding.max(0);
        let bar_size = ctx.skin().metrics.scrollbar_size.max(0);
        let enabled = self.scrolling_enabled;
        let requested = Vec2i::new(ScrollArea::axis_offset(&self.horizontal), ScrollArea::axis_offset(&self.vertical));
        let bars_usable = enabled && bar_size > 0 && surface.width > 0 && surface.height > 0;
        let mut has_horizontal = bars_usable && self.geometry.horizontal.is_some();
        let mut has_vertical = bars_usable && self.geometry.vertical.is_some();
        let mut visited_states = 0_u8;

        // Start from the last committed state so an unchanged retained tree keeps the same width
        // constraint and reuses its child-measurement cache. Candidate states measure the surface;
        // no child is placed until the final scrollbar presence has converged. There are only four
        // states; if perpendicular induction creates a two-state cycle, commit their union.
        let mut resolved = None;
        for _ in 0..5 {
            let state_bit = 1_u8 << ((has_horizontal as u8) | ((has_vertical as u8) << 1));
            visited_states |= state_bit;
            let vertical_width = if has_vertical { bar_size.min(surface.width) } else { 0 };
            let horizontal_height = if has_horizontal { bar_size.min(surface.height) } else { 0 };
            // body_extent = surface_extent - occupied_scrollbar_extent.
            let body = Recti::new(
                0,
                0,
                surface.width.saturating_sub(vertical_width).max(0),
                surface.height.saturating_sub(horizontal_height).max(0),
            );
            // Inset content by the global Skin padding while keeping scrollbar tracks adjacent to
            // the complete body.
            let view = inset_rect(body, padding);
            let preferred = ctx
                .measure_child(
                    children,
                    Self::SURFACE,
                    crate::Constraints::new(crate::AvailableSpace::bounded(view.width), crate::AvailableSpace::Unbounded),
                )
                .unwrap_or_default();
            // Fill the viewport on both axes while preserving larger desired extents. Comparing this
            // filled extent with the view still detects overflow exactly when preferred content is
            // larger, and it matches ScrollSurface's final allocation contract.
            let extent = Dimensioni::new(view.width.max(preferred.width).max(0), view.height.max(preferred.height).max(0));

            let next_vertical = bars_usable && extent.height > view.height;
            let next_horizontal = bars_usable && extent.width > view.width;
            if next_vertical != has_vertical || next_horizontal != has_horizontal {
                let next_bit = 1_u8 << ((next_horizontal as u8) | ((next_vertical as u8) << 1));
                if visited_states & next_bit != 0 {
                    let previous = (has_horizontal, has_vertical);
                    has_horizontal |= next_horizontal;
                    has_vertical |= next_vertical;
                    if (has_horizontal, has_vertical) != previous {
                        continue;
                    }
                } else {
                    has_horizontal = next_horizontal;
                    has_vertical = next_vertical;
                    continue;
                }
            }

            resolved = Some((body, view, extent));
            break;
        }

        let (body, view, extent) = resolved.expect("ScrollArea scrollbar presence must converge across four possible states");
        let vertical_width = if has_vertical { bar_size.min(surface.width) } else { 0 };
        let horizontal_height = if has_horizontal { bar_size.min(surface.height) } else { 0 };
        let vertical_track = scrollbar_track(ScrollbarAxis::Vertical, body, vertical_width);
        let horizontal_track = scrollbar_track(ScrollbarAxis::Horizontal, body, horizontal_height);
        let vertical_visible = has_vertical && vertical_track.width > 0 && vertical_track.height > 0;
        let horizontal_visible = has_horizontal && horizontal_track.width > 0 && horizontal_track.height > 0;

        if vertical_visible {
            // The scrollbar remains a real independently targetable child when active.
            Self::configure_bar(&self.vertical, view.height, extent.height, requested.y);
            let _ = ctx.set_child_participation(children, Self::VERTICAL, ChildParticipation::Active);
            let _ = ctx.layout_child(
                children,
                Self::VERTICAL,
                Recti::new(
                    rect.x + vertical_track.x,
                    rect.y + vertical_track.y,
                    vertical_track.width,
                    vertical_track.height,
                ),
            );
        } else {
            // Hidden participation removes the retained bar from all ordinary runtime phases.
            Self::deactivate_bar(&self.vertical);
            let _ = ctx.set_child_participation(children, Self::VERTICAL, ChildParticipation::Hidden);
        }

        if horizontal_visible {
            Self::configure_bar(&self.horizontal, view.width, extent.width, requested.x);
            let _ = ctx.set_child_participation(children, Self::HORIZONTAL, ChildParticipation::Active);
            let _ = ctx.layout_child(
                children,
                Self::HORIZONTAL,
                Recti::new(
                    rect.x + horizontal_track.x,
                    rect.y + horizontal_track.y,
                    horizontal_track.width,
                    horizontal_track.height,
                ),
            );
        } else {
            Self::deactivate_bar(&self.horizontal);
            let _ = ctx.set_child_participation(children, Self::HORIZONTAL, ChildParticipation::Hidden);
        }

        let offset = Vec2i::new(ScrollArea::axis_offset(&self.horizontal), ScrollArea::axis_offset(&self.vertical));
        // The surface reads the clamped scrollbar offsets while assigning its content. Mark only a
        // changed translation dirty so an unchanged exact allocation may still use the retained
        // layout fast path.
        if (offset.x != self.geometry.offset.x || offset.y != self.geometry.offset.y)
            && let Some(surface) = children.get_mut(Self::SURFACE)
        {
            surface.state.invalidate_layout();
        }
        let child_rect = Recti::new(rect.x + view.x, rect.y + view.y, view.width, view.height);
        let _ = ctx.layout_child(children, Self::SURFACE, child_rect);

        let maximum = Vec2i::new(
            self.horizontal.try_read(Scrollbar::max_offset).unwrap_or(0),
            self.vertical.try_read(Scrollbar::max_offset).unwrap_or(0),
        );
        let corner = (vertical_visible && horizontal_visible)
            .then(|| Recti::new(body.x + body.width, body.y + body.height, vertical_track.width, horizontal_track.height));
        self.geometry = ScrollAreaGeometry {
            surface,
            offset,
            max_offset: maximum,
            viewport: Dimensioni::new(view.width, view.height),
            vertical: vertical_visible.then_some(vertical_track),
            horizontal: horizontal_visible.then_some(horizontal_track),
            corner,
        };
        // The parent clips the three structural children to its surface. Scrolling translation is
        // owned exclusively by the nested scroll surface.
        ctx.set_children_viewport(rect, Vec2i::default());
        ctx.set_content_size(Dimensioni::new(surface.width, surface.height));
        ctx.set_child_overflow_propagation(false);
    }

    fn accepts_event(&self, event: &UiInputEvent) -> bool {
        let UiInputEvent::Scroll { delta, .. } = event else { return false };
        self.accepts_scroll_delta(*delta)
    }
}

impl ScrollArea {
    /// Creates the composite and returns its weak typed widget handle plus completed node.
    ///
    /// The completed parent always owns exactly three structural children: a scrolling content
    /// container and two real scrollbar widgets. The concrete `ScrollArea` is the parent
    /// `ContainerWidget`; the generic `Container` remains the structural owner.
    pub fn create(parameters: ScrollAreaParameters) -> (TypedWidgetHandle<ScrollArea>, Node) {
        // Separate immutable presentation flags from dynamic enablement stored in the widget.
        let enabled = parameters.opt.intersects(ScrollAreaOption::ENABLE_SCROLL);
        let surface_opt = if parameters.opt.intersects(ScrollAreaOption::FRAME) {
            WidgetOption::FRAME
        } else {
            WidgetOption::NONE
        };
        // Build independently addressable scrollbar children and retain only weak typed handles.
        let (horizontal, horizontal_node) = Scrollbar::create(ScrollbarParameters::new(ScrollbarAxis::Horizontal));
        let (vertical, vertical_node) = Scrollbar::create(ScrollbarParameters::new(ScrollbarAxis::Vertical));
        // The surface owns exactly one ordinary content node and keeps its sizing rule local.
        let content = Rc::new(RefCell::new([parameters.content].into_iter().collect()));
        let surface_widget = ScrollSurface {
            horizontal: horizontal.clone(),
            vertical: vertical.clone(),
        };
        let (_, surface_container) = Container::from_shared(content, surface_widget);

        let widget = ScrollArea {
            horizontal: horizontal.clone(),
            vertical: vertical.clone(),
            scrolling_enabled: enabled,
            geometry: ScrollAreaGeometry::default(),
            opt: surface_opt,
        };
        let children = Rc::new(RefCell::new(
            [Node::container(surface_container), horizontal_node, vertical_node].into_iter().collect(),
        ));
        let (handle, container) = Container::from_shared(children, widget);
        (handle, Node::container(container))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::cell::Cell;
    use crate::input::Input;
    use crate::test_support::{AllocationMeasurement, test_atlas};
    use crate::ui_node::UiRuntime;
    use crate::{Custom, CustomParameters, Linear, LinearItem, LinearParameters, MouseButton, Skin, TextBlock, TextBlockParameters, TextWrap, UNCLIPPED_RECT};

    /// Test leaf whose desired size is content behavior rather than a generic node policy.
    struct FixedContent(Dimensioni);

    impl Widget for FixedContent {
        fn widget_opt(&self) -> &WidgetOption {
            &WidgetOption::NONE
        }

        fn update(&mut self, _ctx: &mut WidgetUpdateCtx<'_>, _input: Option<&UiInputEvent>) {}

        fn paint(&mut self, _ctx: &mut WidgetPaintCtx<'_>) {}
    }

    impl crate::LeafWidget for FixedContent {
        fn measure(&self, _style: &crate::Skin, _atlas: &crate::AtlasHandle, _constraints: crate::Constraints) -> Dimensioni {
            self.0
        }
    }

    fn fixed_content(size: Dimensioni) -> Node {
        Node::widget(FixedContent(size))
    }

    /// Container probe that distinguishes measurement candidates from real placement.
    struct CountingContent {
        desired: Dimensioni,
        placements: Rc<Cell<usize>>,
    }

    impl ContainerWidget for CountingContent {
        fn measure(&self, _ctx: &mut MeasureCtx<'_>, _constraints: crate::Constraints) -> Dimensioni {
            self.desired
        }

        fn place(&mut self, ctx: &mut ContainerLayoutCtx<'_>, _children: &mut Children, rect: Recti) {
            self.placements.set(self.placements.get() + 1);
            ctx.set_content_size(Dimensioni::new(rect.width, rect.height));
        }
    }

    impl Widget for CountingContent {
        fn widget_opt(&self) -> &WidgetOption {
            &WidgetOption::NONE
        }

        fn update(&mut self, _ctx: &mut WidgetUpdateCtx<'_>, _input: Option<&UiInputEvent>) {}

        fn paint(&mut self, _ctx: &mut WidgetPaintCtx<'_>) {}
    }

    /// Lays out one fixed content node and returns the parent layout's committed summary.
    ///
    /// `style` must contain resource IDs resolved from `atlas`; accepting the handle explicitly
    /// keeps this helper from creating a second, incompatible ownership domain behind the test's
    /// back.
    fn laid_out_geometry(child_size: Dimensioni, surface: Recti, style: Skin, atlas: &crate::AtlasHandle, requested_offset: Vec2i) -> ScrollAreaGeometry {
        let child = fixed_content(child_size);
        let (scroll, mut root) = ScrollArea::create(ScrollAreaParameters::new(ScrollAreaOption::ENABLE_SCROLL, child));
        scroll.try_update(|state| state.set_offset(requested_offset)).unwrap();

        let mut runtime = UiRuntime::new();
        runtime.begin_update();
        // The runtime takes an owned handle for the pass, so clone the caller's exact atlas
        // capability rather than reconstructing identical-but-foreign atlas contents.
        runtime.layout_tree_root(&mut root, &style, atlas.clone(), surface, UNCLIPPED_RECT);
        scroll.try_read(|state| state.geometry).unwrap()
    }

    #[test]
    fn scroll_area_keeps_one_arbitrary_content_node_behind_its_surface() {
        let child = Custom::create(CustomParameters::new("child"));
        let (child_state, child) = Node::typed_widget(child);
        let (scroll, node) = ScrollArea::create(ScrollAreaParameters::new(ScrollAreaOption::FRAME | ScrollAreaOption::ENABLE_SCROLL, child));

        assert_eq!(
            node.debug_node_count(),
            5,
            "parent, scroll surface, two bars, and arbitrary content are retained nodes"
        );
        scroll.try_update(|state| state.set_offset(Vec2i::new(-4, 12))).unwrap();
        assert_eq!(scroll.try_read(|state| (state.offset().x, state.offset().y)), Some((0, 12)));
        scroll.try_update(|state| state.set_scrolling_enabled(false)).unwrap();
        assert_eq!(scroll.try_read(|state| (state.offset().x, state.offset().y)), Some((0, 0)));

        drop(node);
        assert!(!child_state.is_alive());
        assert!(!scroll.is_alive());
    }

    /// Verifies that intrinsic content smaller than the viewport still owns its blank interaction area.
    #[test]
    fn short_content_fills_the_complete_scroll_viewport() {
        // Retain the child identity so the committed screen allocation can verify both filled axes.
        let child = fixed_content(Dimensioni::new(20, 20));
        let child_id = child.id();
        let (scroll, mut root) = ScrollArea::create(ScrollAreaParameters::new(ScrollAreaOption::ENABLE_SCROLL, child));
        let atlas = test_atlas();
        let style = crate::test_support::test_skin(&atlas).with_metrics(|metrics| {
            metrics.padding = 0;
            metrics.scrollbar_size = 10;
        });
        let viewport = Recti::new(0, 0, 100, 80);
        let mut runtime = UiRuntime::new();

        // A child smaller than the viewport produces no overflow but receives the complete
        // interactive content allocation rather than a top-aligned intrinsic-height strip.
        runtime.begin_update();
        runtime.layout_tree_root(&mut root, &style, atlas.clone(), viewport, UNCLIPPED_RECT);
        let child_rect = runtime.node_rect(std::slice::from_ref(&root), child_id).unwrap();
        assert_eq!((child_rect.width, child_rect.height), (100, 80));
        assert_eq!(
            scroll.try_read(|state| (state.geometry.viewport.width, state.geometry.viewport.height)),
            Some((100, 80))
        );
        assert_eq!(
            scroll.try_read(|state| (state.geometry.max_offset.x, state.geometry.max_offset.y)),
            Some((0, 0))
        );
    }

    /// Verifies nearest-edge reveal behavior on both axes of an overflowing viewport.
    #[test]
    fn scroll_rect_into_view_reveals_nearest_content_edges() {
        // Overflow on both axes reserves two ten-pixel bars and leaves a 90-by-90 content viewport.
        let child = fixed_content(Dimensioni::new(200, 300));
        let (scroll, mut root) = ScrollArea::create(ScrollAreaParameters::new(ScrollAreaOption::ENABLE_SCROLL, child));
        let atlas = test_atlas();
        let style = crate::test_support::test_skin(&atlas).with_metrics(|metrics| {
            metrics.padding = 0;
            metrics.scrollbar_size = 10;
        });
        let viewport = Recti::new(0, 0, 100, 100);
        let mut runtime = UiRuntime::new();
        runtime.begin_update();
        runtime.layout_tree_root(&mut root, &style, atlas.clone(), viewport, UNCLIPPED_RECT);

        // Align the trailing edges of a lower-right target, then commit the requested offsets
        // through ordinary placement and range clamping.
        scroll.try_update(|state| state.scroll_rect_into_view(Recti::new(150, 250, 1, 10))).unwrap();
        runtime.layout_tree_root(&mut root, &style, atlas.clone(), viewport, UNCLIPPED_RECT);
        assert_eq!(scroll.try_read(|state| (state.offset().x, state.offset().y)), Some((61, 170)));

        // A target before the visible origin aligns its leading edges instead of overscrolling to
        // zero or retaining the previous lower-right position.
        scroll.try_update(|state| state.scroll_rect_into_view(Recti::new(10, 20, 5, 5))).unwrap();
        runtime.layout_tree_root(&mut root, &style, atlas.clone(), viewport, UNCLIPPED_RECT);
        assert_eq!(scroll.try_read(|state| (state.offset().x, state.offset().y)), Some((10, 20)));
    }

    #[test]
    fn ordinary_column_content_reuses_retained_measurements_while_scrolling() {
        let mut first_id = None;
        let rows = (0..1_000).map(|index| {
            let node = Node::widget(Custom::create(CustomParameters::new(format!("row-{index}"))));
            first_id.get_or_insert(node.id());
            LinearItem::fixed(node, 20)
        });
        let (_, content) = Linear::create(LinearParameters::vertical(rows));
        let (scroll, mut root) = ScrollArea::create(ScrollAreaParameters::new(ScrollAreaOption::ENABLE_SCROLL, content));
        assert_eq!(root.debug_node_count(), 1_005, "all retained row nodes remain owned by their column");

        let atlas = test_atlas();
        let style = crate::test_support::test_skin(&atlas).with_metrics(|metrics| {
            metrics.padding = 0;
            metrics.spacing = 2;
            metrics.scrollbar_size = 10;
        });
        let viewport = Recti::new(0, 0, 200, 120);
        let mut runtime = UiRuntime::new();
        runtime.begin_update();
        runtime.layout_tree_root(&mut root, &style, atlas.clone(), viewport, UNCLIPPED_RECT);
        let first_id = first_id.unwrap();
        let before = runtime.node_rect(std::slice::from_ref(&root), first_id).unwrap();

        runtime.begin_update();
        let allocation = AllocationMeasurement::begin();
        runtime.layout_tree_root(&mut root, &style, atlas.clone(), viewport, UNCLIPPED_RECT);
        let allocations = allocation.finish();
        let metrics = runtime.debug_metrics();
        assert_eq!(allocations.events, 0);
        assert_eq!(metrics.measures, 1);
        assert_eq!(metrics.layouts, 0);

        scroll.try_update(|state| state.set_offset(Vec2i::new(0, 10_000))).unwrap();
        runtime.begin_update();
        runtime.layout_tree_root(&mut root, &style, atlas, viewport, UNCLIPPED_RECT);
        let after = runtime.node_rect(std::slice::from_ref(&root), first_id).unwrap();
        assert!(after.y < before.y);
        assert_eq!(root.debug_node_count(), 1_005);
    }

    #[test]
    fn disabling_scrolling_revokes_scrollbar_capture_through_participation() {
        let child = fixed_content(Dimensioni::new(200, 200));
        let (scroll, mut root) = ScrollArea::create(ScrollAreaParameters::new(ScrollAreaOption::ENABLE_SCROLL, child));
        let atlas = test_atlas();
        let style = crate::test_support::test_skin(&atlas).with_metrics(|metrics| {
            metrics.padding = 0;
            metrics.scrollbar_size = 10;
        });
        let outer = Recti::new(0, 0, 100, 100);
        let mut runtime = UiRuntime::new();

        runtime.begin_update();
        runtime.layout_tree_root(&mut root, &style, atlas.clone(), outer, UNCLIPPED_RECT);
        let track = scroll
            .try_read(|state| state.geometry.vertical)
            .flatten()
            .expect("overflowing content must activate the vertical scrollbar");

        let mut input = Input::default();
        input.mousedown(track.x + track.width / 2, track.y + 1, MouseButton::LEFT);
        let event = input.pop_event().expect("test input must contain the pointer press");
        let snapshot = input.snapshot();
        runtime.begin_input_event(true, &event);
        let (owner, result) = runtime
            .route_input_event_to_node_ref(&mut root, &style, &event)
            .expect("the scrollbar must receive its pointer press");
        runtime.update_pointer_capture(owner, result, &event, snapshot.mouse_buttons);
        runtime.update_tree_root(&mut root, &style, atlas.clone(), snapshot);
        assert_eq!(runtime.debug_capture_target(), Some(owner));

        scroll.try_update(|state| state.set_scrolling_enabled(false)).unwrap();
        runtime.layout_tree_root(&mut root, &style, atlas.clone(), outer, UNCLIPPED_RECT);
        assert_eq!(runtime.debug_capture_target(), None);
    }

    /// Verifies Skin padding and perpendicular scrollbar overflow converge together.
    #[test]
    fn padding_and_mutually_induced_bars_converge_from_logical_content_extent() {
        // Ten pixels of padding on both sides leaves an eighty-pixel viewport for the child.
        let surface = Recti::new(0, 0, 100, 100);
        let atlas = test_atlas();
        let style = crate::test_support::test_skin(&atlas);
        let fits = laid_out_geometry(
            Dimensioni::new(80, 80),
            surface,
            style.clone().with_metrics(|metrics| {
                metrics.padding = 10;
                metrics.scrollbar_size = 10;
            }),
            &atlas,
            Vec2i::default(),
        );
        assert_eq!((fits.surface.width, fits.surface.height), (100, 100));
        assert_eq!((fits.viewport.width, fits.viewport.height), (80, 80));
        assert!(fits.vertical.is_none() && fits.horizontal.is_none());

        // Vertical overflow removes horizontal space and therefore induces the perpendicular bar;
        // the four-state convergence still commits the union of both required affordances.
        let induced = laid_out_geometry(
            Dimensioni::new(95, 101),
            surface,
            style.with_metrics(|metrics| {
                metrics.padding = 0;
                metrics.scrollbar_size = 10;
            }),
            &atlas,
            Vec2i::default(),
        );
        assert_eq!(induced.vertical.map(|track| track.x), Some(90));
        assert_eq!(induced.horizontal.map(|track| track.y), Some(90));
        assert!(induced.vertical.is_some() && induced.horizontal.is_some() && induced.corner.is_some());
    }

    #[test]
    fn scrollbar_candidates_measure_then_place_content_once() {
        let placements = Rc::new(Cell::new(0));
        let (_, content) = Container::new(
            CountingContent {
                desired: Dimensioni::new(95, 101),
                placements: placements.clone(),
            },
            std::iter::empty::<Node>(),
        );
        let (_, mut root) = ScrollArea::create(ScrollAreaParameters::new(ScrollAreaOption::ENABLE_SCROLL, Node::container(content)));
        let atlas = test_atlas();
        let style = crate::test_support::test_skin(&atlas).with_metrics(|metrics| {
            metrics.padding = 0;
            metrics.scrollbar_size = 10;
        });

        let mut runtime = UiRuntime::new();
        runtime.begin_update();
        runtime.layout_tree_root(&mut root, &style, atlas.clone(), Recti::new(0, 0, 100, 100), UNCLIPPED_RECT);

        assert_eq!(placements.get(), 1, "candidate scrollbar states must never speculatively place content");
    }

    #[test]
    fn wrapped_flex_content_does_not_manufacture_horizontal_overflow() {
        let (_, label) = TextBlock::create(TextBlockParameters::new("label"));
        let (_, text) = TextBlock::create(TextBlockParameters::with_wrap(
            "Lorem ipsum dolor sit amet, consectetur adipiscing elit. Maecenas lacinia, sem eu lacinia molestie, mi risus faucibus ipsum.",
            TextWrap::Word,
        ));
        let text_id = text.id();
        let (_, text_column) = Linear::create(LinearParameters::vertical([text]));
        let (_, row) = Linear::create(LinearParameters::horizontal([
            crate::LinearItem::fixed(label, 40),
            crate::LinearItem::flex(text_column, 1.0),
        ]));
        let (scroll, mut root) = ScrollArea::create(ScrollAreaParameters::new(ScrollAreaOption::ENABLE_SCROLL, row));
        let atlas = test_atlas();
        let style = crate::test_support::test_skin(&atlas).with_metrics(|metrics| {
            metrics.padding = 0;
            metrics.spacing = 4;
            metrics.scrollbar_size = 10;
        });

        let mut runtime = UiRuntime::new();
        runtime.begin_update();
        runtime.layout_tree_root(&mut root, &style, atlas.clone(), Recti::new(0, 0, 100, 300), UNCLIPPED_RECT);

        assert_eq!(scroll.try_read(|state| state.geometry.horizontal.is_none()), Some(true));
        let text_rect = runtime.node_rect(std::slice::from_ref(&root), text_id).unwrap();
        assert_eq!(text_rect.width, 56);
        assert!(text_rect.height > atlas.get_font_height(style.resolve_font_role(&atlas, crate::FontRole::Body)) as i32);
    }

    #[test]
    fn placement_clamps_requested_offsets_after_content_or_viewport_changes() {
        let atlas = test_atlas();
        let style = crate::test_support::test_skin(&atlas);
        let geometry = laid_out_geometry(
            Dimensioni::new(120, 130),
            Recti::new(0, 0, 100, 100),
            style.with_metrics(|metrics| {
                metrics.padding = 0;
                metrics.scrollbar_size = 10;
            }),
            &atlas,
            Vec2i::new(500, 500),
        );
        assert_eq!((geometry.offset.x, geometry.offset.y), (30, 40));
        assert_eq!((geometry.max_offset.x, geometry.max_offset.y), (30, 40));
    }

    #[test]
    fn scroll_to_end_uses_the_next_vertical_range_and_preserves_horizontal_offset() {
        let wide_line = "0123456789".repeat(12);
        let (text, child) = TextBlock::create(TextBlockParameters::new(&wide_line));
        let (scroll, mut root) = ScrollArea::create(ScrollAreaParameters::new(ScrollAreaOption::ENABLE_SCROLL, child));
        let atlas = test_atlas();
        let style = crate::test_support::test_skin(&atlas).with_metrics(|metrics| {
            metrics.padding = 0;
            metrics.scrollbar_size = 10;
        });
        let surface = Recti::new(0, 0, 100, 80);
        let mut runtime = UiRuntime::new();

        runtime.begin_update();
        runtime.layout_tree_root(&mut root, &style, atlas.clone(), surface, UNCLIPPED_RECT);
        text.set_text(std::iter::repeat_n(wide_line, 20).collect::<Vec<_>>().join("\n")).unwrap();
        scroll
            .try_update(|state| {
                state.set_offset(Vec2i::new(7, 0));
                state.scroll_to_end();
            })
            .unwrap();

        runtime.begin_update();
        runtime.layout_tree_root(&mut root, &style, atlas.clone(), surface, UNCLIPPED_RECT);
        let geometry = scroll.try_read(|state| state.geometry).unwrap();
        assert_eq!(geometry.offset.x, 7);
        assert!(geometry.max_offset.y > 0);
        assert_eq!(geometry.offset.y, geometry.max_offset.y);
    }

    /// Verifies that the optional frame and scroll transform each contribute exactly one offset.
    #[test]
    fn frame_origin_and_scroll_translation_are_applied_exactly_once() {
        let child = fixed_content(Dimensioni::new(160, 200));
        let child_id = child.id();
        let (scroll, mut root) = ScrollArea::create(ScrollAreaParameters::new(ScrollAreaOption::FRAME | ScrollAreaOption::ENABLE_SCROLL, child));
        let mut runtime = UiRuntime::new();
        let atlas = test_atlas();
        let mut style = crate::test_support::test_skin(&atlas).with_metrics(|metrics| {
            metrics.padding = 5;
            metrics.scrollbar_size = 10;
        });
        let frame_insets = crate::SliceInsets::uniform(3);
        crate::test_support::replace_skin_patches(
            &mut style,
            crate::AppearanceRole::Surface(SurfaceRole::Panel),
            crate::StateTable::filled(crate::NinePatch::framed(
                frame_insets,
                crate::color(1, 2, 3, 255),
                Some(crate::color(4, 5, 6, 255)),
            )),
        );
        let outer = Recti::new(10, 20, 100, 80);

        runtime.begin_update();
        runtime.layout_tree_root(&mut root, &style, atlas.clone(), outer, UNCLIPPED_RECT);
        let allocation_before = root.with_node(child_id, |node| node.state.layout.allocation).unwrap();
        let screen_before = runtime.node_rect(std::slice::from_ref(&root), child_id).unwrap();
        assert_eq!((allocation_before.x, allocation_before.y), (0, 0));
        assert_eq!(
            (screen_before.x, screen_before.y),
            (
                outer.x + frame_insets.left + style.metrics.padding,
                outer.y + frame_insets.top + style.metrics.padding
            )
        );

        scroll.try_update(|state| state.set_offset(Vec2i::new(0, 12))).unwrap();
        runtime.layout_tree_root(&mut root, &style, atlas.clone(), outer, UNCLIPPED_RECT);
        let allocation_after = root.with_node(child_id, |node| node.state.layout.allocation).unwrap();
        let screen_after = runtime.node_rect(std::slice::from_ref(&root), child_id).unwrap();
        assert_eq!(
            (allocation_after.x, allocation_after.y, allocation_after.width, allocation_after.height),
            (allocation_before.x, allocation_before.y, allocation_before.width, allocation_before.height),
            "scrolling changes transform, not content placement"
        );
        assert_eq!((screen_after.x, screen_after.y), (screen_before.x, screen_before.y - 12));

        runtime.layout_tree_root(&mut root, &style, atlas.clone(), Recti::new(10, 20, 240, 260), UNCLIPPED_RECT);
        assert_eq!(scroll.try_read(|state| (state.offset().x, state.offset().y)), Some((0, 0)));
    }
}
