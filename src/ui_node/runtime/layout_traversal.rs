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

//! Retained-tree measurement, allocation traversal, and child-content bounds.

use super::*;
use crate::math::RectExt;

impl UiRuntime {
    /// Measures one already-borrowed node through the authoritative private node path.
    pub(in crate::ui_node) fn measure_node(&self, node: &mut Node, style: &Skin, atlas: &crate::AtlasHandle, constraints: Constraints) -> Dimensioni {
        #[cfg(test)]
        self.bump_metric(|metrics| metrics.measures += 1);
        // Node::measure is the only place that adds frame geometry; containers receive the same
        // content-only measurement contract whether reached here or through Children.
        node.measure(style, atlas, constraints)
    }

    fn measure_node_for_layout(&self, node: &mut Node, style: &Skin, atlas: &crate::AtlasHandle, constraints: Constraints) -> (Dimensioni, bool) {
        #[cfg(test)]
        self.bump_metric(|metrics| metrics.measures += 1);
        node.measure_with_cache_status(style, atlas, constraints)
    }

    /// Assigns one exact parent-owned rectangle through direct widget/container dispatch.
    pub(in crate::ui_node) fn layout_node_ref(&mut self, node: &mut Node, style: &Skin, atlas: &crate::AtlasHandle, rect: Recti) -> Dimensioni {
        let frame_role = node_frame_role(node);
        // Preserve the measure/layout phase contract while keeping the parent's rectangle
        // authoritative. Sizing relationships live in the parent container, never on Node.
        let (preferred, measurement_cached) = self.measure_node_for_layout(node, style, atlas, Constraints::bounded(Dimensioni::new(rect.width, rect.height)));
        let outer = Recti::new(rect.x, rect.y, rect.width.max(0), rect.height.max(0));
        self.layout_node_outer_ref(node, style, atlas, frame_role, outer, preferred, measurement_cached)
    }

    /// Applies frame/content geometry and delegates layout for one resolved outer allocation.
    fn layout_node_outer_ref(
        &mut self,
        node: &mut Node,
        style: &Skin,
        atlas: &crate::AtlasHandle,
        frame_role: Option<crate::FrameRole>,
        outer: Recti,
        preferred: Dimensioni,
        measurement_cached: bool,
    ) -> Dimensioni {
        let previous = node.state.layout.allocation;
        let same_allocation = previous.x == outer.x && previous.y == outer.y && previous.width == outer.width && previous.height == outer.height;
        if measurement_cached && same_allocation && !node.state.layout_is_dirty() {
            return Dimensioni::new(outer.width, outer.height);
        }
        #[cfg(test)]
        self.bump_metric(|metrics| metrics.layouts += 1);
        // Store every rectangle in node-local coordinates except the outer allocation, which stays
        // parent-local. Transform traversal later composes those two coordinate spaces once.
        let local_outer = Recti::new(0, 0, outer.width, outer.height);
        let frame_geometry = crate::ui_node::frame::frame_geometry(local_outer, frame_role, style);
        let content = frame_geometry.content_or_empty();
        let is_branch = node.is_container();
        node.set_layout(NodeLayout::from_parts(outer, content, Dimensioni::new(outer.width.max(0), outer.height.max(0))));

        // Leaves expose overflow when their preference exceeds allocation. Containers instead
        // author their child viewport and content extent through the scoped layout context.
        match &mut node.data {
            NodeKind::Widget(_) => {
                let content_size = Dimensioni::new(outer.width.max(preferred.width).max(0), outer.height.max(preferred.height).max(0));
                node.state.set_layout(node.state.layout.with_content_size(content_size));
            }
            NodeKind::Container(container) => {
                let mut ctx = ContainerLayoutCtx::new(self, style, atlas, content, &mut node.state);
                container.place(&mut ctx, content);
            }
        }

        // A container may narrow its child clip, but it cannot expand beyond framed content.
        node.state.layout.allocation = outer;
        node.state.layout.children.clip = node
            .state
            .layout
            .children
            .clip
            .positive_intersection(content)
            .unwrap_or_else(|| Recti::new(content.x, content.y, 0, 0));

        // Only visible branches that opt into propagation contribute descendant overflow to their
        // own content size. Gated children retain stale boxes without affecting active geometry.
        let propagate_child_overflow = node.state.layout.propagate_child_overflow;
        if is_branch && node_children_visible(node) && propagate_child_overflow {
            let content_rect = node.with_children(child_content_bounds_from_children).unwrap_or(content);
            let content_size = Dimensioni::new(
                content_rect.x.saturating_add(content_rect.width).max(outer.width).max(0),
                content_rect.y.saturating_add(content_rect.height).max(outer.height).max(0),
            );
            node.set_layout(node.state.layout.with_content_size(content_size));
        }
        node.state.validate_layout();
        Dimensioni::new(outer.width, outer.height)
    }
}

/// Returns the rectangle occupied by a child and any overflow content it measured.
fn child_content_rect(node: &Node) -> Recti {
    let allocation = node.state.layout.allocation;
    let content_size = node.state.layout.content_size;
    Recti::new(
        allocation.x,
        allocation.y,
        allocation.width.max(content_size.width),
        allocation.height.max(content_size.height),
    )
}

fn child_content_bounds_from_children(children: &Children) -> Option<Recti> {
    // Hidden children keep their last allocation for state continuity but cannot enlarge active
    // content bounds. Visible child overflow is folded without allocating a temporary collection.
    let mut bounds: Option<Recti> = None;
    for child in children.iter().filter(|child| node_is_visible(child)) {
        let child_rect = child_content_rect(child);
        bounds = Some(match bounds {
            Some(rect) => rect.union(child_rect),
            None => child_rect,
        });
    }
    bounds
}
