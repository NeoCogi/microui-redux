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
//! Retained widget-tree traversal and cache management.
//!
//! The retained tree path is intentionally split into two passes:
//! 1. a layout pass that measures widgets, allocates rectangles, and records geometry into the
//!    per-frame tree cache;
//! 2. a render pass that reuses the cached rectangles, samples interaction, and records
//!    `NodeInteraction` entries for the same tree nodes.
//!
//! Keeping those passes separate lets the retained path reason about geometry deterministically:
//! widgets do not advance layout while rendering, containers can recurse into children using
//! already-computed rectangles, and tests can inspect the cache after either phase.

use super::*;

impl Container {
    /// Returns the previous frame layout for `node_id`, if any.
    #[cfg_attr(not(test), allow(dead_code))]
    pub(crate) fn previous_node_layout(&self, node_id: NodeId) -> Option<NodeLayout> {
        self.tree_cache.prev_layout(node_id).copied()
    }

    /// Returns the current frame layout for `node_id`, if any.
    #[cfg_attr(not(test), allow(dead_code))]
    pub(crate) fn current_node_layout(&self, node_id: NodeId) -> Option<NodeLayout> {
        self.tree_cache.current_layout(node_id).copied()
    }

    /// Stores the current frame geometry snapshot for a retained tree node.
    fn record_tree_layout(&mut self, node_id: NodeId, layout: NodeLayout) {
        self.tree_cache.record_layout(node_id, layout);
    }

    /// Stores the current frame control/result snapshot for a retained tree node.
    fn record_tree_interaction(&mut self, node_id: NodeId, interaction: NodeInteraction) {
        self.tree_cache.record_interaction(node_id, interaction);
    }

    /// Returns the current frame layout for `node_id` or panics if layout was skipped.
    fn current_tree_layout_or_panic(&self, node_id: NodeId) -> NodeLayout {
        self.tree_cache
            .current_layout(node_id)
            .copied()
            .unwrap_or_else(|| panic!("tree node {:?} missing current layout", node_id))
    }

    /// Synthesizes a structural node rect by unioning the current frame bounds of its children.
    fn record_tree_group_from_children(&mut self, node_id: NodeId, children: &[WidgetTreeNode]) {
        let mut bounds: Option<Recti> = None;
        for child in children {
            if let Some(child_state) = self.tree_cache.current_layout(child.id()) {
                // Structural nodes like rows, grids, and columns do not have their own widget
                // state; their effective bounds are the union of their children for the current
                // frame. That cached group rect is useful for debugging/tests and keeps the cache
                // shape uniform across leaf and non-leaf nodes.
                bounds = Some(match bounds {
                    Some(existing_rect) => {
                        let min_x = existing_rect.x.min(child_state.rect.x);
                        let min_y = existing_rect.y.min(child_state.rect.y);
                        let max_x = (existing_rect.x + existing_rect.width).max(child_state.rect.x + child_state.rect.width);
                        let max_y = (existing_rect.y + existing_rect.height).max(child_state.rect.y + child_state.rect.height);
                        rect(min_x, min_y, max_x - min_x, max_y - min_y)
                    }
                    None => child_state.rect,
                });
            }
        }

        if let Some(rect) = bounds {
            self.record_tree_layout(node_id, NodeLayout::new(rect, rect, vec2(rect.width, rect.height)));
        }
    }

    /// Runs the retained layout pass for a slice of sibling tree nodes.
    fn layout_tree_nodes(&mut self, results: &FrameResults, nodes: &[WidgetTreeNode]) {
        for node in nodes {
            self.layout_tree_node(results, node);
        }
    }

    /// Runs the retained render pass for a slice of sibling tree nodes.
    fn render_tree_nodes(&mut self, results: &mut FrameResults, nodes: &[WidgetTreeNode]) {
        for node in nodes {
            self.render_tree_node(results, node);
        }
    }

    /// Measures a leaf widget node and records its allocated rectangle in the tree cache.
    fn layout_tree_widget(&mut self, results: &FrameResults, node_id: NodeId, widget: &dyn WidgetStateHandleDyn) {
        // Measurement goes through the same generic widget dispatch used elsewhere in the
        // container. The retained path just captures the allocated rectangle in the tree cache
        // instead of consuming it immediately.
        let rect = self.layout_widget_dyn(results, widget);
        self.record_tree_layout(node_id, NodeLayout::new(rect, rect, Vec2i::default()));
    }

    /// Replays a leaf widget node using the rectangle captured during the layout pass.
    fn render_tree_widget(&mut self, results: &mut FrameResults, node_id: NodeId, widget: &dyn WidgetStateHandleDyn) {
        // Rendering must reuse the rect produced during layout. If a node reaches render without a
        // cached layout entry, the retained traversal is internally inconsistent and should fail
        // loudly instead of silently inventing geometry.
        let rect = self.current_tree_layout_or_panic(node_id).rect;
        let opt = widget.effective_widget_opt();
        let bopt = widget.effective_behaviour_opt();
        let input = if widget.needs_input_snapshot() { Some(self.snapshot_input()) } else { None };
        let (control, result) = self.render_widget_dyn(results, widget, rect, input, opt, bopt);
        self.record_tree_interaction(node_id, NodeInteraction::new(control, result));
    }

    /// Measures a retained custom-render node and records its allocated rectangle.
    fn layout_tree_custom_render(&mut self, results: &FrameResults, node_id: NodeId, state: &WidgetHandle<Custom>) {
        let rect = self.layout_widget_handle(results, state);
        self.record_tree_layout(node_id, NodeLayout::new(rect, rect, Vec2i::default()));
    }

    /// Executes a retained custom-render node and records both interaction and callback payload.
    fn render_tree_custom_render(&mut self, results: &mut FrameResults, node_id: NodeId, state: &WidgetHandle<Custom>, render: &TreeCustomRender) {
        let rect = self.current_tree_layout_or_panic(node_id).rect;
        let (opt, bopt, needs_input) = {
            let state = state.borrow();
            (state.effective_widget_opt(), state.effective_behaviour_opt(), state.needs_input_snapshot())
        };
        let input = if needs_input { Some(self.snapshot_input()) } else { None };
        let (control, result) = self.render_widget_handle(results, state, rect, input, opt, bopt);

        let snapshot = self.snapshot_input();
        let input_ref = snapshot.as_ref();
        let mouse_event = self.input_to_mouse_event(&control, input_ref, rect);

        // Custom render callbacks are fed the same normalized interaction payload that built-in
        // widgets observe. This keeps the custom path aligned with the retained widget contract:
        // geometry comes from the retained pass, input is localized to the widget rect, and the
        // backend-specific drawing work is deferred through the command list.
        let active = control.focused && self.in_hover_root;
        let key_mods = if active { input_ref.key_mods } else { KeyMode::NONE };
        let key_codes = if active { input_ref.key_codes } else { KeyCode::NONE };
        let text_input = if active { input_ref.text_input.clone() } else { String::new() };
        let cra = CustomRenderArgs {
            content_area: rect,
            view: self.get_clip_rect(),
            mouse_event,
            scroll_delta: control.scroll_delta,
            widget_opt: opt,
            behaviour_opt: bopt,
            key_mods,
            key_codes,
            text_input,
        };
        let render = render.clone();
        self.command_list.push(Command::CustomRender(
            cra,
            Box::new(move |dim, args| {
                (*render.borrow_mut())(dim, args);
            }),
        ));

        self.record_tree_interaction(node_id, NodeInteraction::new(control, result));
    }

    /// Measures a header/tree disclosure node and returns the stable expansion state used this frame.
    fn layout_tree_node_scope(&mut self, results: &FrameResults, node_id: NodeId, state: &WidgetHandle<Node>) -> NodeStateValue {
        // Header/tree nodes always reserve a full-width row for the disclosure widget itself. The
        // returned stable state decides whether children participate in the current layout pass.
        self.layout.row(&[SizePolicy::Remainder(0)], SizePolicy::Auto);
        let rect = self.layout_widget_handle(results, state);
        let stable_state = {
            let state = state.borrow();
            state.state
        };
        self.record_tree_layout(node_id, NodeLayout::new(rect, rect, Vec2i::default()));
        stable_state
    }

    /// Renders a header/tree disclosure node and returns the stable expansion state observed this frame.
    fn render_tree_node_scope(&mut self, results: &mut FrameResults, node_id: NodeId, state: &WidgetHandle<Node>) -> NodeStateValue {
        let rect = self.current_tree_layout_or_panic(node_id).rect;
        let (opt, bopt, stable_state) = {
            let state = state.borrow();
            (*state.widget_opt(), *state.behaviour_opt(), state.state)
        };
        let (control, result) = self.render_widget_handle(results, state, rect, None, opt, bopt);
        self.record_tree_interaction(node_id, NodeInteraction::new(control, result));
        stable_state
    }

    /// Performs the layout pass for one retained tree node, recursing into children when needed.
    fn layout_tree_node(&mut self, results: &FrameResults, node: &WidgetTreeNode) {
        let (node_id, kind, children) = node.parts();
        match kind {
            WidgetTreeNodeKind::Widget { widget } => {
                self.layout_tree_widget(results, node_id, &**widget);
            }
            WidgetTreeNodeKind::CustomRender { state, .. } => {
                self.layout_tree_custom_render(results, node_id, state);
            }
            WidgetTreeNodeKind::Container { handle, opt, behaviour } => {
                let mut handle = handle.clone();
                // Containers are nested retained sub-contexts. The parent records the panel bounds,
                // then lets the child container execute the same layout pass against its own body.
                self.begin_panel_layout(&mut handle, *opt, *behaviour);
                handle.with_mut(|container| {
                    container.layout_tree_nodes(results, children);
                });
                self.end_panel_layout(&mut handle);
                let (rect, body, content_size) = handle.with(|container| (container.rect(), container.body(), container.content_size()));
                self.record_tree_layout(node_id, NodeLayout::new(rect, body, content_size));
            }
            WidgetTreeNodeKind::Header { state } => {
                // Headers gate child participation entirely. In the strict retained model the
                // current stable state decides whether descendants exist for this frame.
                if self.layout_tree_node_scope(results, node_id, state).is_expanded() {
                    self.layout_tree_nodes(results, children);
                }
            }
            WidgetTreeNodeKind::Tree { state } => {
                if self.layout_tree_node_scope(results, node_id, state).is_expanded() {
                    // Tree nodes differ from plain headers only by indenting their descendants.
                    let indent_size = self.style.as_ref().indent;
                    self.layout.adjust_indent(indent_size);
                    self.layout_tree_nodes(results, children);
                    self.layout.adjust_indent(-indent_size);
                }
            }
            WidgetTreeNodeKind::Row { widths, height } => {
                // Structural layout nodes do not run widgets themselves; they only establish a new
                // layout scope for descendants and then synthesize a cached group rect afterward.
                self.with_row(widths, *height, |container| {
                    container.layout_tree_nodes(results, children);
                });
                self.record_tree_group_from_children(node_id, children);
            }
            WidgetTreeNodeKind::Grid { widths, heights } => {
                self.with_grid(widths, heights, |container| {
                    container.layout_tree_nodes(results, children);
                });
                self.record_tree_group_from_children(node_id, children);
            }
            WidgetTreeNodeKind::Column => {
                self.column(|container| {
                    container.layout_tree_nodes(results, children);
                });
                self.record_tree_group_from_children(node_id, children);
            }
            WidgetTreeNodeKind::Stack { width, height, direction } => {
                self.stack_with_width_direction(*width, *height, *direction, |container| {
                    container.layout_tree_nodes(results, children);
                });
                self.record_tree_group_from_children(node_id, children);
            }
        }
    }

    /// Performs the render pass for one retained tree node using cached layout from the first pass.
    fn render_tree_node(&mut self, results: &mut FrameResults, node: &WidgetTreeNode) {
        let (node_id, kind, children) = node.parts();
        match kind {
            WidgetTreeNodeKind::Widget { widget } => {
                self.render_tree_widget(results, node_id, &**widget);
            }
            WidgetTreeNodeKind::CustomRender { state, render } => {
                self.render_tree_custom_render(results, node_id, state, render);
            }
            WidgetTreeNodeKind::Container { handle, opt, behaviour } => {
                let mut handle = handle.clone();
                let layout = self.current_tree_layout_or_panic(node_id);
                // The render pass re-enters the panel with the layout snapshot already frozen.
                // Scrollbars, body clipping, and child drawing therefore use the same geometry that
                // was computed during the first pass.
                self.begin_panel_render(&mut handle, *opt, *behaviour, layout);
                handle.with_mut(|container| {
                    container.render_tree_nodes(results, children);
                });
                self.end_panel_render(&mut handle);
            }
            WidgetTreeNodeKind::Header { state } => {
                if self.render_tree_node_scope(results, node_id, state).is_expanded() {
                    self.render_tree_nodes(results, children);
                }
            }
            WidgetTreeNodeKind::Tree { state } => {
                if self.render_tree_node_scope(results, node_id, state).is_expanded() {
                    self.render_tree_nodes(results, children);
                }
            }
            WidgetTreeNodeKind::Row { .. } | WidgetTreeNodeKind::Grid { .. } | WidgetTreeNodeKind::Column | WidgetTreeNodeKind::Stack { .. } => {
                // Structural nodes simply forward rendering to descendants because their own cached
                // bounds were already synthesized during layout.
                self.render_tree_nodes(results, children);
            }
        }
    }

    /// Evaluates a prebuilt widget tree using the current container layout.
    pub(crate) fn widget_tree(&mut self, results: &mut FrameResults, tree: &WidgetTree) {
        // Layout must always happen before rendering so the retained cache contains every rect the
        // second pass expects to reuse.
        self.layout_tree_nodes(results, tree.roots());
        self.render_tree_nodes(results, tree.roots());
    }

    /// Measures a prebuilt widget tree using the current container layout without rendering it.
    pub(crate) fn measure_widget_tree_content(&mut self, results: &FrameResults, tree: &WidgetTree) -> Vec2i {
        self.layout_tree_nodes(results, tree.roots());

        let content_size = match self.layout.current_max() {
            Some(max_rect) => {
                let body = self.layout.current_body();
                Vec2i::new(max_rect.x - body.x, max_rect.y - body.y)
            }
            None => Vec2i::default(),
        };

        // Discard provisional cache entries from the measurement-only pass so the real render pass
        // can record the current frame layout and interaction exactly once.
        self.tree_cache.begin_frame();
        content_size
    }
}
