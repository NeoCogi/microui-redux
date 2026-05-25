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
// ARE DISCLAIMED. IN NO EVENT SHALL THE COPYRIGHT HOLDER OR ITS CONTRIBUTORS BE
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
//! Retained scroll-area state and lifecycle.
//!
//! A `ScrollArea` owns the persistent state needed by a nested scrollable subtree: viewport
//! geometry, measured content size, scroll offset and scrollbar widgets, child focus/hover routing,
//! a retained tree cache, and the child draw command stream. It does not own root/window concerns
//! such as z-order, popup lifecycle, chrome, root registration, or global frame orchestration.

use std::{
    cell::RefCell,
    ops::{Deref, DerefMut},
    rc::Rc,
};

use super::*;
use crate::id::IdNamespace;

/// Retained state for one scrollable child subtree.
pub struct ScrollArea {
    /// Traversal state owned by this nested scrollable region.
    host: TraversalHost,
}

impl ScrollArea {
    /// Creates a retained scroll area with shared renderer/style/input handles.
    pub(crate) fn new(name: &str, atlas: AtlasHandle, style: Rc<Style>, input: Rc<RefCell<Input>>) -> Self {
        Self {
            host: TraversalHost::new(name, atlas, style, input),
        }
    }

    /// Replays this scroll area's command list into the renderer canvas.
    pub(crate) fn render<R: Renderer>(&mut self, canvas: &mut Canvas<R>) {
        self.host.render(canvas);
    }

    /// Publishes this scroll area's frame-local interaction and retained tree cache.
    fn finish_frame(&mut self) {
        self.host.finish();
    }

    /// Returns the immutable traversal host backing this scroll area.
    pub(crate) fn host(&self) -> &TraversalHost {
        &self.host
    }

    /// Applies parent-driven state that must stay synchronized on every scroll-area pass.
    fn apply_base_state(host: &mut TraversalHost, parent: &TraversalHost, scope: Id) {
        host.set_internal_id_seed(scope);
        host.style = parent.style.clone();
    }

    /// Allocates a viewport rect in the parent.
    fn allocate_layout_rect(parent: &mut TraversalHost, host: &mut TraversalHost, policy: Policy) -> Recti {
        let rect = parent.layout.next_with_policies(Dimensioni::default(), policy.width, policy.height);
        host.set_rect(rect);
        rect
    }

    /// Runs the layout pass for this scroll area's child subtree.
    pub(crate) fn layout_children(
        &mut self,
        parent: &mut TraversalHost,
        results: &FrameResults,
        resources: &WidgetTreeResources,
        node_id: NodeId,
        policy: Policy,
        scroll_behavior: ScrollBehavior,
        children: &[WidgetTreeNode],
    ) -> NodeLayout {
        let scope = parent.scroll_area_scope_id(node_id);
        Self::apply_base_state(&mut self.host, parent, scope);
        self.host.prepare();
        let rect = Self::allocate_layout_rect(parent, &mut self.host, policy);
        self.host
            .layout_body_until_scrollbars_stable(results, resources, rect, scroll_behavior, children)
    }

    /// Runs the update pass for this scroll area's child subtree.
    pub(crate) fn update_children(
        &mut self,
        parent: &mut TraversalHost,
        results: &mut FrameResults,
        resources: &WidgetTreeResources,
        node_id: NodeId,
        layout: NodeLayout,
        scroll_behavior: ScrollBehavior,
        children: &[WidgetTreeNode],
    ) {
        let area_id = parent.retained_id_for_node(node_id);
        let scope = parent.scroll_area_scope_id(node_id);
        if parent.hit_test_rect(layout.rect, parent.interaction.in_hover_root) {
            parent.interaction.set_next_hover_root_child(area_id, layout.rect);
        }

        self.host.interaction.in_hover_root = parent.interaction.in_hover_root && parent.interaction.hover_root_child == Some(area_id);
        if parent.interaction.pending_scroll.is_some() && self.host.interaction.in_hover_root {
            self.host.interaction.seed_pending_scroll(parent.interaction.take_pending_scroll());
        }

        Self::apply_base_state(&mut self.host, parent, scope);
        self.host.update_body_tree(results, resources, layout, scroll_behavior, children);
        let pending = self.host.interaction.take_pending_scroll();
        if parent.interaction.pending_scroll.is_none() {
            parent.interaction.seed_pending_scroll(pending);
        }
    }

    /// Runs the paint pass for this scroll area's child subtree.
    pub(crate) fn paint_children(
        &mut self,
        parent: &mut TraversalHost,
        resources: &WidgetTreeResources,
        node_id: NodeId,
        layout: NodeLayout,
        opt: ContainerOption,
        scroll_behavior: ScrollBehavior,
        children: &[WidgetTreeNode],
    ) {
        let scope = parent.scroll_area_scope_id(node_id);
        Self::apply_base_state(&mut self.host, parent, scope);

        if !opt.intersects(ContainerOption::NO_FRAME) {
            parent.draw_frame(layout.rect, ControlColor::PanelBG);
        }

        self.host.paint_body_tree(resources, layout, scroll_behavior, children);
        self.finish_frame();
    }
}

impl Deref for ScrollArea {
    type Target = TraversalHost;

    fn deref(&self) -> &Self::Target {
        &self.host
    }
}

impl DerefMut for ScrollArea {
    fn deref_mut(&mut self) -> &mut Self::Target {
        &mut self.host
    }
}

impl TraversalHost {
    /// Derives a stable child scroll-area scope id from the parent scope and retained node id.
    pub(crate) fn scroll_area_scope_id(&self, node_id: NodeId) -> Id {
        IdNamespace::SCROLL_AREA_SCOPE.id([self.internal_id_seed.raw() as u64, node_id.raw() as u64])
    }
}
