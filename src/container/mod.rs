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
//! Traversal state and retained scroll-area logic are split by concern so rendering,
//! interaction, retained-tree traversal, and widget helpers can evolve
//! independently without one file becoming the crate's de facto core.

use super::*;
use crate::draw_context::DrawCtx;
use crate::widget::{FocusPolicy, RetainedId};
use crate::widget_tree::{
    erased_widget_state, NodeId, NodeLayout, Policy, TreeCustomRender, WidgetHandle, WidgetStateHandleDyn, WidgetTreeNode, WidgetTreeNodeKind,
    WidgetTreeResources,
};
use std::cell::RefCell;

mod command;
pub use command::{CustomRenderArgs, CustomRenderCommand, TextWrap};
pub(crate) use command::Command;

mod dispatch;
mod draw;
mod interaction;
mod layout_api;
mod measurement;
pub(crate) use measurement::MeasurementContext;
mod scroll;
mod scroll_area;
mod tree;

#[cfg(test)]
mod tests;

use scroll::ScrollState;
pub use scroll_area::ScrollArea;

/// Persistent viewport state shared by root bodies and retained scroll areas.
#[derive(Clone)]
struct ViewportState {
    /// Outer rectangle including frame and title.
    rect: Recti,
    /// Inner rectangle excluding frame/title and visible scrollbars.
    body: Recti,
    /// Size of the content region based on layout traversal.
    content_size: Dimensioni,
    scroll: ScrollState,
}

impl Default for ViewportState {
    fn default() -> Self {
        Self {
            rect: Recti::default(),
            body: Recti::default(),
            content_size: Dimensioni::default(),
            scroll: ScrollState::default(),
        }
    }
}

impl ViewportState {
    /// Clears viewport-local state when a root or scroll area is reset.
    fn reset(&mut self) {
        self.body = Recti::default();
        self.content_size = Dimensioni::default();
        self.scroll.reset();
    }

    /// Prepares viewport-local state for a new traversal.
    fn prepare_frame(&mut self) {
        self.scroll.prepare_frame();
    }

    /// Applies a frozen retained-tree layout snapshot to this viewport.
    fn apply_layout(&mut self, layout: NodeLayout) {
        self.rect = layout.rect;
        self.body = layout.body;
        self.content_size = layout.content_size;
    }
}

/// Per-traversal execution state shared by root windows and retained scroll areas.
///
/// `TraversalHost` owns layout, draw, interaction, and retained-tree caches for one traversed
/// body. Viewport geometry and scroll offsets are kept in `ViewportState`; root-only concerns such
/// as z-order and window lifecycle stay on `Window`; retained child state lives in `ScrollArea`.
pub struct TraversalHost {
    atlas: AtlasHandle,
    /// Style used when drawing widgets in the container.
    style: Rc<Style>,
    /// Human-readable name for the container.
    name: String,
    viewport: ViewportState,
    /// Stable seed used to derive internal retained node IDs for framework controls.
    internal_id_seed: Id,
    draw: DrawState,
    layout: LayoutManager,
    interaction: InteractionState,
    /// Shared access to the input state.
    input: Rc<RefCell<Input>>,
    /// Previous/current frame cache for retained tree node geometry and interaction state.
    tree_cache: WidgetTreeCache,
}

#[derive(Default)]
struct DrawState {
    /// Recorded draw commands for this frame.
    commands: Vec<Command>,
    /// Shared triangle vertex arena referenced by retained triangle commands.
    triangle_vertices: Vec<Vertex>,
    /// Stack of clip rectangles applied while drawing.
    clip_stack: Vec<Recti>,
}

impl DrawState {
    /// Clears every frame-owned draw buffer.
    fn clear(&mut self) {
        self.commands.clear();
        self.triangle_vertices.clear();
        self.clip_stack.clear();
    }

    /// Clears commands while preserving triangle allocation capacity.
    fn clear_commands(&mut self) {
        self.commands.clear();
    }

    /// Clears the retained custom-triangle arena.
    fn clear_triangle_vertices(&mut self) {
        self.triangle_vertices.clear();
    }

    /// Verifies that all scoped clips were popped before a new pass starts.
    fn assert_clip_stack_empty(&self) {
        assert!(self.clip_stack.is_empty());
    }

    /// Pushes a clip rectangle without intersecting it with the current clip.
    fn push_raw_clip(&mut self, rect: Recti) {
        self.clip_stack.push(rect);
    }

    /// Appends a command to the current command stream.
    fn push_command(&mut self, command: Command) {
        self.commands.push(command);
    }

    /// Creates a draw context over the container's mutable drawing buffers.
    fn ctx<'a>(&'a mut self, style: &'a Style, atlas: &'a AtlasHandle) -> DrawCtx<'a> {
        DrawCtx::new(&mut self.commands, &mut self.triangle_vertices, &mut self.clip_stack, style, atlas)
    }
}

#[derive(Default)]
struct InteractionState {
    /// ID of the widget currently hovered, if any.
    hover: Option<RetainedId>,
    /// ID of the widget currently focused, if any.
    focus: Option<RetainedId>,
    /// Retained scroll-area node that currently owns pointer routing inside this container.
    hover_root_child: Option<RetainedId>,
    /// Rectangle occupied by the child scroll area that currently owns pointer routing.
    hover_root_child_rect: Option<Recti>,
    /// Retained scroll-area node selected to own pointer routing on the next frame.
    next_hover_root_child: Option<RetainedId>,
    /// Rectangle for the child scroll area selected to own pointer routing on the next frame.
    next_hover_root_child_rect: Option<Recti>,
    /// Tracks whether focus changed this frame.
    updated_focus: bool,
    /// Cached per-frame input snapshot for widgets that need it.
    input_snapshot: Option<Rc<InputSnapshot>>,
    /// Whether this container is the current hover root.
    in_hover_root: bool,
    /// Tracks whether a popup was just opened this frame to avoid instant auto-close.
    popup_just_opened: bool,
    /// Pending scroll delta that can be consumed by the active container/widget.
    pending_scroll: Option<Vec2i>,
}

impl InteractionState {
    /// Clears all persistent and frame-local interaction state.
    fn reset_all(&mut self) {
        self.hover = None;
        self.focus = None;
        self.clear_root_frame_state();
        self.updated_focus = false;
        self.input_snapshot = None;
        self.popup_just_opened = false;
    }

    /// Clears root-local hover and scroll routing fields without touching focused widget state.
    fn clear_root_frame_state(&mut self) {
        self.hover_root_child = None;
        self.hover_root_child_rect = None;
        self.next_hover_root_child = None;
        self.next_hover_root_child_rect = None;
        self.in_hover_root = false;
        self.pending_scroll = None;
    }

    /// Prepares transient interaction fields for a new traversal.
    fn prepare_frame(&mut self) {
        self.input_snapshot = None;
        self.next_hover_root_child = None;
        self.next_hover_root_child_rect = None;
        self.pending_scroll = None;
    }

    /// Publishes next-hover routing and clears focus when no focused widget was seen this frame.
    fn finish_frame(&mut self) {
        if !self.updated_focus {
            // Retained focus survives only when the focused widget participates in the frame.
            self.focus = None;
        }
        self.updated_focus = false;
        self.hover_root_child = self.next_hover_root_child;
        self.hover_root_child_rect = self.next_hover_root_child_rect;
        self.next_hover_root_child = None;
        self.next_hover_root_child_rect = None;
    }

    /// Sets focused widget id and marks focus as refreshed for this frame.
    fn set_focus(&mut self, retained_id: RetainedId) {
        self.focus = Some(retained_id);
        self.updated_focus = true;
    }

    /// Clears focused widget id and marks focus as handled for this frame.
    fn clear_focus(&mut self) {
        self.focus = None;
        self.updated_focus = true;
    }

    /// Keeps the current focus alive when a focused widget is encountered.
    fn mark_focus_seen(&mut self) {
        self.updated_focus = true;
    }

    /// Records which child scroll area should receive hover routing on the next frame.
    fn set_next_hover_root_child(&mut self, scroll_area_id: RetainedId, rect: Recti) {
        self.next_hover_root_child = Some(scroll_area_id);
        self.next_hover_root_child_rect = Some(rect);
    }

    /// Seeds scroll delta that may be consumed by this container or active children.
    fn seed_pending_scroll(&mut self, delta: Option<Vec2i>) {
        self.pending_scroll = delta;
    }

    /// Removes and returns the pending scroll delta.
    fn take_pending_scroll(&mut self) -> Option<Vec2i> {
        self.pending_scroll.take()
    }

    /// Clears pending scroll once a container/widget consumes it.
    fn clear_pending_scroll(&mut self) {
        self.pending_scroll = None;
    }
}

impl TraversalHost {
    /// Creates a traversal host with persistent retained state and shared style/input handles.
    pub(crate) fn new(name: &str, atlas: AtlasHandle, style: Rc<Style>, input: Rc<RefCell<Input>>) -> Self {
        Self {
            name: name.to_string(),
            style,
            atlas,
            viewport: ViewportState::default(),
            internal_id_seed: Id::from_str(name),
            draw: DrawState::default(),
            interaction: InteractionState::default(),
            layout: LayoutManager::default(),
            input,
            tree_cache: WidgetTreeCache::default(),
        }
    }

    /// Overrides the seed used to derive framework-owned retained ids.
    pub(crate) fn set_internal_id_seed(&mut self, seed: Id) {
        self.internal_id_seed = seed;
    }

    /// Clears persistent container state when a root closes or is recreated.
    pub(crate) fn reset(&mut self) {
        self.draw.clear();
        self.viewport.reset();
        self.interaction.reset_all();
        self.tree_cache.clear();
    }

    /// Clears root-only frame routing while preserving widget focus and scroll-area state.
    pub(crate) fn clear_root_frame_state(&mut self) {
        self.interaction.clear_root_frame_state();
    }

    /// Prepares command, scroll-area, interaction, and tree-cache state for traversal.
    pub(crate) fn prepare(&mut self) {
        self.draw.clear_commands();
        self.draw.assert_clip_stack_empty();
        self.interaction.prepare_frame();
        self.viewport.prepare_frame();
        self.tree_cache.begin_frame();
    }

    /// Seeds scroll delta before a root or scroll-area traversal starts.
    pub(crate) fn seed_pending_scroll(&mut self, delta: Option<Vec2i>) {
        self.interaction.seed_pending_scroll(delta);
    }

    /// Begins a root command scope with an unclipped base clip.
    pub(crate) fn begin_root_command_scope(&mut self, pending_scroll: Option<Vec2i>) {
        self.seed_pending_scroll(pending_scroll);
        self.draw.push_raw_clip(UNCLIPPED_RECT);
    }

    /// Ends a root command scope.
    pub(crate) fn finish_root_command_scope(&mut self) {
        self.pop_clip_rect();
    }

    /// Resets transient per-frame state after widgets have been processed.
    pub fn finish(&mut self) {
        self.interaction.finish_frame();
        self.tree_cache.finish_frame();
    }

    /// Returns the outer container rectangle.
    pub fn rect(&self) -> Recti {
        self.viewport.rect
    }

    /// Sets the outer container rectangle.
    pub fn set_rect(&mut self, rect: Recti) {
        self.viewport.rect = rect;
    }

    /// Updates the outer container size without moving its origin.
    pub(crate) fn set_rect_size(&mut self, size: Dimensioni) {
        self.viewport.rect.width = size.width;
        self.viewport.rect.height = size.height;
    }

    /// Moves the outer rectangle by a drag delta.
    pub(crate) fn translate_rect(&mut self, delta: Vec2i) {
        self.viewport.rect.x += delta.x;
        self.viewport.rect.y += delta.y;
    }

    /// Resizes the outer rectangle while respecting a minimum size.
    pub(crate) fn resize_rect_by(&mut self, delta: Vec2i, min_size: Dimensioni) {
        self.viewport.rect.width = (self.viewport.rect.width + delta.x).max(min_size.width);
        self.viewport.rect.height = (self.viewport.rect.height + delta.y).max(min_size.height);
    }

    /// Returns whether a point is inside the outer container rectangle.
    pub(crate) fn contains_point(&self, point: Vec2i) -> bool {
        self.viewport.rect.contains(&point)
    }

    /// Returns the inner container body rectangle.
    pub fn body(&self) -> Recti {
        self.viewport.body
    }

    /// Sets the inner container body rectangle.
    #[cfg(test)]
    pub(crate) fn set_body(&mut self, body: Recti) {
        self.viewport.body = body;
    }

    /// Returns the current scroll offset.
    pub fn scroll(&self) -> Vec2i {
        self.viewport.scroll.offset()
    }

    /// Sets the current scroll offset.
    pub fn set_scroll(&mut self, scroll: Vec2i) {
        self.viewport.scroll.set_offset(scroll);
    }

    /// Returns the content size derived from layout traversal.
    pub fn content_size(&self) -> Dimensioni {
        self.viewport.content_size
    }

    /// Stores content size measured during layout traversal.
    pub(crate) fn set_content_size(&mut self, content_size: Dimensioni) {
        self.viewport.content_size = content_size;
    }

    /// Applies a frozen retained-tree layout snapshot to the current viewport.
    pub(crate) fn apply_viewport_layout(&mut self, layout: NodeLayout) {
        self.viewport.apply_layout(layout);
    }

    /// Returns the container's debug/display name.
    pub(crate) fn name(&self) -> &str {
        &self.name
    }

    /// Returns the atlas used by widgets inside this container.
    pub(crate) fn atlas(&self) -> &AtlasHandle {
        &self.atlas
    }

    /// Returns the style used by widgets inside this container.
    pub(crate) fn style(&self) -> &Style {
        self.style.as_ref()
    }

    /// Replaces the shared style handle used by the container.
    pub(crate) fn set_style_handle(&mut self, style: Rc<Style>) {
        self.style = style;
    }

    /// Returns the shared input handle.
    pub(crate) fn input(&self) -> &Rc<RefCell<Input>> {
        &self.input
    }

    /// Returns whether this root or scroll area currently receives hover routing.
    pub(crate) fn in_hover_root(&self) -> bool {
        self.interaction.in_hover_root
    }

    /// Sets whether this root or scroll area currently receives hover routing.
    pub(crate) fn set_in_hover_root(&mut self, in_hover_root: bool) {
        self.interaction.in_hover_root = in_hover_root;
    }

    /// Returns whether a popup was just opened this frame.
    pub(crate) fn popup_just_opened(&self) -> bool {
        self.interaction.popup_just_opened
    }

    /// Clears the just-opened popup guard.
    pub(crate) fn clear_popup_just_opened(&mut self) {
        self.interaction.popup_just_opened = false;
    }

    /// Sets the just-opened popup guard.
    pub(crate) fn mark_popup_just_opened(&mut self) {
        self.interaction.popup_just_opened = true;
    }

    #[cfg(test)]
    pub(crate) fn debug_commands(&self) -> &[Command] {
        &self.draw.commands
    }

    #[cfg(test)]
    pub(crate) fn debug_push_command(&mut self, command: Command) {
        self.draw.push_command(command);
    }

    #[cfg(test)]
    pub(crate) fn debug_push_clip(&mut self, rect: Recti) {
        self.draw.push_raw_clip(rect);
    }

    /// Clamps `x` into the inclusive range `[a, b]`.
    fn clamp(x: i32, a: i32, b: i32) -> i32 {
        min(b, max(a, x))
    }
}
