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

use super::*;

impl Container {
    /// Returns the previous frame cache entry for `node_id`, if any.
    pub fn previous_node_state(&self, node_id: NodeId) -> Option<NodeCacheEntry> {
        self.tree_cache.prev(node_id).copied()
    }

    /// Returns the current frame cache entry for `node_id`, if any.
    pub fn current_node_state(&self, node_id: NodeId) -> Option<NodeCacheEntry> {
        self.tree_cache.current(node_id).copied()
    }

    fn run_node_scope(&mut self, results: &mut FrameResults, state: &mut Node) -> NodeStateValue {
        self.layout.row(&[SizePolicy::Remainder(0)], SizePolicy::Auto);
        let rect = self.next_widget_rect(state);
        let opt = *state.widget_opt();
        let bopt = *state.behaviour_opt();
        let _ = self.handle_widget_in_rect(results, state, rect, None, opt, bopt);
        state.state
    }

    fn node_scope<F: FnOnce(&mut Self)>(&mut self, results: &mut FrameResults, state: &mut Node, indent: bool, f: F) -> NodeStateValue {
        let node_state = self.run_node_scope(results, state);
        if state.state.is_expanded() {
            if indent {
                let indent_size = self.style.as_ref().indent;
                self.layout.adjust_indent(indent_size);
                f(self);
                self.layout.adjust_indent(-indent_size);
            } else {
                f(self);
            }
        }
        node_state
    }

    /// Builds a collapsible header row that executes `f` when expanded.
    pub fn header<F: FnOnce(&mut Self)>(&mut self, results: &mut FrameResults, state: &mut Node, f: F) -> NodeStateValue {
        self.node_scope(results, state, false, f)
    }

    /// Builds a tree node with automatic indentation while expanded.
    pub fn treenode<F: FnOnce(&mut Self)>(&mut self, results: &mut FrameResults, state: &mut Node, f: F) -> NodeStateValue {
        self.node_scope(results, state, true, f)
    }

    fn run_tree_nodes(&mut self, results: &mut FrameResults, nodes: &[RuntimeTreeNode<'_>]) {
        for node in nodes {
            self.run_tree_node(results, node);
        }
    }

    fn pre_handle_tree_nodes(&mut self, nodes: &[RuntimeTreeNode<'_>]) {
        for node in nodes {
            self.pre_handle_tree_node(node);
        }
    }

    fn pre_handle_tree_node(&mut self, node: &RuntimeTreeNode<'_>) {
        match node.kind() {
            RuntimeTreeNodeKind::Header { state } | RuntimeTreeNodeKind::Tree { state } => {
                if self.cached_tree_click(node.id()) {
                    let mut state = state.borrow_mut();
                    state.state = if state.state.is_expanded() {
                        NodeStateValue::Closed
                    } else {
                        NodeStateValue::Expanded
                    };
                }
                if state.borrow().state.is_expanded() {
                    self.pre_handle_tree_nodes(node.children());
                }
            }
            RuntimeTreeNodeKind::Container { handle, .. } => {
                let mut handle = handle.clone();
                handle.with_mut(|container| {
                    container.pre_handle_tree_nodes(node.children());
                });
            }
            RuntimeTreeNodeKind::Row { .. } | RuntimeTreeNodeKind::Grid { .. } | RuntimeTreeNodeKind::Column | RuntimeTreeNodeKind::Stack { .. } => {
                self.pre_handle_tree_nodes(node.children())
            }
            RuntimeTreeNodeKind::Widget { .. } | RuntimeTreeNodeKind::CustomRender { .. } | RuntimeTreeNodeKind::Run { .. } => {}
        }
    }

    fn cached_tree_click(&mut self, node_id: NodeId) -> bool {
        let Some(cached) = self.tree_cache.prev(node_id).copied() else {
            return false;
        };

        self.mouse_over(cached.rect, self.in_hover_root) && self.input.borrow().mouse_pressed.is_left()
    }

    fn record_tree_node(&mut self, node_id: NodeId, state: NodeCacheEntry) {
        self.tree_cache.record(node_id, state);
    }

    fn record_tree_group_from_children(&mut self, node_id: NodeId, children: &[RuntimeTreeNode<'_>]) {
        let mut bounds: Option<Recti> = None;
        for child in children {
            if let Some(child_state) = self.tree_cache.current(child.id()) {
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
            self.record_tree_node(
                node_id,
                NodeCacheEntry {
                    rect,
                    body: rect,
                    content_size: vec2(rect.width, rect.height),
                    control: ControlState::default(),
                    result: ResourceState::NONE,
                },
            );
        }
    }

    fn handle_tree_widget(&mut self, results: &mut FrameResults, node_id: NodeId, widget: &dyn WidgetStateHandleDyn) {
        let rect = self.next_widget_rect_dyn(widget);
        let opt = widget.effective_widget_opt();
        let bopt = widget.effective_behaviour_opt();
        let input = if widget.needs_input_snapshot() { Some(self.snapshot_input()) } else { None };
        let (control, result) = self.run_widget_dyn(results, widget, rect, input, opt, bopt);
        self.record_tree_node(
            node_id,
            NodeCacheEntry {
                rect,
                body: rect,
                content_size: Vec2i::default(),
                control,
                result,
            },
        );
    }

    fn handle_tree_custom_render(&mut self, results: &mut FrameResults, node_id: NodeId, state: &WidgetHandle<Custom>, render: &TreeCustomRender) {
        let rect = self.next_widget_rect_handle(state);
        let (opt, bopt, needs_input) = {
            let state = state.borrow();
            (state.effective_widget_opt(), state.effective_behaviour_opt(), state.needs_input_snapshot())
        };
        let input = if needs_input { Some(self.snapshot_input()) } else { None };
        let (control, result) = self.run_widget_handle(results, state, rect, input, opt, bopt);

        let snapshot = self.snapshot_input();
        let input_ref = snapshot.as_ref();
        let mouse_event = self.input_to_mouse_event(&control, input_ref, rect);

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

        self.record_tree_node(
            node_id,
            NodeCacheEntry {
                rect,
                body: rect,
                content_size: Vec2i::default(),
                control,
                result,
            },
        );
    }

    fn run_tree_node_scope(&mut self, results: &mut FrameResults, node_id: NodeId, state: &WidgetHandle<Node>) -> NodeStateValue {
        self.layout.row(&[SizePolicy::Remainder(0)], SizePolicy::Auto);
        let rect = self.next_widget_rect_handle(state);
        let (opt, bopt, stable_state) = {
            let state = state.borrow();
            (*state.widget_opt(), *state.behaviour_opt(), state.state)
        };
        let (control, result) = self.run_widget_handle(results, state, rect, None, opt, bopt);

        if control.clicked {
            state.borrow_mut().state = stable_state;
        }

        self.record_tree_node(
            node_id,
            NodeCacheEntry {
                rect,
                body: rect,
                content_size: Vec2i::default(),
                control,
                result,
            },
        );
        stable_state
    }

    fn run_tree_node(&mut self, results: &mut FrameResults, node: &RuntimeTreeNode<'_>) {
        match node.kind() {
            RuntimeTreeNodeKind::Widget { widget } => {
                self.handle_tree_widget(results, node.id(), *widget);
            }
            RuntimeTreeNodeKind::CustomRender { state, render } => {
                self.handle_tree_custom_render(results, node.id(), state, render);
            }
            RuntimeTreeNodeKind::Run { run } => {
                (*run.borrow_mut())(self, results);
            }
            RuntimeTreeNodeKind::Container { handle, opt, behaviour } => {
                let mut handle = handle.clone();
                self.begin_panel(&mut handle, *opt, *behaviour);
                handle.with_mut(|container| {
                    container.run_tree_nodes(results, node.children());
                });
                self.end_panel(&mut handle);
                let (rect, body, content_size) = handle.with(|container| (container.rect(), container.body(), container.content_size()));
                self.record_tree_node(
                    node.id(),
                    NodeCacheEntry {
                        rect,
                        body,
                        content_size,
                        control: ControlState::default(),
                        result: ResourceState::NONE,
                    },
                );
            }
            RuntimeTreeNodeKind::Header { state } => {
                if self.run_tree_node_scope(results, node.id(), state).is_expanded() {
                    self.run_tree_nodes(results, node.children());
                }
            }
            RuntimeTreeNodeKind::Tree { state } => {
                if self.run_tree_node_scope(results, node.id(), state).is_expanded() {
                    let indent_size = self.style.as_ref().indent;
                    self.layout.adjust_indent(indent_size);
                    self.run_tree_nodes(results, node.children());
                    self.layout.adjust_indent(-indent_size);
                }
            }
            RuntimeTreeNodeKind::Row { widths, height } => {
                self.with_row(widths, *height, |container| {
                    container.run_tree_nodes(results, node.children());
                });
                self.record_tree_group_from_children(node.id(), node.children());
            }
            RuntimeTreeNodeKind::Grid { widths, heights } => {
                self.with_grid(widths, heights, |container| {
                    container.run_tree_nodes(results, node.children());
                });
                self.record_tree_group_from_children(node.id(), node.children());
            }
            RuntimeTreeNodeKind::Column => {
                self.column(|container| {
                    container.run_tree_nodes(results, node.children());
                });
                self.record_tree_group_from_children(node.id(), node.children());
            }
            RuntimeTreeNodeKind::Stack { width, height, direction } => {
                self.stack_with_width_direction(*width, *height, *direction, |container| {
                    container.run_tree_nodes(results, node.children());
                });
                self.record_tree_group_from_children(node.id(), node.children());
            }
        }
    }

    /// Evaluates a prebuilt widget tree using the current container layout.
    pub fn widget_tree(&mut self, results: &mut FrameResults, tree: &WidgetTree) {
        let runtime_roots = tree.runtime_roots();
        self.pre_handle_tree_nodes(&runtime_roots);
        self.run_tree_nodes(results, &runtime_roots);
    }

    /// Builds a widget tree and evaluates it immediately.
    #[track_caller]
    pub fn build_tree(&mut self, results: &mut FrameResults, f: impl FnOnce(&mut WidgetTreeBuilder)) {
        let location = std::panic::Location::caller();
        let mut hasher = std::collections::hash_map::DefaultHasher::new();
        location.file().hash(&mut hasher);
        location.line().hash(&mut hasher);
        location.column().hash(&mut hasher);
        let tree = WidgetTreeBuilder::build_with_seed(hasher.finish(), f);
        self.widget_tree(results, &tree);
    }

    /// Same as [`Container::build_tree`], but lets callers provide an explicit
    /// root key when the same call site builds multiple independent trees.
    ///
    /// The key is mixed with the caller location instead of replacing it so a
    /// reused logical key in unrelated call sites still lands in a distinct
    /// root namespace.
    #[track_caller]
    pub fn build_tree_with_key<K: Hash>(&mut self, key: K, results: &mut FrameResults, f: impl FnOnce(&mut WidgetTreeBuilder)) {
        let location = std::panic::Location::caller();
        let mut hasher = std::collections::hash_map::DefaultHasher::new();
        location.file().hash(&mut hasher);
        location.line().hash(&mut hasher);
        location.column().hash(&mut hasher);
        key.hash(&mut hasher);
        let tree = WidgetTreeBuilder::build_with_seed(hasher.finish(), f);
        self.widget_tree(results, &tree);
    }
}
