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

use std::{
    cell::RefCell,
    rc::{Rc, Weak},
};

use bitflags::bitflags;

use crate::ui_node::children::ChildrenHandle;
use crate::ui_node::scrollbar::{RetainedScrollbar, ScrollAxis, scrollbar_base};
use crate::ui_node::{runtime_read_state, runtime_update_state};
use crate::{
    AtlasHandle, ChildParticipation, Container, ContainerSurface, ControlColor, Dimensioni, FocusPolicy, Layout, Recti, Style, TypedWidgetHandle, UiInputEvent,
    Vec2i, Widget, WidgetOption, WidgetPaintCtx, WidgetParameters, WidgetState, WidgetStateHandle, WidgetUpdateCtx,
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
    /// Nodes transferred into the virtual-surface child during construction.
    children: Children,
    /// Immutable presentation options for the completed composite.
    opt: ScrollAreaOption,
}

impl WidgetParameters for ScrollAreaParameters {}

impl ScrollAreaParameters {
    /// Creates a scroll area that owns `children` in iterator order.
    pub fn new(opt: ScrollAreaOption, children: impl IntoIterator<Item = Node>) -> Self {
        Self {
            children: children.into_iter().collect(),
            opt,
        }
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

/// Application-facing state for the three-child ScrollArea composite.
///
/// The concrete parent and virtual-surface containers remain the only strong topology owners.
/// This state holds weak capabilities for content mutation and for the two real scrollbar child
/// widgets; cloning an application handle therefore cannot keep any removed subtree alive.
pub struct ScrollAreaState {
    /// Weak topology capability for the virtual-surface child.
    content: ChildrenHandle,
    /// Weak state capability for the horizontal scrollbar child.
    horizontal: TypedWidgetHandle<RetainedScrollbar>,
    /// Weak state capability for the vertical scrollbar child.
    vertical: TypedWidgetHandle<RetainedScrollbar>,
    /// Dynamic participation policy shared by surface and layout.
    scrolling_enabled: bool,
    /// Latest geometry summary; interactive state remains in the child widgets.
    geometry: ScrollAreaGeometry,
}

impl WidgetState for ScrollAreaState {}

impl ScrollAreaState {
    /// Returns the number of content nodes, or `None` when topology is unavailable.
    pub fn len(&self) -> Option<usize> {
        self.content.len()
    }

    /// Returns whether the virtual surface owns no content nodes.
    pub fn is_empty(&self) -> Option<bool> {
        self.content.is_empty()
    }

    /// Appends one still-unmounted content node without exposing structural children.
    pub fn push(&mut self, node: Node) -> Result<(), Node> {
        self.content.try_push(node)
    }

    /// Inserts a content node or returns its exact owner when insertion is unavailable.
    #[allow(clippy::result_large_err)]
    pub fn insert(&mut self, index: usize, node: Node) -> Result<(), Node> {
        self.content.try_insert(index, node)
    }

    /// Drops one indexed content owner and reports whether it existed.
    pub fn remove_drop(&mut self, index: usize) -> Option<bool> {
        self.content.try_remove_drop(index)
    }

    /// Drops every content node while preserving the composite's three structural children.
    pub fn clear(&mut self) -> Option<()> {
        self.content.try_clear()
    }

    /// Replaces all content nodes in iterator order.
    pub fn replace<I>(&mut self, nodes: I) -> Result<(), I>
    where
        I: IntoIterator<Item = Node>,
    {
        self.content.try_replace(nodes)
    }

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
        self.geometry.offset = offset;
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
    fn scroll_by(&mut self, delta: Vec2i) {
        if !self.accepts_scroll_delta(delta) {
            return;
        }
        let offset = self.offset();
        let maximum = self.max_offset();
        // next_offset = clamp(previous_offset + wheel_delta, zero, maximum_offset).
        self.set_offset(Vec2i::new(
            offset.x.saturating_add(delta.x).clamp(0, maximum.x),
            offset.y.saturating_add(delta.y).clamp(0, maximum.y),
        ));
    }

    /// Reads one live structural scrollbar without taking ownership of its state.
    fn axis_offset(handle: &TypedWidgetHandle<RetainedScrollbar>) -> i32 {
        handle
            .try_read(RetainedScrollbar::offset)
            .expect("ScrollArea structural scrollbar must outlive its parent state")
    }

    /// Writes a requested offset through the scrollbar's weak state capability.
    fn set_axis_offset(handle: &TypedWidgetHandle<RetainedScrollbar>, offset: i32) {
        handle
            .try_update(|state| state.set_offset(offset))
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
            .try_update(|state| {
                state.set_offset(0);
                state.deactivate();
            })
            .expect("ScrollArea structural scrollbar must be available outside traversal");
    }
}

/// Wheel-only widget behavior installed on the parent container surface.
struct ScrollAreaSurface {
    /// Weak state access prevents the surface from owning its enclosing container layout.
    state: Weak<RefCell<ScrollAreaState>>,
    /// Optional generic frame plus the dynamic wheel option.
    opt: WidgetOption,
}

impl Widget for ScrollAreaSurface {
    fn widget_opt(&self) -> &WidgetOption {
        // These are presentation defaults; effective options below add dynamic scroll eligibility.
        &self.opt
    }

    fn effective_widget_opt(&self) -> WidgetOption {
        // Expired composite state makes the surface inert. A live enabled state advertises wheel
        // capture without turning the parent into the owner of scrollbar drag interaction.
        let Some(state) = self.state.upgrade() else {
            return self.opt | WidgetOption::NO_INTERACT;
        };
        runtime_read_state(&state, "ScrollAreaSurface::options", |state| {
            if state.scrolling_enabled {
                self.opt | WidgetOption::GRAB_SCROLL
            } else {
                self.opt | WidgetOption::NO_INTERACT
            }
        })
    }

    fn measure(&self, _style: &Style, _atlas: &AtlasHandle, _available: Dimensioni) -> Dimensioni {
        // Preferred composite size comes from ScrollAreaLayout and its virtual-surface child.
        Dimensioni::default()
    }

    fn update(&mut self, _ctx: &mut WidgetUpdateCtx<'_>, input: Option<&UiInputEvent>) {
        // Routing guarantees that only an accepted wheel event reaches this surface.
        let Some(UiInputEvent::Scroll { delta, .. }) = input else { return };
        let Some(state) = self.state.upgrade() else { return };
        runtime_update_state(&state, "ScrollAreaSurface::update", |state| state.scroll_by(*delta));
    }

    fn paint(&mut self, ctx: &mut WidgetPaintCtx<'_>) {
        // The parent paints only its panel and non-interactive corner; scrollbar children paint
        // their own tracks and thumbs afterward in ordinary child paint order.
        let fallback = ctx.local_rect();
        let Some(state) = self.state.upgrade() else { return };
        let (surface, corner) = runtime_read_state(&state, "ScrollAreaSurface::paint", |state| (state.geometry.surface, state.geometry.corner));
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

impl ContainerSurface for ScrollAreaSurface {
    /// Accepts only wheel movement that changes the committed scroll offset.
    fn accepts_event(&self, event: &UiInputEvent) -> bool {
        // Non-wheel input must continue bubbling because the parent surface exists only to provide
        // background paint and boundary-aware wheel handling for the composite.
        let UiInputEvent::Scroll { delta, .. } = event else { return false };
        let Some(state) = self.state.upgrade() else { return false };
        runtime_read_state(&state, "ScrollAreaSurface::accepts_event", |state| state.accepts_scroll_delta(*delta))
    }
}

/// Geometry policy for content owned by the virtual-surface structural child.
struct VirtualSurfaceLayout {
    /// Scroll translation is read from the real horizontal child widget.
    horizontal: TypedWidgetHandle<RetainedScrollbar>,
    /// Scroll translation is read from the real vertical child widget.
    vertical: TypedWidgetHandle<RetainedScrollbar>,
}

impl Layout for VirtualSurfaceLayout {
    fn measure(&self, children: &Children, style: &Style, atlas: &AtlasHandle, available: Dimensioni) -> Dimensioni {
        // Vertical content remains intrinsically unbounded while width can constrain wrapping.
        super::column::measure_column(children, style, atlas, Dimensioni::new(available.width, 0))
    }

    fn place(&mut self, ctx: &mut ContainerLayoutCtx<'_>, children: &mut Children, rect: Recti) {
        // Child allocations remain in stable logical coordinates. Only the descendant transform
        // changes when scrollbar state changes, so scrolling never rewrites application topology.
        let extent = layout_virtual_content(ctx, children, Dimensioni::new(rect.width, rect.height));
        let offset = Vec2i::new(ScrollAreaState::axis_offset(&self.horizontal), ScrollAreaState::axis_offset(&self.vertical));
        // Translation changes traversal coordinates only; content allocations remain stable.
        ctx.set_children_viewport(rect, Vec2i::new(-offset.x, -offset.y));
        ctx.set_content_size(extent);
        ctx.set_child_overflow_propagation(false);
    }
}

/// Places a vertical sequence inside the virtual surface and returns its logical extent.
fn layout_virtual_content(ctx: &mut ContainerLayoutCtx<'_>, children: &mut Children, view: Dimensioni) -> Dimensioni {
    // This is a vertical intrinsic flow, not another retained container object. It operates on the
    // virtual surface's authoritative Children for exactly one placement call.
    let child_width = view.width.max(0);
    let spacing = ctx.style().spacing.max(0);
    let mut y: i32 = 0;
    let mut width = 0;
    for index in 0..children.len() {
        // Preserve intrinsic horizontal overflow while constraining responsive children to at
        // least the viewport width. A second measurement observes wrapping at the committed width.
        let preferred = children
            .measure_child(index, ctx.style(), ctx.atlas(), Dimensioni::new(child_width.max(1), 0))
            .unwrap_or_default();
        let offered_width = child_width.max(preferred.width);
        let measured_width = children
            .child_policy(index)
            .unwrap_or_else(crate::Policy::auto)
            .width
            .measurement_bound(offered_width);
        let preferred_height = children
            .measure_child(index, ctx.style(), ctx.atlas(), Dimensioni::new(measured_width, 0))
            .unwrap_or_default()
            .height;
        let size = ctx
            .layout_child(children, index, Recti::new(0, y, offered_width, preferred_height))
            .unwrap_or_default();
        width = width.max(offered_width.max(size.width));
        // next_y = current_y + child_height, followed by spacing when another child remains.
        y = y.saturating_add(size.height);
        if index + 1 < children.len() {
            y = y.saturating_add(spacing);
        }
    }
    Dimensioni::new(width.max(0), y.max(0))
}

/// Geometry-only policy for the fixed virtual/horizontal/vertical child roles.
pub struct ScrollAreaLayout {
    /// Sole strong owner of application-facing composite state.
    state: Rc<RefCell<ScrollAreaState>>,
    /// Weak capability used only to configure the horizontal child after placement.
    horizontal: TypedWidgetHandle<RetainedScrollbar>,
    /// Weak capability used only to configure the vertical child after placement.
    vertical: TypedWidgetHandle<RetainedScrollbar>,
}

impl ScrollAreaLayout {
    /// Virtual surface containing every application-provided content node.
    const VIRTUAL_SURFACE: usize = 0;
    /// Independently targetable horizontal scrollbar widget.
    const HORIZONTAL: usize = 1;
    /// Independently targetable vertical scrollbar widget.
    const VERTICAL: usize = 2;

    /// Configures one visible scrollbar using widget-local track coordinates.
    fn configure_bar(handle: &TypedWidgetHandle<RetainedScrollbar>, track: Recti, view_len: i32, content_len: i32, min_thumb_len: i32, requested_offset: i32) {
        // Apply the requested offset before configuring the new range; configure performs the final
        // clamp and resets drag geometry using the current widget-local track.
        handle
            .try_update(|state| {
                state.set_offset(requested_offset);
                state.configure(Recti::new(0, 0, track.width, track.height), view_len, content_len, min_thumb_len);
            })
            .expect("ScrollArea layout requires its retained scrollbar child");
    }

    /// Deactivates one hidden scrollbar without removing its strong child owner.
    fn deactivate_bar(handle: &TypedWidgetHandle<RetainedScrollbar>) {
        // Retain the child node and handle identity while clearing geometry, offset, and drag state.
        handle
            .try_update(RetainedScrollbar::deactivate)
            .expect("ScrollArea layout requires its retained scrollbar child");
    }
}

impl Layout for ScrollAreaLayout {
    fn measure(&self, children: &Children, style: &Style, atlas: &AtlasHandle, available: Dimensioni) -> Dimensioni {
        // Measure application content through the virtual surface and add only the panel padding.
        // Scrollbars are responsive affordances and do not inflate intrinsic composite size.
        let padding = style.padding.max(0);
        // inset = leading_padding + trailing_padding = padding * 2.
        let inset = padding.saturating_mul(2);
        let content_width = if available.width > 0 {
            // content_width = max(available_width - inset, 1).
            available.width.saturating_sub(inset).max(1)
        } else {
            0
        };
        let content = children
            .measure_child(Self::VIRTUAL_SURFACE, style, atlas, Dimensioni::new(content_width, 0))
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
        let enabled = runtime_read_state(&self.state, "ScrollAreaLayout::enabled", |state| state.scrolling_enabled);
        let requested = Vec2i::new(ScrollAreaState::axis_offset(&self.horizontal), ScrollAreaState::axis_offset(&self.vertical));
        let bars_usable = enabled && bar_size > 0 && surface.width > 0 && surface.height > 0;
        let mut has_horizontal = false;
        let mut has_vertical = false;

        // Scrollbar presence has four monotonic states. Each speculative placement updates the
        // virtual child's logical content size, which can induce the perpendicular scrollbar.
        for _ in 0..4 {
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
            // Speculatively lay out content at this candidate viewport to discover logical extent.
            let child_rect = Recti::new(rect.x + view.x, rect.y + view.y, view.width, view.height);
            let _ = ctx.layout_child(children, Self::VIRTUAL_SURFACE, child_rect);
            let extent = ctx.child_content_size(children, Self::VIRTUAL_SURFACE).unwrap_or_default();

            let next_vertical = has_vertical || (bars_usable && extent.height > view.height);
            let next_horizontal = has_horizontal || (bars_usable && extent.width > view.width);
            if next_vertical != has_vertical || next_horizontal != has_horizontal {
                // Presence only moves from false to true, guaranteeing convergence in this loop.
                has_vertical = next_vertical;
                has_horizontal = next_horizontal;
                continue;
            }

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

            // Re-place after bar configuration so the virtual transform observes clamped offsets.
            let _ = ctx.layout_child(children, Self::VIRTUAL_SURFACE, child_rect);
            let offset = Vec2i::new(ScrollAreaState::axis_offset(&self.horizontal), ScrollAreaState::axis_offset(&self.vertical));
            let maximum = Vec2i::new(
                self.horizontal.try_read(RetainedScrollbar::max_offset).unwrap_or(0),
                self.vertical.try_read(RetainedScrollbar::max_offset).unwrap_or(0),
            );
            let corner = (vertical_visible && horizontal_visible)
                .then(|| Recti::new(body.x + body.width, body.y + body.height, vertical_track.width, horizontal_track.height));
            runtime_update_state(&self.state, "ScrollAreaLayout::commit", |state| {
                // Publish one coherent snapshot consumed by surface paint and application queries.
                state.geometry = ScrollAreaGeometry {
                    surface,
                    offset,
                    max_offset: maximum,
                    vertical: vertical_visible.then_some(vertical_track),
                    horizontal: horizontal_visible.then_some(horizontal_track),
                    corner,
                };
            });
            // The parent clips the three structural children to its surface. Scrolling translation
            // is owned exclusively by the nested virtual surface.
            ctx.set_children_viewport(rect, Vec2i::default());
            ctx.set_content_size(Dimensioni::new(surface.width, surface.height));
            ctx.set_child_overflow_propagation(false);
            return;
        }

        unreachable!("ScrollArea scrollbar presence must converge within four states")
    }
}

/// Convenience constructor namespace for retained scroll areas.
pub struct ScrollArea;

impl ScrollArea {
    /// Creates the composite and returns its weak application state plus completed node.
    ///
    /// The completed parent always owns exactly three structural children: a virtual content
    /// container and two real scrollbar widgets. The surface behavior is installed directly on the
    /// parent `Container`; no special scroll-area container type or builder is introduced.
    pub fn create(parameters: ScrollAreaParameters) -> (WidgetStateHandle<ScrollAreaState>, Node) {
        // Separate immutable presentation flags from the dynamic enablement stored in typed state.
        let enabled = parameters.opt.intersects(ScrollAreaOption::ENABLE_SCROLL);
        let surface_opt = if parameters.opt.intersects(ScrollAreaOption::FRAME) {
            WidgetOption::FRAME
        } else {
            WidgetOption::NONE
        };
        // Build independently addressable scrollbar children and retain only their weak state handles.
        let (horizontal, horizontal_node) = RetainedScrollbar::create(ScrollAxis::Horizontal);
        let (vertical, vertical_node) = RetainedScrollbar::create(ScrollAxis::Vertical);
        // Allocate application content once; the virtual container receives the strong owner below.
        let content = Rc::new(RefCell::new(parameters.children));
        let state = Rc::new(RefCell::new(ScrollAreaState {
            content: ChildrenHandle::new(&content),
            horizontal: horizontal.clone(),
            vertical: vertical.clone(),
            scrolling_enabled: enabled,
            geometry: ScrollAreaGeometry::default(),
        }));

        // The virtual child is the sole strong owner of application content.
        let virtual_surface = Container::from_shared(
            content,
            VirtualSurfaceLayout {
                horizontal: horizontal.clone(),
                vertical: vertical.clone(),
            },
            WidgetOption::NONE,
        );

        // Parent layout coordinates the fixed child roles and retains the strong composite state.
        let layout = ScrollAreaLayout {
            state: state.clone(),
            horizontal,
            vertical,
        };
        // Surface state is weak so ScrollAreaLayout remains the sole strong composite-state owner.
        let surface = ScrollAreaSurface {
            state: Rc::downgrade(&state),
            opt: surface_opt,
        };
        // Publish the weak handle before moving all strong owners into the completed parent node.
        let handle = WidgetStateHandle::new(&state);
        let container = Container::new(layout, WidgetOption::NONE, [Node::container(virtual_surface), horizontal_node, vertical_node]).with_surface(surface);
        (handle, Node::container(container))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::input::Input;
    use crate::test_support::test_atlas;
    use crate::ui_node::UiRuntime;
    use crate::{
        Column, ColumnParameters, Custom, CustomParameters, MouseButton, Policy, Row, RowParameters, SizePolicy, Stack, StackDirection, StackParameters,
        TextBlock, TextBlockParameters, TextWrap, UNCLIPPED_RECT,
    };

    /// Lays out one fixed content node and returns the parent layout's committed summary.
    fn laid_out_geometry(child_size: Dimensioni, surface: Recti, style: Style, requested_offset: Vec2i) -> ScrollAreaGeometry {
        let child = Node::widget(Custom::create(CustomParameters::new("child"))).with_policy(Policy::fixed(child_size.width, child_size.height));
        let (scroll, mut root) = ScrollArea::create(ScrollAreaParameters::new(ScrollAreaOption::ENABLE_SCROLL, [child]));
        scroll.try_update(|state| state.set_offset(requested_offset)).unwrap();

        let mut runtime = UiRuntime::new();
        runtime.begin_update();
        runtime.layout_tree_root(&mut root, &style, test_atlas(), surface, UNCLIPPED_RECT);
        scroll.try_read(|state| state.geometry).unwrap()
    }

    #[test]
    fn scroll_area_keeps_three_structural_children_and_content_behind_virtual_surface() {
        let child = Custom::create(CustomParameters::new("child"));
        let (child_state, child) = Node::typed_widget(child);
        let (scroll, node) = ScrollArea::create(ScrollAreaParameters::new(ScrollAreaOption::FRAME | ScrollAreaOption::ENABLE_SCROLL, [child]));

        assert_eq!(scroll.try_read(ScrollAreaState::len), Some(Some(1)));
        assert_eq!(
            node.debug_node_count(),
            5,
            "parent, virtual surface, two bars, and content are all retained nodes"
        );
        scroll.try_update(|state| state.set_offset(Vec2i::new(-4, 12))).unwrap();
        assert_eq!(scroll.try_read(|state| (state.offset().x, state.offset().y)), Some((0, 12)));
        scroll.try_update(|state| state.set_scrolling_enabled(false)).unwrap();
        assert_eq!(scroll.try_read(|state| (state.offset().x, state.offset().y)), Some((0, 0)));

        assert_eq!(scroll.try_update(|state| state.remove_drop(0)), Some(Some(true)));
        assert!(!child_state.is_alive());
        drop(node);
        assert!(!scroll.is_alive());
    }

    #[test]
    fn disabling_scrolling_revokes_scrollbar_capture_through_participation() {
        let child = Node::widget(Custom::create(CustomParameters::new("child"))).with_policy(Policy::fixed(200, 200));
        let (scroll, mut root) = ScrollArea::create(ScrollAreaParameters::new(ScrollAreaOption::ENABLE_SCROLL, [child]));
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
    fn wrapped_remainder_content_does_not_manufacture_horizontal_overflow() {
        let (_, label) = TextBlock::create(TextBlockParameters::new("label"));
        let (_, text) = TextBlock::create(TextBlockParameters::with_wrap(
            "Lorem ipsum dolor sit amet, consectetur adipiscing elit. Maecenas lacinia, sem eu lacinia molestie, mi risus faucibus ipsum.",
            TextWrap::Word,
        ));
        let text_id = text.id();
        let (_, text_stack) = Stack::create(StackParameters::new(
            SizePolicy::Remainder(0),
            SizePolicy::Auto,
            StackDirection::TopToBottom,
            [text],
        ));
        let (_, text_column) = Column::create(ColumnParameters::new([text_stack]));
        let (_, row) = Row::create(RowParameters::new(
            [SizePolicy::Fixed(40), SizePolicy::Remainder(0)],
            SizePolicy::Auto,
            [label, text_column],
        ));
        let (scroll, mut root) = ScrollArea::create(ScrollAreaParameters::new(ScrollAreaOption::ENABLE_SCROLL, [row]));
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
        let child = Node::widget(Custom::create(CustomParameters::new("child"))).with_policy(Policy::fixed(160, 200));
        let child_id = child.id();
        let (scroll, mut root) = ScrollArea::create(ScrollAreaParameters::new(ScrollAreaOption::FRAME | ScrollAreaOption::ENABLE_SCROLL, [child]));
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
