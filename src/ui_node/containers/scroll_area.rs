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

use std::{cell::RefCell, rc::Rc};

use bitflags::bitflags;

use crate::ui_node::scrollbar::{RetainedScrollbar, ScrollAxis, scrollbar_base};
use crate::{
    ChildParticipation, Container, ContainerWidget, ControlColor, Dimensioni, FocusPolicy, MeasureCtx, Recti, TypedWidgetHandle, UiInputEvent, Vec2i, Widget,
    WidgetOption, WidgetPaintCtx, WidgetParameters, WidgetUpdateCtx,
};

use super::{Children, ContainerLayoutCtx, Node};

bitflags! {
    #[derive(Copy, Clone)]
    /// Options fixed when a retained scroll area is constructed.
    pub struct ScrollAreaOption : u32 {
        /// Gives the scroll area a Style-owned outer border and inset content area.
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
    /// The content fills at least the viewport width, may remain wider when its desired width
    /// overflows, and keeps its desired height. Use explicit tracks inside the content container to
    /// express fixed or flexible descendants; ScrollArea adds no policy to the content node.
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

/// Concrete application-facing widget for the three-child ScrollArea composite.
pub struct ScrollArea {
    /// Weak typed widget capability for the horizontal scrollbar child.
    horizontal: TypedWidgetHandle<RetainedScrollbar>,
    /// Weak typed widget capability for the vertical scrollbar child.
    vertical: TypedWidgetHandle<RetainedScrollbar>,
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
        Vec2i::new(Self::axis_offset(&self.horizontal), Self::axis_offset(&self.vertical))
    }

    /// Requests a non-negative offset; the next placement clamps it to current content geometry.
    pub fn set_offset(&mut self, offset: Vec2i) {
        let offset = if self.scrolling_enabled {
            Vec2i::new(offset.x.max(0), offset.y.max(0))
        } else {
            Vec2i::default()
        };
        Self::set_axis_offset(&self.horizontal, offset.x);
        Self::set_axis_offset(&self.vertical, offset.y);
    }

    /// Returns whether layout may activate the scrollbar children.
    pub fn scrolling_enabled(&self) -> bool {
        self.scrolling_enabled
    }

    /// Enables or disables scrolling and its two interactive children.
    pub fn set_scrolling_enabled(&mut self, enabled: bool) {
        self.scrolling_enabled = enabled;
        if !enabled {
            // Reset child-owned interaction synchronously so capture cannot survive deactivation.
            Self::reset_axis(&self.horizontal);
            Self::reset_axis(&self.vertical);
            self.geometry.offset = Vec2i::default();
            self.geometry.max_offset = Vec2i::default();
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
    fn axis_offset(handle: &TypedWidgetHandle<RetainedScrollbar>) -> i32 {
        handle
            .try_read(RetainedScrollbar::offset)
            .expect("ScrollArea structural scrollbar must outlive its parent state")
    }

    /// Writes a requested offset through the scrollbar's weak typed widget capability.
    fn set_axis_offset(handle: &TypedWidgetHandle<RetainedScrollbar>, offset: i32) {
        handle
            .try_update_without_measurement(|state| state.set_offset(offset))
            .expect("ScrollArea structural scrollbar must be available outside traversal")
    }

    /// Returns both committed ranges without copying geometry into interactive state.
    fn max_offset(&self) -> Vec2i {
        Vec2i::new(
            self.horizontal.try_read(RetainedScrollbar::max_offset).unwrap_or(0),
            self.vertical.try_read(RetainedScrollbar::max_offset).unwrap_or(0),
        )
    }

    /// Clears one scrollbar's offset and committed geometry.
    fn reset_axis(handle: &TypedWidgetHandle<RetainedScrollbar>) {
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
        ctx.draw_rect(surface, ctx.style().colors[ControlColor::PanelBG as usize]);
        if let Some(corner) = corner {
            ctx.draw_rect(corner, ctx.style().colors[ControlColor::PanelBG as usize]);
        }
    }

    fn focus_policy(&self) -> FocusPolicy {
        // The surface accepts only wheel events, so it never creates persistent pointer focus.
        FocusPolicy::Momentary
    }
}

/// The transform boundary between the viewport and its ordinary content node.
struct ScrollSurface {
    /// Scroll translation is read from the real horizontal child widget.
    horizontal: TypedWidgetHandle<RetainedScrollbar>,
    /// Scroll translation is read from the real vertical child widget.
    vertical: TypedWidgetHandle<RetainedScrollbar>,
}

impl ContainerWidget for ScrollSurface {
    fn measure(&self, ctx: &mut MeasureCtx<'_>, constraints: crate::Constraints) -> Dimensioni {
        ctx.measure_child(0, crate::Constraints::new(constraints.width, crate::AvailableSpace::Unbounded))
            .unwrap_or_default()
    }

    fn place(&mut self, ctx: &mut ContainerLayoutCtx<'_>, children: &mut Children, rect: Recti) {
        let preferred = ctx
            .measure_child(
                children,
                0,
                crate::Constraints::new(crate::AvailableSpace::bounded(rect.width), crate::AvailableSpace::Unbounded),
            )
            .unwrap_or_default();
        // Horizontal content fills the viewport so ordinary rows receive its usable width, but an
        // intrinsically wider child remains wider and creates horizontal overflow. Vertical content
        // keeps its desired height; stretching it to the viewport would hide whether scrolling is
        // needed.
        let content_rect = Recti::new(0, 0, rect.width.max(preferred.width).max(0), preferred.height.max(0));
        let _ = ctx.layout_child(children, 0, content_rect);
        let offset = Vec2i::new(ScrollArea::axis_offset(&self.horizontal), ScrollArea::axis_offset(&self.vertical));
        ctx.set_children_viewport(rect, Vec2i::new(-offset.x, -offset.y));
        ctx.set_content_size(Dimensioni::new(content_rect.width, content_rect.height));
        ctx.set_child_overflow_propagation(false);
    }
}

impl Widget for ScrollSurface {
    fn widget_opt(&self) -> &WidgetOption {
        &WidgetOption::NO_INTERACT
    }

    fn update(&mut self, _ctx: &mut WidgetUpdateCtx<'_>, _input: Option<&UiInputEvent>) {}

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
    fn configure_bar(handle: &TypedWidgetHandle<RetainedScrollbar>, track: Recti, view_len: i32, content_len: i32, min_thumb_len: i32, requested_offset: i32) {
        // Apply the requested offset before configuring the new range; configure performs the final
        // clamp and resets drag geometry using the current widget-local track.
        handle
            .try_update_without_measurement(|state| {
                state.set_offset(requested_offset);
                state.configure(Recti::new(0, 0, track.width, track.height), view_len, content_len, min_thumb_len);
            })
            .expect("ScrollArea layout requires its retained scrollbar child");
    }

    /// Deactivates one hidden scrollbar without removing its strong child owner.
    fn deactivate_bar(handle: &TypedWidgetHandle<RetainedScrollbar>) {
        // Retain the child node and handle identity while clearing geometry, offset, and drag state.
        handle
            .try_update_without_measurement(RetainedScrollbar::deactivate)
            .expect("ScrollArea layout requires its retained scrollbar child");
    }
}

impl ContainerWidget for ScrollArea {
    fn measure(&self, ctx: &mut MeasureCtx<'_>, constraints: crate::Constraints) -> Dimensioni {
        // Measure application content through the scroll surface and add only panel padding.
        // Scrollbars are responsive affordances and do not inflate intrinsic composite size.
        let padding = ctx.style().padding.max(0);
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

    fn place(&mut self, ctx: &mut ContainerLayoutCtx<'_>, children: &mut Children, rect: Recti) {
        // All committed summary geometry is parent-local; child scrollbar geometry passed to each
        // widget is normalized to that child's own local track by configure_bar.
        let surface = Recti::new(0, 0, rect.width.max(0), rect.height.max(0));
        let padding = ctx.style().padding.max(0);
        let bar_size = ctx.style().scrollbar_size.max(0);
        let min_thumb = ctx.style().thumb_size.max(0);
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
            let view = inset_rect(body, padding);
            let preferred = ctx
                .measure_child(
                    children,
                    Self::SURFACE,
                    crate::Constraints::new(crate::AvailableSpace::bounded(view.width), crate::AvailableSpace::Unbounded),
                )
                .unwrap_or_default();
            // ScrollSurface fills available width but keeps desired height, matching the one exact
            // content rectangle it will assign after bar selection.
            let extent = Dimensioni::new(view.width.max(preferred.width).max(0), preferred.height.max(0));

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
        let vertical_track = scrollbar_base(ScrollAxis::Vertical, body, vertical_width);
        let horizontal_track = scrollbar_base(ScrollAxis::Horizontal, body, horizontal_height);
        let vertical_visible = has_vertical && vertical_track.width > 0 && vertical_track.height > 0;
        let horizontal_visible = has_horizontal && horizontal_track.width > 0 && horizontal_track.height > 0;

        if vertical_visible {
            // The scrollbar remains a real independently targetable child when active.
            Self::configure_bar(&self.vertical, vertical_track, view.height, extent.height, min_thumb, requested.y);
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
            Self::configure_bar(&self.horizontal, horizontal_track, view.width, extent.width, min_thumb, requested.x);
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
            self.horizontal.try_read(RetainedScrollbar::max_offset).unwrap_or(0),
            self.vertical.try_read(RetainedScrollbar::max_offset).unwrap_or(0),
        );
        let corner = (vertical_visible && horizontal_visible)
            .then(|| Recti::new(body.x + body.width, body.y + body.height, vertical_track.width, horizontal_track.height));
        self.geometry = ScrollAreaGeometry {
            surface,
            offset,
            max_offset: maximum,
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
        let (horizontal, horizontal_node) = RetainedScrollbar::create(ScrollAxis::Horizontal);
        let (vertical, vertical_node) = RetainedScrollbar::create(ScrollAxis::Vertical);
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
    use crate::{Custom, CustomParameters, Linear, LinearItem, LinearParameters, MouseButton, Style, TextBlock, TextBlockParameters, TextWrap, UNCLIPPED_RECT};

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
        fn measure(&self, _style: &crate::Style, _atlas: &crate::AtlasHandle, _constraints: crate::Constraints) -> Dimensioni {
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
    fn laid_out_geometry(child_size: Dimensioni, surface: Recti, style: Style, requested_offset: Vec2i) -> ScrollAreaGeometry {
        let child = fixed_content(child_size);
        let (scroll, mut root) = ScrollArea::create(ScrollAreaParameters::new(ScrollAreaOption::ENABLE_SCROLL, child));
        scroll.try_update(|state| state.set_offset(requested_offset)).unwrap();

        let mut runtime = UiRuntime::new();
        runtime.begin_update();
        runtime.layout_tree_root(&mut root, &style, test_atlas(), surface, UNCLIPPED_RECT);
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

        let style = Style {
            padding: 0,
            spacing: 2,
            scrollbar_size: 10,
            ..Style::default()
        };
        let viewport = Recti::new(0, 0, 200, 120);
        let mut runtime = UiRuntime::new();
        let atlas = test_atlas();
        runtime.begin_update();
        runtime.layout_tree_root(&mut root, &style, atlas.clone(), viewport, UNCLIPPED_RECT);
        let first_id = first_id.unwrap();
        let before = runtime.debug_node_rect(std::slice::from_ref(&root), first_id).unwrap();

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
        let after = runtime.debug_node_rect(std::slice::from_ref(&root), first_id).unwrap();
        assert!(after.y < before.y);
        assert_eq!(root.debug_node_count(), 1_005);
    }

    #[test]
    fn disabling_scrolling_revokes_scrollbar_capture_through_participation() {
        let child = fixed_content(Dimensioni::new(200, 200));
        let (scroll, mut root) = ScrollArea::create(ScrollAreaParameters::new(ScrollAreaOption::ENABLE_SCROLL, child));
        let style = Style {
            padding: 0,
            scrollbar_size: 10,
            ..Style::default()
        };
        let outer = Recti::new(0, 0, 100, 100);
        let mut runtime = UiRuntime::new();

        runtime.begin_update();
        runtime.layout_tree_root(&mut root, &style, test_atlas(), outer, UNCLIPPED_RECT);
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
            .route_input_event_to_node_ref(&mut root, runtime.root_transform(), &style, &event)
            .expect("the scrollbar must receive its pointer press");
        runtime.update_pointer_capture(owner, result, &event, snapshot.mouse_buttons);
        runtime.update_tree_root(&mut root, &style, test_atlas(), snapshot);
        assert_eq!(runtime.capture, Some(owner));

        scroll.try_update(|state| state.set_scrolling_enabled(false)).unwrap();
        runtime.layout_tree_root(&mut root, &style, test_atlas(), outer, UNCLIPPED_RECT);
        assert_eq!(runtime.capture, None);
    }

    #[test]
    fn padding_and_mutually_induced_bars_converge_from_logical_content_extent() {
        let surface = Recti::new(0, 0, 100, 100);
        let fits = laid_out_geometry(
            Dimensioni::new(80, 80),
            surface,
            Style {
                padding: 10,
                scrollbar_size: 10,
                ..Style::default()
            },
            Vec2i::default(),
        );
        assert_eq!((fits.surface.width, fits.surface.height), (100, 100));
        assert!(fits.vertical.is_none() && fits.horizontal.is_none());

        let induced = laid_out_geometry(
            Dimensioni::new(95, 101),
            surface,
            Style {
                padding: 0,
                scrollbar_size: 10,
                ..Style::default()
            },
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
        let style = Style {
            padding: 0,
            scrollbar_size: 10,
            ..Style::default()
        };

        let mut runtime = UiRuntime::new();
        runtime.begin_update();
        runtime.layout_tree_root(&mut root, &style, test_atlas(), Recti::new(0, 0, 100, 100), UNCLIPPED_RECT);

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
        let style = Style {
            padding: 0,
            spacing: 4,
            scrollbar_size: 10,
            ..Style::default()
        };

        let mut runtime = UiRuntime::new();
        runtime.begin_update();
        runtime.layout_tree_root(&mut root, &style, test_atlas(), Recti::new(0, 0, 100, 300), UNCLIPPED_RECT);

        assert_eq!(scroll.try_read(|state| state.geometry.horizontal.is_none()), Some(true));
        let text_rect = runtime.debug_node_rect(std::slice::from_ref(&root), text_id).unwrap();
        assert_eq!(text_rect.width, 56);
        assert!(text_rect.height > test_atlas().get_font_height(style.font) as i32);
    }

    #[test]
    fn placement_clamps_requested_offsets_after_content_or_viewport_changes() {
        let geometry = laid_out_geometry(
            Dimensioni::new(120, 130),
            Recti::new(0, 0, 100, 100),
            Style {
                padding: 0,
                scrollbar_size: 10,
                ..Style::default()
            },
            Vec2i::new(500, 500),
        );
        assert_eq!((geometry.offset.x, geometry.offset.y), (30, 40));
        assert_eq!((geometry.max_offset.x, geometry.max_offset.y), (30, 40));
    }

    #[test]
    fn frame_origin_and_scroll_translation_are_applied_exactly_once() {
        let child = fixed_content(Dimensioni::new(160, 200));
        let child_id = child.id();
        let (scroll, mut root) = ScrollArea::create(ScrollAreaParameters::new(ScrollAreaOption::FRAME | ScrollAreaOption::ENABLE_SCROLL, child));
        let mut runtime = UiRuntime::new();
        let style = Style {
            frame_border_width: 3,
            padding: 5,
            scrollbar_size: 10,
            ..Style::default()
        };
        let outer = Recti::new(10, 20, 100, 80);

        runtime.begin_update();
        runtime.layout_tree_root(&mut root, &style, test_atlas(), outer, UNCLIPPED_RECT);
        let allocation_before = root.with_node(child_id, |node| node.state.layout.allocation).unwrap();
        let screen_before = runtime.debug_node_rect(std::slice::from_ref(&root), child_id).unwrap();
        assert_eq!((allocation_before.x, allocation_before.y), (0, 0));
        assert_eq!(
            (screen_before.x, screen_before.y),
            (
                outer.x + style.frame_border_width + style.padding,
                outer.y + style.frame_border_width + style.padding
            )
        );

        scroll.try_update(|state| state.set_offset(Vec2i::new(0, 12))).unwrap();
        runtime.layout_tree_root(&mut root, &style, test_atlas(), outer, UNCLIPPED_RECT);
        let allocation_after = root.with_node(child_id, |node| node.state.layout.allocation).unwrap();
        let screen_after = runtime.debug_node_rect(std::slice::from_ref(&root), child_id).unwrap();
        assert_eq!(
            (allocation_after.x, allocation_after.y, allocation_after.width, allocation_after.height),
            (allocation_before.x, allocation_before.y, allocation_before.width, allocation_before.height),
            "scrolling changes transform, not content placement"
        );
        assert_eq!((screen_after.x, screen_after.y), (screen_before.x, screen_before.y - 12));

        runtime.layout_tree_root(&mut root, &style, test_atlas(), Recti::new(10, 20, 240, 260), UNCLIPPED_RECT);
        assert_eq!(scroll.try_read(|state| (state.offset().x, state.offset().y)), Some((0, 0)));
    }
}
