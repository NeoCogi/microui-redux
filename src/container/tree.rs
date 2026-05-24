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
//! The retained tree path is intentionally split into three passes:
//! 1. a layout pass that measures widgets, allocates rectangles, and records geometry into the
//!    per-frame tree cache;
//! 2. an update pass that reuses the cached rectangles, samples interaction, mutates retained
//!    widget state, and stores the current `ControlState` for the same tree nodes;
//! 3. a paint pass that replays the same tree shape and records draw commands from the already
//!    updated widget state.
//!
//! Keeping those passes separate lets the retained path reason about geometry deterministically:
//! widgets do not advance layout while updating or painting, containers can recurse into children
//! using already-computed rectangles, and tests can inspect the cache after each phase.

use super::*;

struct RetainedCustomRenderCommand {
    render: TreeCustomRender,
}

impl CustomRenderCommand for RetainedCustomRenderCommand {
    fn render(&mut self, dim: Dimensioni, args: &CustomRenderArgs) {
        self.render.borrow_mut().render(dim, args);
    }
}

enum TreePass<'a> {
    Layout(&'a FrameResults),
    Update(&'a mut FrameResults),
    Paint,
}

impl TraversalHost {
    fn widget_dispatch_site(&self, node_id: NodeId, kind: &str) -> String {
        format!("container {:?}, tree node {:?} ({})", self.name, node_id, kind)
    }

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
    pub(crate) fn record_tree_layout(&mut self, node_id: NodeId, layout: NodeLayout) {
        self.tree_cache.record_layout(node_id, layout);
    }

    /// Stores the current frame control snapshot for a retained tree node.
    pub(crate) fn record_tree_control(&mut self, node_id: NodeId, control: ControlState) {
        self.tree_cache.record_control(node_id, control);
    }

    /// Returns the current frame layout for `node_id` or panics if layout was skipped.
    fn current_tree_layout_or_panic(&self, node_id: NodeId) -> NodeLayout {
        self.tree_cache
            .current_layout(node_id)
            .copied()
            .unwrap_or_else(|| panic!("tree node {:?} missing current layout", node_id))
    }

    /// Returns the current frame control state for `node_id` or panics if update was skipped.
    fn current_tree_control_or_panic(&self, node_id: NodeId) -> ControlState {
        self.tree_cache
            .current_control(node_id)
            .copied()
            .unwrap_or_else(|| panic!("tree node {:?} missing current control state", node_id))
    }

    /// Returns whether layout included this node's children in the current frame.
    fn tree_children_were_laid_out(&self, children: &[WidgetTreeNode]) -> bool {
        children.first().is_some_and(|child| self.tree_cache.current_layout(child.id()).is_some())
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
            self.record_tree_layout(node_id, NodeLayout::new(rect, rect, Dimensioni::new(rect.width, rect.height)));
        }
    }

    /// Runs the retained layout pass for a slice of sibling tree nodes.
    pub(crate) fn layout_tree_nodes(&mut self, results: &FrameResults, nodes: &[WidgetTreeNode]) {
        let mut pass = TreePass::Layout(results);
        self.visit_tree_nodes(&mut pass, nodes);
    }

    /// Runs the retained widget update pass for a slice of sibling tree nodes.
    pub(super) fn update_tree_nodes(&mut self, results: &mut FrameResults, nodes: &[WidgetTreeNode]) {
        let mut pass = TreePass::Update(results);
        self.visit_tree_nodes(&mut pass, nodes);
    }

    /// Runs the retained widget paint pass for a slice of sibling tree nodes.
    pub(super) fn paint_tree_nodes(&mut self, nodes: &[WidgetTreeNode]) {
        let mut pass = TreePass::Paint;
        self.visit_tree_nodes(&mut pass, nodes);
    }

    fn visit_tree_nodes(&mut self, pass: &mut TreePass<'_>, nodes: &[WidgetTreeNode]) {
        for node in nodes {
            self.visit_tree_node(pass, node);
        }
    }

    fn visit_tree_node(&mut self, pass: &mut TreePass<'_>, node: &WidgetTreeNode) {
        let (node_id, kind, children) = node.parts();
        let policy = node.policy();
        match kind {
            WidgetTreeNodeKind::Widget { widget } => self.visit_tree_widget(pass, node_id, policy, &**widget),
            WidgetTreeNodeKind::CustomRender { state, render } => self.visit_tree_custom_render(pass, node_id, policy, state, render),
            WidgetTreeNodeKind::ScrollArea { handle, opt, scroll_behavior } => {
                self.visit_tree_scroll_area(pass, node_id, policy, handle, *opt, *scroll_behavior, children);
            }
            WidgetTreeNodeKind::Header { state } => self.visit_tree_scope(pass, node_id, policy, state, children, false),
            WidgetTreeNodeKind::Tree { state } => self.visit_tree_scope(pass, node_id, policy, state, children, true),
            WidgetTreeNodeKind::Row { widths, height } => self.visit_tree_row(pass, node_id, policy, children, widths, *height),
            WidgetTreeNodeKind::Grid { widths, heights } => self.visit_tree_grid(pass, node_id, policy, children, widths, heights),
            WidgetTreeNodeKind::Column => self.visit_tree_column(pass, node_id, policy, children),
            WidgetTreeNodeKind::Stack { width, height, direction } => self.visit_tree_stack(pass, node_id, policy, children, *width, *height, *direction),
        }
    }

    /// Measures a leaf widget node and records its allocated rectangle in the tree cache.
    fn layout_tree_widget(&mut self, node_id: NodeId, policy: Policy, widget: &dyn WidgetStateHandleDyn) {
        // Measurement goes through the same generic widget dispatch used elsewhere in the
        // container. The retained path just captures the allocated rectangle in the tree cache
        // instead of consuming it immediately.
        let rect = self.measure_widget_rect_dyn_with_policy(widget, policy);
        self.record_tree_layout(node_id, NodeLayout::new(rect, rect, Dimensioni::default()));
    }

    /// Updates a leaf widget node using the rectangle captured during the layout pass.
    fn update_tree_widget(&mut self, results: &mut FrameResults, node_id: NodeId, widget: &dyn WidgetStateHandleDyn) {
        // Update must reuse the rect produced during layout. If a node reaches this pass
        // without a cached layout entry, the retained traversal is internally inconsistent.
        let rect = self.current_tree_layout_or_panic(node_id).rect;
        let opt = widget.effective_widget_opt();
        let scroll_behavior = widget.effective_scroll_behavior();
        let focus_policy = widget.focus_policy();
        let input = if widget.needs_input_snapshot() { Some(self.snapshot_input()) } else { None };
        let dispatch_site = self.widget_dispatch_site(node_id, "widget");
        let (control, _result) = self.update_node_dyn(results, node_id, widget, rect, input, opt, scroll_behavior, focus_policy, dispatch_site);
        self.record_tree_control(node_id, control);
    }

    /// Paints a leaf widget node from the state produced by the update pass.
    fn paint_tree_widget(&mut self, node_id: NodeId, widget: &dyn WidgetStateHandleDyn) {
        let rect = self.current_tree_layout_or_panic(node_id).rect;
        let control = self.current_tree_control_or_panic(node_id);
        let input = if widget.needs_input_snapshot() { Some(self.snapshot_input()) } else { None };
        self.paint_node_dyn(node_id, widget, rect, input, &control);
    }

    /// Measures a retained custom-render node and records its allocated rectangle.
    fn layout_tree_custom_render(&mut self, node_id: NodeId, policy: Policy, state: &WidgetHandle<Custom>) {
        let widget = erased_widget_state(state.clone());
        let rect = self.measure_widget_rect_dyn_with_policy(&*widget, policy);
        self.record_tree_layout(node_id, NodeLayout::new(rect, rect, Dimensioni::default()));
    }

    /// Updates a retained custom-render node and records its interaction payload.
    fn update_tree_custom_render(&mut self, results: &mut FrameResults, node_id: NodeId, state: &WidgetHandle<Custom>) {
        let rect = self.current_tree_layout_or_panic(node_id).rect;
        let widget = erased_widget_state(state.clone());
        let opt = widget.effective_widget_opt();
        let scroll_behavior = widget.effective_scroll_behavior();
        let focus_policy = widget.focus_policy();
        let needs_input = widget.needs_input_snapshot();
        let input = if needs_input { Some(self.snapshot_input()) } else { None };
        let dispatch_site = self.widget_dispatch_site(node_id, "custom render");
        let (control, _result) = self.update_node_dyn(results, node_id, &*widget, rect, input, opt, scroll_behavior, focus_policy, dispatch_site);
        self.record_tree_control(node_id, control);
    }

    /// Paints a retained custom-render node and records its backend callback payload.
    fn paint_tree_custom_render(&mut self, node_id: NodeId, state: &WidgetHandle<Custom>, render: &TreeCustomRender) {
        let rect = self.current_tree_layout_or_panic(node_id).rect;
        let control = self.current_tree_control_or_panic(node_id);
        let widget = erased_widget_state(state.clone());
        let opt = widget.effective_widget_opt();
        let scroll_behavior = widget.effective_scroll_behavior();
        let needs_input = widget.needs_input_snapshot();
        let input = if needs_input { Some(self.snapshot_input()) } else { None };
        self.paint_node_dyn(node_id, &*widget, rect, input, &control);

        let snapshot = self.snapshot_input();
        let input_ref = snapshot.as_ref();
        let mouse_event = self.input_to_mouse_event(&control, input_ref, rect);

        // Custom render callbacks are fed the same normalized interaction payload that built-in
        // widgets observe. This keeps the custom path aligned with the retained widget contract:
        // geometry comes from the retained pass, input is localized to the widget rect, and the
        // backend-specific drawing work is deferred through the command list.
        let active = control.focused && self.interaction.in_hover_root;
        let key_mods = if active { input_ref.key_mods } else { KeyMode::NONE };
        let key_codes = if active { input_ref.key_codes } else { KeyCode::NONE };
        let text_input = if active { input_ref.text_input.clone() } else { String::new() };
        let view = self.get_clip_rect().intersect(&rect).unwrap_or_else(|| Recti::new(rect.x, rect.y, 0, 0));
        let cra = CustomRenderArgs {
            content_area: rect,
            view,
            mouse_event,
            scroll_delta: control.scroll_delta,
            widget_opt: opt,
            scroll_behavior,
            key_mods,
            key_codes,
            text_input,
        };
        let render = render.clone();
        self.draw
            .push_command(Command::BackendCustomRender(cra, Box::new(RetainedCustomRenderCommand { render })));
    }

    /// Measures a header/tree disclosure node and returns the stable expansion state used this frame.
    fn layout_tree_node_scope(&mut self, node_id: NodeId, policy: Policy, state: &WidgetHandle<Node>) -> NodeStateValue {
        // Header/tree nodes always reserve a full-width row for the disclosure widget itself. The
        // returned stable state decides whether children participate in the current layout pass.
        self.layout.row(&[SizePolicy::Remainder(0)], SizePolicy::Auto);
        let widget = erased_widget_state(state.clone());
        let rect = self.measure_widget_rect_dyn_with_policy(&*widget, policy);
        let stable_state = state.read(|state| state.state);
        self.record_tree_layout(node_id, NodeLayout::new(rect, rect, Dimensioni::default()));
        stable_state
    }

    fn layout_policy_group<F: FnOnce(&mut Self)>(&mut self, node_id: NodeId, policy: Policy, children: &[WidgetTreeNode], f: F) {
        if policy == Policy::auto() {
            f(self);
            self.record_tree_group_from_children(node_id, children);
            return;
        }

        let rect = self.layout.begin_node_scope_with_policies(Dimensioni::default(), policy.width, policy.height);
        f(self);
        let content_size = self.layout.end_node_scope();
        self.record_tree_layout(node_id, NodeLayout::new(rect, rect, content_size));
    }

    fn layout_tree_node_scope_children(
        &mut self,
        results: &FrameResults,
        node_id: NodeId,
        policy: Policy,
        state: &WidgetHandle<Node>,
        children: &[WidgetTreeNode],
        indent_children: bool,
    ) {
        if !self.layout_tree_node_scope(node_id, policy, state).is_expanded() {
            return;
        }

        if indent_children {
            let indent_size = self.style.as_ref().indent;
            self.layout.adjust_indent(indent_size);
            self.layout_tree_nodes(results, children);
            self.layout.adjust_indent(-indent_size);
        } else {
            self.layout_tree_nodes(results, children);
        }
    }

    /// Updates a header/tree disclosure node and returns the layout-time expansion state.
    fn update_tree_node_scope(&mut self, results: &mut FrameResults, node_id: NodeId, state: &WidgetHandle<Node>) -> NodeStateValue {
        let rect = self.current_tree_layout_or_panic(node_id).rect;
        let stable_state = state.read(|state| state.state);
        let widget = erased_widget_state(state.clone());
        let opt = widget.effective_widget_opt();
        let scroll_behavior = widget.effective_scroll_behavior();
        let focus_policy = widget.focus_policy();
        let dispatch_site = self.widget_dispatch_site(node_id, "node disclosure");
        let (control, _result) = self.update_node_dyn(results, node_id, &*widget, rect, None, opt, scroll_behavior, focus_policy, dispatch_site);
        self.record_tree_control(node_id, control);
        stable_state
    }

    fn update_tree_node_scope_children(&mut self, results: &mut FrameResults, node_id: NodeId, state: &WidgetHandle<Node>, children: &[WidgetTreeNode]) {
        if self.update_tree_node_scope(results, node_id, state).is_expanded() {
            self.update_tree_nodes(results, children);
        }
    }

    /// Paints a header/tree disclosure node.
    fn paint_tree_node_scope(&mut self, node_id: NodeId, state: &WidgetHandle<Node>) {
        let rect = self.current_tree_layout_or_panic(node_id).rect;
        let control = self.current_tree_control_or_panic(node_id);
        let widget = erased_widget_state(state.clone());
        self.paint_node_dyn(node_id, &*widget, rect, None, &control);
    }

    fn paint_tree_node_scope_children(&mut self, node_id: NodeId, state: &WidgetHandle<Node>, children: &[WidgetTreeNode]) {
        self.paint_tree_node_scope(node_id, state);
        if self.tree_children_were_laid_out(children) {
            self.paint_tree_nodes(children);
        }
    }

    fn update_structural_tree_node(&mut self, results: &mut FrameResults, children: &[WidgetTreeNode]) {
        self.update_tree_nodes(results, children);
    }

    fn paint_structural_tree_node(&mut self, children: &[WidgetTreeNode]) {
        self.paint_tree_nodes(children);
    }

    fn visit_tree_widget(&mut self, pass: &mut TreePass<'_>, node_id: NodeId, policy: Policy, widget: &dyn WidgetStateHandleDyn) {
        match pass {
            TreePass::Layout(_) => self.layout_tree_widget(node_id, policy, widget),
            TreePass::Update(results) => self.update_tree_widget(&mut **results, node_id, widget),
            TreePass::Paint => self.paint_tree_widget(node_id, widget),
        }
    }

    fn visit_tree_custom_render(&mut self, pass: &mut TreePass<'_>, node_id: NodeId, policy: Policy, state: &WidgetHandle<Custom>, render: &TreeCustomRender) {
        match pass {
            TreePass::Layout(_) => self.layout_tree_custom_render(node_id, policy, state),
            TreePass::Update(results) => self.update_tree_custom_render(&mut **results, node_id, state),
            TreePass::Paint => self.paint_tree_custom_render(node_id, state, render),
        }
    }

    fn visit_tree_scroll_area(
        &mut self,
        pass: &mut TreePass<'_>,
        node_id: NodeId,
        policy: Policy,
        handle: &ScrollAreaHandle,
        opt: ContainerOption,
        scroll_behavior: ScrollBehavior,
        children: &[WidgetTreeNode],
    ) {
        match pass {
            TreePass::Layout(results) => {
                let handle = handle.clone();
                let layout = handle.with_inner_mut(|area| area.layout_children(self, *results, node_id, policy, scroll_behavior, children));
                self.record_tree_layout(node_id, layout);
            }
            TreePass::Update(results) => {
                let handle = handle.clone();
                let layout = self.current_tree_layout_or_panic(node_id);
                handle.with_inner_mut(|area| {
                    area.update_children(self, &mut **results, node_id, layout, scroll_behavior, children);
                });
            }
            TreePass::Paint => {
                let handle = handle.clone();
                let layout = self.current_tree_layout_or_panic(node_id);
                handle.with_inner_mut(|area| {
                    area.paint_children(self, node_id, layout, opt, scroll_behavior, children);
                });
                self.draw.push_command(Command::RetainedScrollArea { handle });
            }
        }
    }

    fn visit_tree_scope(
        &mut self,
        pass: &mut TreePass<'_>,
        node_id: NodeId,
        policy: Policy,
        state: &WidgetHandle<Node>,
        children: &[WidgetTreeNode],
        indent_children: bool,
    ) {
        match pass {
            TreePass::Layout(results) => {
                // Headers gate child participation entirely. In the strict retained model the
                // current stable state decides whether descendants exist for this frame.
                self.layout_tree_node_scope_children(*results, node_id, policy, state, children, indent_children);
            }
            TreePass::Update(results) => self.update_tree_node_scope_children(&mut **results, node_id, state, children),
            TreePass::Paint => self.paint_tree_node_scope_children(node_id, state, children),
        }
    }

    fn visit_tree_row(
        &mut self,
        pass: &mut TreePass<'_>,
        node_id: NodeId,
        policy: Policy,
        children: &[WidgetTreeNode],
        widths: &[SizePolicy],
        height: SizePolicy,
    ) {
        match pass {
            TreePass::Layout(results) => {
                let results = *results;
                self.layout_policy_group(node_id, policy, children, |container| {
                    container.with_row(widths, height, |container| {
                        container.layout_tree_nodes(results, children);
                    });
                });
            }
            TreePass::Update(results) => self.update_structural_tree_node(&mut **results, children),
            TreePass::Paint => self.paint_structural_tree_node(children),
        }
    }

    fn visit_tree_grid(
        &mut self,
        pass: &mut TreePass<'_>,
        node_id: NodeId,
        policy: Policy,
        children: &[WidgetTreeNode],
        widths: &[SizePolicy],
        heights: &[SizePolicy],
    ) {
        match pass {
            TreePass::Layout(results) => {
                let results = *results;
                self.layout_policy_group(node_id, policy, children, |container| {
                    container.with_grid(widths, heights, |container| {
                        container.layout_tree_nodes(results, children);
                    });
                });
            }
            TreePass::Update(results) => self.update_structural_tree_node(&mut **results, children),
            TreePass::Paint => self.paint_structural_tree_node(children),
        }
    }

    fn visit_tree_column(&mut self, pass: &mut TreePass<'_>, node_id: NodeId, policy: Policy, children: &[WidgetTreeNode]) {
        match pass {
            TreePass::Layout(results) => {
                let results = *results;
                if policy == Policy::auto() {
                    self.column(|container| {
                        container.layout_tree_nodes(results, children);
                    });
                    self.record_tree_group_from_children(node_id, children);
                } else {
                    let rect = self.layout.begin_node_scope_with_policies(Dimensioni::default(), policy.width, policy.height);
                    self.layout_tree_nodes(results, children);
                    let content_size = self.layout.end_node_scope();
                    self.record_tree_layout(node_id, NodeLayout::new(rect, rect, content_size));
                }
            }
            TreePass::Update(results) => self.update_structural_tree_node(&mut **results, children),
            TreePass::Paint => self.paint_structural_tree_node(children),
        }
    }

    fn visit_tree_stack(
        &mut self,
        pass: &mut TreePass<'_>,
        node_id: NodeId,
        policy: Policy,
        children: &[WidgetTreeNode],
        width: SizePolicy,
        height: SizePolicy,
        direction: StackDirection,
    ) {
        match pass {
            TreePass::Layout(results) => {
                let results = *results;
                self.layout_policy_group(node_id, policy, children, |container| {
                    container.stack_with_width_direction(width, height, direction, |container| {
                        container.layout_tree_nodes(results, children);
                    });
                });
            }
            TreePass::Update(results) => self.update_structural_tree_node(&mut **results, children),
            TreePass::Paint => self.paint_structural_tree_node(children),
        }
    }

    /// Evaluates a prebuilt widget tree using the current container layout.
    #[cfg(test)]
    pub(crate) fn widget_tree(&mut self, results: &mut FrameResults, tree: &WidgetTree) {
        // Layout must always happen before update and paint so the cache contains every rect the
        // later passes reuse. Update then walks the whole tree before paint records any commands.
        self.layout_tree_nodes(results, tree.roots());
        self.update_tree_nodes(results, tree.roots());
        self.paint_tree_nodes(tree.roots());
    }

    /// Measures a prebuilt widget tree using the current container layout without rendering it.
    pub(crate) fn measure_widget_tree_content(&mut self, results: &FrameResults, tree: &WidgetTree) -> Dimensioni {
        let mut scratch = self.measurement_scratch();
        scratch.layout_tree_nodes(results, tree.roots());

        match scratch.layout.current_max() {
            Some(max_rect) => {
                let body = scratch.layout.current_body();
                Dimensioni::new(max_rect.x - body.x, max_rect.y - body.y)
            }
            None => Dimensioni::default(),
        }
    }
}
