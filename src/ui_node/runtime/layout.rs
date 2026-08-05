//! Retained measurement, allocation, and child-content bounds.

use super::*;

impl UiRuntime {
    /// Measures one already-borrowed node through the authoritative private node path.
    pub(super) fn measure_node(&self, node: &Node, style: &Style, atlas: &crate::AtlasHandle, available: Dimensioni) -> Dimensioni {
        #[cfg(test)]
        self.bump_metric(|metrics| metrics.measures += 1);
        // Node::measure is the only place that adds frame geometry; containers receive the same
        // content-only measurement contract whether reached here or through Children.
        node.measure(style, atlas, available)
    }

    /// Lays out one already-borrowed node through direct widget/container dispatch.
    pub(in crate::ui_node) fn layout_node_ref(&mut self, node: &mut Node, style: &Style, atlas: &crate::AtlasHandle, rect: Recti) -> Dimensioni {
        #[cfg(test)]
        self.bump_metric(|metrics| metrics.layouts += 1);
        let framed = node_is_framed(node);
        // Query content preference at the offered slot before the parent-owned node policy chooses
        // the actual outer allocation.
        let preferred = self.measure_node(node, style, atlas, Dimensioni::new(rect.width.max(1), rect.height.max(1)));
        let policy = node.state.policy;
        let outer = Recti::new(
            rect.x,
            rect.y,
            policy.width.allocated_extent(rect.width),
            policy.height.allocated_extent(rect.height),
        );
        self.layout_node_outer_ref(node, style, atlas, framed, outer, preferred)
    }

    /// Lays out a node whose parent/root flow has already resolved its size policy.
    pub(super) fn layout_allocated_node_ref(&mut self, node: &mut Node, style: &Style, atlas: &crate::AtlasHandle, rect: Recti) -> Dimensioni {
        #[cfg(test)]
        self.bump_metric(|metrics| metrics.layouts += 1);
        let framed = node_is_framed(node);
        // Preserve the established measure/layout phase contract while keeping the resolved root
        // allocation authoritative.
        let preferred = self.measure_node(node, style, atlas, Dimensioni::new(rect.width.max(1), rect.height.max(1)));
        let outer = Recti::new(rect.x, rect.y, rect.width.max(0), rect.height.max(0));
        self.layout_node_outer_ref(node, style, atlas, framed, outer, preferred)
    }

    /// Applies frame/content geometry and delegates layout for one resolved outer allocation.
    fn layout_node_outer_ref(
        &mut self,
        node: &mut Node,
        style: &Style,
        atlas: &crate::AtlasHandle,
        framed: bool,
        outer: Recti,
        preferred: Dimensioni,
    ) -> Dimensioni {
        // Store every rectangle in node-local coordinates except the outer allocation, which stays
        // parent-local. Transform traversal later composes those two coordinate spaces once.
        let local_outer = Recti::new(0, 0, outer.width, outer.height);
        let frame_geometry = crate::ui_node::frame::frame_geometry(local_outer, framed, style);
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
            .intersect(&content)
            .unwrap_or_else(|| Recti::new(content.x, content.y, 0, 0));

        // Only visible branches that opt into propagation contribute descendant overflow to their
        // own content size. Gated children retain stale boxes without affecting active geometry.
        let propagate_child_overflow = node.state.layout.propagate_child_overflow;
        if is_branch && node_children_visible(node) && propagate_child_overflow {
            let content_rect = node.with_children(child_content_bounds_from_children).unwrap_or(content);
            let content_size = Dimensioni::new(
                (content_rect.x + content_rect.width).max(outer.width).max(0),
                (content_rect.y + content_rect.height).max(outer.height).max(0),
            );
            node.set_layout(node.state.layout.with_content_size(content_size));
        }
        Dimensioni::new(outer.width, outer.height)
    }
}

/// Returns the union of two rectangles.
fn union_rect(a: Recti, b: Recti) -> Recti {
    let min_x = a.x.min(b.x);
    let min_y = a.y.min(b.y);
    let max_x = (a.x + a.width).max(b.x + b.width);
    let max_y = (a.y + a.height).max(b.y + b.height);
    Recti::new(min_x, min_y, max_x - min_x, max_y - min_y)
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
    let mut bounds = None;
    for child in children.iter().filter(|child| node_is_visible(child)) {
        let child_rect = child_content_rect(child);
        bounds = Some(match bounds {
            Some(rect) => union_rect(rect, child_rect),
            None => child_rect,
        });
    }
    bounds
}
