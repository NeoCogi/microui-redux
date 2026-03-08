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
use crate::widget_tree::{TreeCustomRender, WidgetHandle, WidgetStateHandleDyn, WidgetTreeNode, WidgetTreeNodeKind};
use std::cell::RefCell;

mod command;
pub use command::{CustomRenderArgs, TextWrap};
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
    pub(crate) atlas: AtlasHandle,
    /// Style used when drawing widgets in the container.
    pub(crate) style: Rc<Style>,
    /// Human-readable name for the container.
    pub(crate) name: String,
    /// Outer rectangle including frame and title.
    pub(crate) rect: Recti,
    /// Inner rectangle excluding frame/title.
    pub(crate) body: Recti,
    /// Size of the content region based on layout traversal.
    pub(crate) content_size: Vec2i,
    /// Accumulated scroll offset.
    pub(crate) scroll: Vec2i,
    /// Z-index used to order overlapping windows.
    pub(crate) zindex: i32,
    /// Recorded draw commands for this frame.
    pub(crate) command_list: Vec<Command>,
    /// Stack of clip rectangles applied while drawing.
    pub(crate) clip_stack: Vec<Recti>,
    pub(crate) layout: LayoutManager,
    /// ID of the widget currently hovered, if any.
    pub(crate) hover: Option<WidgetId>,
    /// ID of the widget currently focused, if any.
    pub(crate) focus: Option<WidgetId>,
    /// Child container that currently owns pointer routing inside this container.
    hover_root_child: Option<ContainerId>,
    /// Rectangle occupied by the child container that currently owns pointer routing.
    hover_root_child_rect: Option<Recti>,
    /// Child container selected to own pointer routing on the next frame.
    next_hover_root_child: Option<ContainerId>,
    /// Rectangle for the child container selected to own pointer routing on the next frame.
    next_hover_root_child_rect: Option<Recti>,
    /// Tracks whether focus changed this frame.
    pub(crate) updated_focus: bool,
    /// Internal state for the vertical scrollbar.
    pub(crate) scrollbar_y_state: Internal,
    /// Internal state for the horizontal scrollbar.
    pub(crate) scrollbar_x_state: Internal,
    /// Shared access to the input state.
    pub(crate) input: Rc<RefCell<Input>>,
    /// Cached per-frame input snapshot for widgets that need it.
    input_snapshot: Option<Rc<InputSnapshot>>,
    /// Whether this container is the current hover root.
    pub(crate) in_hover_root: bool,
    /// Tracks whether a popup was just opened this frame to avoid instant auto-close.
    pub(crate) popup_just_opened: bool,
    pending_scroll: Option<Vec2i>,
    /// Determines whether container scrollbars and scroll consumption are enabled.
    scroll_enabled: bool,
    /// Previous/current frame cache for tree node geometry and interaction state.
    tree_cache: WidgetTreeCache,
    panels: Vec<ContainerHandle>,
}

impl Container {
    pub(crate) fn new(name: &str, atlas: AtlasHandle, style: Rc<Style>, input: Rc<RefCell<Input>>) -> Self {
        Self {
            name: name.to_string(),
            style,
            atlas,
            rect: Recti::default(),
            body: Recti::default(),
            content_size: Vec2i::default(),
            scroll: Vec2i::default(),
            zindex: 0,
            command_list: Vec::default(),
            clip_stack: Vec::default(),
            hover: None,
            focus: None,
            hover_root_child: None,
            hover_root_child_rect: None,
            next_hover_root_child: None,
            next_hover_root_child_rect: None,
            updated_focus: false,
            layout: LayoutManager::default(),
            scrollbar_y_state: Internal::new("!scrollbary"),
            scrollbar_x_state: Internal::new("!scrollbarx"),
            popup_just_opened: false,
            in_hover_root: false,
            input,
            input_snapshot: None,
            pending_scroll: None,
            scroll_enabled: true,
            tree_cache: WidgetTreeCache::default(),
            panels: Default::default(),
        }
    }

    pub(crate) fn reset(&mut self) {
        self.command_list.clear();
        self.clip_stack.clear();
        self.body = Recti::default();
        self.content_size = Vec2i::default();
        self.scroll = Vec2i::default();
        self.hover = None;
        self.focus = None;
        self.clear_root_frame_state();
        self.updated_focus = false;
        self.input_snapshot = None;
        self.popup_just_opened = false;
        self.scroll_enabled = true;
        self.panels.clear();
        self.tree_cache.clear();
    }

    pub(crate) fn clear_root_frame_state(&mut self) {
        self.hover_root_child = None;
        self.hover_root_child_rect = None;
        self.next_hover_root_child = None;
        self.next_hover_root_child_rect = None;
        self.in_hover_root = false;
        self.pending_scroll = None;
    }

    pub(crate) fn prepare(&mut self) {
        self.command_list.clear();
        assert!(self.clip_stack.is_empty());
        self.panels.clear();
        self.input_snapshot = None;
        self.next_hover_root_child = None;
        self.next_hover_root_child_rect = None;
        self.pending_scroll = None;
        self.scroll_enabled = true;
        self.tree_cache.begin_frame();
    }

    pub(crate) fn seed_pending_scroll(&mut self, delta: Option<Vec2i>) {
        self.pending_scroll = delta;
    }

    /// Resets transient per-frame state after widgets have been processed.
    pub fn finish(&mut self) {
        for panel in &mut self.panels {
            panel.finish();
        }
        if !self.updated_focus {
            self.focus = None;
        }
        self.updated_focus = false;
        self.hover_root_child = self.next_hover_root_child;
        self.hover_root_child_rect = self.next_hover_root_child_rect;
        self.next_hover_root_child = None;
        self.next_hover_root_child_rect = None;
        self.tree_cache.finish_frame();
    }

    /// Returns the outer container rectangle.
    pub fn rect(&self) -> Recti {
        self.rect
    }

    /// Sets the outer container rectangle.
    pub fn set_rect(&mut self, rect: Recti) {
        self.rect = rect;
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
    pub fn content_size(&self) -> Vec2i {
        self.content_size
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
