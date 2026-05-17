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
//! Container state and traversal logic are split by concern so rendering,
//! interaction, retained-tree traversal, and widget helpers can evolve
//! independently without one file becoming the crate's de facto core.

use super::*;
use crate::draw_context::DrawCtx;
use crate::scrollbar::{scrollbar_base, scrollbar_drag_delta, scrollbar_max_scroll, scrollbar_thumb, ScrollAxis};
use crate::text_layout::build_text_lines;
use crate::widget::{FocusPolicy, InteractionId, WidgetId};
use crate::widget_tree::{Policy, TreeCustomRender, WidgetHandle, WidgetStateHandleDyn, WidgetTreeNode, WidgetTreeNodeKind};
use std::cell::RefCell;

mod command;
pub use command::{CustomRenderArgs, CustomRenderCommand, TextWrap};
pub(crate) use command::Command;

mod dispatch;
mod draw;
mod interaction;
mod layout_api;
mod panels;
mod tree;

#[cfg(test)]
mod tests;

/// Core UI building block that records commands and hosts layouts.
pub struct Container {
    atlas: AtlasHandle,
    /// Style used when drawing widgets in the container.
    style: Rc<Style>,
    /// Human-readable name for the container.
    name: String,
    /// Outer rectangle including frame and title.
    rect: Recti,
    /// Inner rectangle excluding frame/title.
    body: Recti,
    /// Size of the content region based on layout traversal.
    content_size: Dimensioni,
    /// Accumulated scroll offset.
    scroll: Vec2i,
    /// Z-index used to order overlapping windows.
    zindex: i32,
    draw: DrawState,
    layout: LayoutManager,
    interaction: InteractionState,
    /// Internal state for the vertical scrollbar.
    scrollbar_y_state: Internal,
    /// Internal state for the horizontal scrollbar.
    scrollbar_x_state: Internal,
    /// Shared access to the input state.
    input: Rc<RefCell<Input>>,
    /// Determines whether container scrollbars and scroll consumption are enabled.
    scroll_enabled: bool,
    /// True when this container is a scratch container used only for measurement.
    measurement_mode: bool,
    tree: TreeState,
    panels: PanelState,
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

#[derive(Default)]
struct InteractionState {
    /// ID of the widget currently hovered, if any.
    hover: Option<InteractionId>,
    /// ID of the widget currently focused, if any.
    focus: Option<InteractionId>,
    /// Child container that currently owns pointer routing inside this container.
    hover_root_child: Option<ContainerId>,
    /// Rectangle occupied by the child container that currently owns pointer routing.
    hover_root_child_rect: Option<Recti>,
    /// Child container selected to own pointer routing on the next frame.
    next_hover_root_child: Option<ContainerId>,
    /// Rectangle for the child container selected to own pointer routing on the next frame.
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

#[derive(Default)]
struct TreeState {
    /// Previous/current frame cache for tree node geometry and interaction state.
    cache: WidgetTreeCache,
}

#[derive(Default)]
struct PanelState {
    /// Embedded panels active in the current retained traversal.
    active: Vec<ContainerHandle>,
}

impl Container {
    pub(crate) fn new(name: &str, atlas: AtlasHandle, style: Rc<Style>, input: Rc<RefCell<Input>>) -> Self {
        Self {
            name: name.to_string(),
            style,
            atlas,
            rect: Recti::default(),
            body: Recti::default(),
            content_size: Dimensioni::default(),
            scroll: Vec2i::default(),
            zindex: 0,
            draw: DrawState::default(),
            interaction: InteractionState::default(),
            layout: LayoutManager::default(),
            scrollbar_y_state: Internal::new("!scrollbary"),
            scrollbar_x_state: Internal::new("!scrollbarx"),
            input,
            scroll_enabled: true,
            measurement_mode: false,
            tree: TreeState::default(),
            panels: PanelState::default(),
        }
    }

    pub(crate) fn reset(&mut self) {
        self.draw.commands.clear();
        self.draw.triangle_vertices.clear();
        self.draw.clip_stack.clear();
        self.body = Recti::default();
        self.content_size = Dimensioni::default();
        self.scroll = Vec2i::default();
        self.interaction.hover = None;
        self.interaction.focus = None;
        self.clear_root_frame_state();
        self.interaction.updated_focus = false;
        self.interaction.input_snapshot = None;
        self.interaction.popup_just_opened = false;
        self.scroll_enabled = true;
        self.measurement_mode = false;
        self.panels.active.clear();
        self.tree.cache.clear();
    }

    pub(crate) fn clear_root_frame_state(&mut self) {
        self.interaction.hover_root_child = None;
        self.interaction.hover_root_child_rect = None;
        self.interaction.next_hover_root_child = None;
        self.interaction.next_hover_root_child_rect = None;
        self.interaction.in_hover_root = false;
        self.interaction.pending_scroll = None;
    }

    pub(crate) fn prepare(&mut self) {
        self.draw.commands.clear();
        assert!(self.draw.clip_stack.is_empty());
        self.panels.active.clear();
        self.interaction.input_snapshot = None;
        self.interaction.next_hover_root_child = None;
        self.interaction.next_hover_root_child_rect = None;
        self.interaction.pending_scroll = None;
        self.scroll_enabled = true;
        self.tree.cache.begin_frame();
    }

    pub(crate) fn measurement_scratch(&self) -> Self {
        let mut scratch = Container::new(&self.name, self.atlas.clone(), self.style.clone(), self.input.clone());
        scratch.rect = self.rect;
        scratch.body = self.body;
        scratch.content_size = self.content_size;
        scratch.scroll = self.scroll;
        scratch.zindex = self.zindex;
        scratch.layout = self.layout.clone();
        scratch.scroll_enabled = self.scroll_enabled;
        scratch.measurement_mode = true;
        scratch
    }

    pub(crate) fn seed_pending_scroll(&mut self, delta: Option<Vec2i>) {
        self.interaction.pending_scroll = delta;
    }

    pub(crate) fn begin_root_command_scope(&mut self, pending_scroll: Option<Vec2i>) {
        self.seed_pending_scroll(pending_scroll);
        self.draw.clip_stack.push(UNCLIPPED_RECT);
    }

    pub(crate) fn finish_root_command_scope(&mut self) {
        self.pop_clip_rect();

        let layout_body = self.layout.current_body();
        if let Some(lm) = self.layout.current_max() {
            self.set_content_size(Dimensioni::new(lm.x - layout_body.x, lm.y - layout_body.y));
        }
        self.render_active_scrollbars();
        self.consume_pending_scroll();
        self.layout.pop_scope();
    }

    /// Resets transient per-frame state after widgets have been processed.
    pub fn finish(&mut self) {
        for panel in &mut self.panels.active {
            panel.finish();
        }
        if !self.interaction.updated_focus {
            self.interaction.focus = None;
        }
        self.interaction.updated_focus = false;
        self.interaction.hover_root_child = self.interaction.next_hover_root_child;
        self.interaction.hover_root_child_rect = self.interaction.next_hover_root_child_rect;
        self.interaction.next_hover_root_child = None;
        self.interaction.next_hover_root_child_rect = None;
        self.tree.cache.finish_frame();
    }

    /// Returns the outer container rectangle.
    pub fn rect(&self) -> Recti {
        self.rect
    }

    /// Sets the outer container rectangle.
    pub fn set_rect(&mut self, rect: Recti) {
        self.rect = rect;
    }

    pub(crate) fn set_rect_size(&mut self, size: Dimensioni) {
        self.rect.width = size.width;
        self.rect.height = size.height;
    }

    pub(crate) fn translate_rect(&mut self, delta: Vec2i) {
        self.rect.x += delta.x;
        self.rect.y += delta.y;
    }

    pub(crate) fn resize_rect_by(&mut self, delta: Vec2i, min_size: Dimensioni) {
        self.rect.width = (self.rect.width + delta.x).max(min_size.width);
        self.rect.height = (self.rect.height + delta.y).max(min_size.height);
    }

    pub(crate) fn contains_point(&self, point: Vec2i) -> bool {
        self.rect.contains(&point)
    }

    /// Returns the inner container body rectangle.
    pub fn body(&self) -> Recti {
        self.body
    }

    /// Returns the current scroll offset.
    pub fn scroll(&self) -> Vec2i {
        self.scroll
    }

    /// Sets the current scroll offset.
    pub fn set_scroll(&mut self, scroll: Vec2i) {
        self.scroll = scroll;
    }

    /// Returns the content size derived from layout traversal.
    pub fn content_size(&self) -> Dimensioni {
        self.content_size
    }

    pub(crate) fn set_content_size(&mut self, content_size: Dimensioni) {
        self.content_size = content_size;
    }

    pub(crate) fn clear_content_and_scroll(&mut self) {
        self.content_size = Dimensioni::default();
        self.scroll = Vec2i::default();
    }

    pub(crate) fn name(&self) -> &str {
        &self.name
    }

    pub(crate) fn atlas(&self) -> &AtlasHandle {
        &self.atlas
    }

    pub(crate) fn style(&self) -> &Style {
        self.style.as_ref()
    }

    pub(crate) fn set_style_handle(&mut self, style: Rc<Style>) {
        self.style = style;
    }

    pub(crate) fn input(&self) -> &Rc<RefCell<Input>> {
        &self.input
    }

    pub(crate) fn zindex(&self) -> i32 {
        self.zindex
    }

    pub(crate) fn set_zindex(&mut self, zindex: i32) {
        self.zindex = zindex;
    }

    pub(crate) fn in_hover_root(&self) -> bool {
        self.interaction.in_hover_root
    }

    pub(crate) fn set_in_hover_root(&mut self, in_hover_root: bool) {
        self.interaction.in_hover_root = in_hover_root;
    }

    pub(crate) fn popup_just_opened(&self) -> bool {
        self.interaction.popup_just_opened
    }

    pub(crate) fn clear_popup_just_opened(&mut self) {
        self.interaction.popup_just_opened = false;
    }

    pub(crate) fn mark_popup_just_opened(&mut self) {
        self.interaction.popup_just_opened = true;
    }

    #[cfg(test)]
    pub(crate) fn debug_commands(&self) -> &[Command] {
        &self.draw.commands
    }

    #[cfg(test)]
    pub(crate) fn debug_push_command(&mut self, command: Command) {
        self.draw.commands.push(command);
    }

    #[cfg(test)]
    pub(crate) fn debug_push_clip(&mut self, rect: Recti) {
        self.draw.clip_stack.push(rect);
    }

    #[cfg(test)]
    pub(crate) fn panel_count(&self) -> usize {
        self.panels.active.len()
    }

    fn clamp(x: i32, a: i32, b: i32) -> i32 {
        min(b, max(a, x))
    }

    // Baseline alignment is shared by text blocks and control labels.
    fn baseline_aligned_top(rect: Recti, line_height: i32, baseline: i32) -> i32 {
        if rect.height >= line_height {
            return rect.y + (rect.height - line_height) / 2;
        }

        let baseline_center = rect.y + rect.height / 2;
        let min_top = rect.y + rect.height - line_height;
        let max_top = rect.y;
        Self::clamp(baseline_center - baseline, min_top, max_top)
    }

    fn vertical_text_padding(padding: i32) -> i32 {
        max(1, padding / 2)
    }
}
