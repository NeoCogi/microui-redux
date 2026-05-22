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
//! Embedded retained panel lifecycle.

use super::*;
use crate::id::IdNamespace;

impl Container {
    /// Derives a stable child-container scope id from the parent scope and retained node id.
    pub(crate) fn panel_scope_id(&self, node_id: NodeId) -> Id {
        IdNamespace::PANEL_SCOPE.id([self.internal_id_seed.raw() as u64, node_id.raw() as u64])
    }

    /// Ends an embedded panel layout scope and stores its measured content size.
    fn pop_panel_container(container: &mut Container) {
        let layout_body = container.layout.current_body();
        let layout_max = container.layout.current_max();
        if let Some(lm) = layout_max {
            container.content_size = Dimensioni::new(lm.x - layout_body.x, lm.y - layout_body.y);
        }

        container.layout.pop_scope();
    }

    /// Allocates a panel rect in the parent and prepares the child container layout.
    fn begin_panel_layout_container(&mut self, container: &mut Container, scroll_behavior: ScrollBehavior, policy: Policy) {
        let rect = self.layout.next_with_policies(Dimensioni::default(), policy.width, policy.height);
        container.prepare();
        container.rect = rect;
        container.configure_container_body(rect, scroll_behavior);
    }

    /// Applies parent-driven state that must stay synchronized on every panel pass.
    fn apply_panel_base_state(&self, container: &mut Container, panel_scope: Id) {
        container.set_internal_id_seed(panel_scope);
        container.style = self.style.clone();
    }

    /// Applies previously measured geometry to the child container before update/paint.
    fn apply_panel_layout_state(&self, container: &mut Container, panel_scope: Id, scroll_behavior: ScrollBehavior, layout: NodeLayout) {
        self.apply_panel_base_state(container, panel_scope);
        container.rect = layout.rect;
        container.body = layout.body;
        container.content_size = layout.content_size;
        container.scroll.enabled = !scroll_behavior.is_no_scroll();
    }

    /// Starts the layout pass for an embedded retained panel.
    pub(crate) fn begin_panel_layout(
        &mut self,
        panel: &mut ContainerHandle,
        node_id: NodeId,
        _opt: ContainerOption,
        scroll_behavior: ScrollBehavior,
        policy: Policy,
    ) {
        let panel_scope = self.panel_scope_id(node_id);
        let container = &mut panel.inner_mut();
        self.apply_panel_base_state(container, panel_scope);
        self.begin_panel_layout_container(container, scroll_behavior, policy);
    }

    /// Ends the layout pass for an embedded retained panel.
    pub(crate) fn end_panel_layout(&mut self, panel: &mut ContainerHandle) {
        let container = &mut panel.inner_mut();
        Self::pop_panel_container(container);
    }

    /// Measures a panel in a scratch container so parent layout can reserve its final rectangle.
    pub(crate) fn measure_panel_layout(
        &mut self,
        panel: &ContainerHandle,
        node_id: NodeId,
        scroll_behavior: ScrollBehavior,
        policy: Policy,
        results: &FrameResults,
        children: &[WidgetTreeNode],
    ) -> NodeLayout {
        // Measurement uses a clone of panel state so probing child geometry does not mutate live
        // hover/focus/scroll state before the update pass.
        let mut scratch = panel.inner().measurement_scratch();
        scratch.measurement_mode = true;
        self.apply_panel_base_state(&mut scratch, self.panel_scope_id(node_id));
        self.begin_panel_layout_container(&mut scratch, scroll_behavior, policy);
        scratch.layout_tree_nodes(results, children);
        Self::pop_panel_container(&mut scratch);
        NodeLayout::new(scratch.rect(), scratch.body(), scratch.content_size())
    }

    /// Starts update traversal for an embedded retained panel.
    pub(crate) fn begin_panel_update(
        &mut self,
        panel: &mut ContainerHandle,
        node_id: NodeId,
        _opt: ContainerOption,
        scroll_behavior: ScrollBehavior,
        layout: NodeLayout,
    ) {
        let panel_id = self.retained_id_for_node(node_id);
        let panel_scope = self.panel_scope_id(node_id);
        if self.hit_test_rect(layout.rect, self.interaction.in_hover_root) {
            // The parent tracks which child panel should own hover routing on the next frame.
            self.interaction.set_next_hover_root_child(panel_id, layout.rect);
        }

        let container = &mut panel.inner_mut();
        self.apply_panel_layout_state(container, panel_scope, scroll_behavior, layout);

        container.interaction.in_hover_root = self.interaction.in_hover_root && self.interaction.hover_root_child == Some(panel_id);
        if self.interaction.pending_scroll.is_some() && container.interaction.in_hover_root {
            // Scroll deltas descend into the active child panel first.
            container.interaction.seed_pending_scroll(self.interaction.take_pending_scroll());
        }
        container.push_clip_rect(layout.body);
    }

    /// Ends update traversal for an embedded panel and bubbles unconsumed scroll back to the parent.
    pub(crate) fn end_panel_update(&mut self, panel: &mut ContainerHandle) {
        panel.inner_mut().pop_clip_rect();
        {
            let mut inner = panel.inner_mut();
            inner.update_active_scrollbars();
            inner.consume_pending_scroll();
            let pending = inner.interaction.take_pending_scroll();
            if self.interaction.pending_scroll.is_none() {
                // If the child did not consume the scroll, the parent may still apply it.
                self.interaction.seed_pending_scroll(pending);
            }
        }
    }

    /// Starts paint traversal for an embedded retained panel.
    pub(crate) fn begin_panel_paint(
        &mut self,
        panel: &mut ContainerHandle,
        node_id: NodeId,
        opt: ContainerOption,
        scroll_behavior: ScrollBehavior,
        layout: NodeLayout,
    ) {
        let panel_scope = self.panel_scope_id(node_id);
        let container = &mut panel.inner_mut();
        self.apply_panel_layout_state(container, panel_scope, scroll_behavior, layout);

        if !opt.has_no_frame() {
            // The parent draws the panel frame before the child command list is replayed.
            self.draw_frame(layout.rect, ControlColor::PanelBG);
        }

        container.paint_active_scrollbars();
        container.push_clip_rect(layout.body);
    }

    /// Ends paint traversal and records a command that replays the child panel in tree order.
    pub(crate) fn end_panel_paint(&mut self, panel: &mut ContainerHandle) {
        panel.inner_mut().pop_clip_rect();
        self.draw.push_command(Command::RetainedPanel { handle: panel.clone() });
        self.panels.push(panel.clone())
    }
}
