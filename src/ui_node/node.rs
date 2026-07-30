use crate::{Dimensioni, Id, Recti, Vec2i};

use super::{Container, NodeBehavior};

/// Stable runtime node identifier.
pub(crate) type UiNodeId = Id;

/// Persistent layout result for one runtime node.
#[derive(Copy, Clone, Debug)]
pub(crate) struct NodeLayout {
    /// Allocation assigned by the parent, in the parent's content coordinate space.
    pub(crate) allocation: Recti,
    /// Node-local transform and viewport exposed to children.
    pub(crate) children: ChildLayout,
    /// Measured/assigned virtual content size in content coordinates.
    pub(crate) content_size: Dimensioni,
    /// Whether child overflow contributes to this node's parent-visible content size.
    pub(crate) propagate_child_overflow: bool,
}

impl Default for NodeLayout {
    fn default() -> Self {
        Self {
            allocation: Recti::default(),
            children: ChildLayout::default(),
            content_size: Dimensioni::default(),
            propagate_child_overflow: true,
        }
    }
}

impl NodeLayout {
    /// Builds a simple non-scrolled layout.
    pub(crate) fn from_rect(rect: Recti, content_size: Dimensioni) -> Self {
        Self::from_parts(rect, Recti::new(0, 0, rect.width.max(0), rect.height.max(0)), content_size)
    }

    /// Builds a layout from an outer allocation and node-local child viewport.
    pub(crate) fn from_parts(allocation: Recti, child_clip: Recti, content_size: Dimensioni) -> Self {
        Self {
            allocation,
            children: ChildLayout::new(child_clip),
            content_size,
            propagate_child_overflow: true,
        }
    }

    /// Returns this layout with an updated content size.
    pub(crate) fn with_content_size(mut self, content_size: Dimensioni) -> Self {
        self.content_size = content_size;
        self
    }

    /// Returns this layout with updated child-overflow propagation.
    pub(crate) fn with_child_overflow_propagation(mut self, propagate_child_overflow: bool) -> Self {
        self.propagate_child_overflow = propagate_child_overflow;
        self
    }
}

/// Node-local transform and viewport exposed to child nodes.
#[derive(Copy, Clone, Debug, Default)]
pub(crate) struct ChildLayout {
    /// Translation from child content coordinates into this node's local coordinates.
    pub(crate) offset: Vec2i,
    /// Visible child viewport in this node's local coordinates.
    pub(crate) clip: Recti,
}

impl ChildLayout {
    /// Builds child-layout data from a node-local viewport.
    pub(crate) fn new(clip: Recti) -> Self {
        Self { offset: Vec2i::default(), clip }
    }
}

/// Stack-only transform derived while walking the node tree.
#[derive(Copy, Clone, Debug)]
pub(crate) struct Transform {
    /// Translation from the current content coordinate space to screen coordinates.
    pub(crate) offset: Vec2i,
    /// Inherited effective clip in screen coordinates.
    pub(crate) clip: Recti,
}

impl Transform {
    /// Creates a root transform.
    pub(crate) fn root(screen_clip: Recti) -> Self {
        Self {
            offset: Vec2i::default(),
            clip: screen_clip,
        }
    }

    /// Creates a root transform with a screen-space origin.
    pub(crate) fn root_at(origin: Vec2i, screen_clip: Recti) -> Self {
        Self { offset: origin, clip: screen_clip }
    }

    /// Pushes a node's child coordinate system onto the transform stack.
    pub(crate) fn push(self, layout: NodeLayout) -> Self {
        let node_origin = self.offset + Vec2i::new(layout.allocation.x, layout.allocation.y);
        let screen_clip = translate_rect(layout.children.clip, node_origin);
        Self {
            offset: node_origin + layout.children.offset,
            clip: self.clip.intersect(&screen_clip).unwrap_or_default(),
        }
    }

    /// Resolves a parent-local allocation into screen coordinates.
    pub(crate) fn resolve(self, allocation: Recti) -> Recti {
        translate_rect(allocation, self.offset)
    }
}

fn translate_rect(rect: Recti, offset: Vec2i) -> Recti {
    Recti::new(rect.x + offset.x, rect.y + offset.y, rect.width, rect.height)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn pushing_a_node_uses_node_local_child_geometry() {
        let mut layout = NodeLayout::from_parts(Recti::new(10, 20, 30, 40), Recti::new(2, 3, 20, 10), Dimensioni::new(30, 40));
        layout.children.offset = Vec2i::new(4, -5);

        let parent = Transform::root_at(Vec2i::new(100, 200), Recti::new(0, 0, 1000, 1000));
        let child = parent.push(layout);

        assert_eq!((child.offset.x, child.offset.y), (114, 215));
        assert_eq!((child.clip.x, child.clip.y, child.clip.width, child.clip.height), (112, 223, 20, 10));
    }
}

/// Common runtime node state shared by widgets and containers.
pub(crate) struct UiNodeState {
    /// Stable runtime node id.
    id: UiNodeId,
    /// Persistent layout result for traversal.
    pub(crate) layout: NodeLayout,
    /// Whether this node participates in traversal.
    pub(crate) visible: bool,
    /// Cursor is hovering this node.
    pub(crate) hovered: bool,
    /// This node currently owns focus.
    pub(crate) focused: bool,
    /// Mouse was pressed on this node during the current frame.
    pub(crate) clicked: bool,
    /// Mouse is held down while this node owns focus.
    pub(crate) active: bool,
    /// Scroll delta consumed by this node during the current frame.
    pub(crate) scroll_delta: Option<Vec2i>,
    /// Placement policy used by runtime layout passes.
    pub(crate) policy: crate::Policy,
}

impl UiNodeState {
    /// Returns this node state's stable identity.
    pub const fn id(&self) -> UiNodeId {
        self.id
    }

    /// Writes layout as the source of truth.
    pub(crate) fn set_layout(&mut self, layout: NodeLayout) {
        self.layout = layout;
    }

    /// Writes a simple non-scrolled layout.
    pub(crate) fn set_layout_from_rect(&mut self, rect: Recti, content_size: Dimensioni) {
        self.set_layout(NodeLayout::from_rect(rect, content_size));
    }
}

/// Runtime node that owns shared state plus widget/container payload.
pub(crate) struct UiNode {
    /// Common state for layout, identity, and interaction.
    pub(crate) state: UiNodeState,
    /// Node-specific payload.
    pub(crate) data: UiNodeData,
}

impl UiNode {
    /// Returns this node's stable identity.
    pub const fn id(&self) -> UiNodeId {
        self.state.id()
    }

    /// Returns this node's shared runtime state.
    pub(crate) fn state(&self) -> &UiNodeState {
        &self.state
    }

    /// Returns this node's shared runtime state mutably.
    pub(crate) fn state_mut(&mut self) -> &mut UiNodeState {
        &mut self.state
    }

    /// Creates a node with default geometry and traversal state.
    pub(crate) fn new(id: UiNodeId, policy: crate::Policy, data: UiNodeData) -> Self {
        Self {
            state: UiNodeState {
                id,
                layout: NodeLayout::default(),
                visible: true,
                hovered: false,
                focused: false,
                clicked: false,
                active: false,
                scroll_delta: None,
                policy,
            },
            data,
        }
    }

    /// Writes layout as the source of truth.
    pub(crate) fn set_layout(&mut self, layout: NodeLayout) {
        self.state.set_layout(layout);
    }

    /// Writes a simple non-scrolled layout.
    pub(crate) fn set_layout_from_rect(&mut self, rect: Recti, content_size: Dimensioni) {
        self.set_layout(NodeLayout::from_rect(rect, content_size));
    }

    /// Returns the node's children when it accepts children.
    pub(crate) fn children(&self) -> &[UiNode] {
        match &self.data {
            UiNodeData::Widget(_) => &[],
            UiNodeData::Container(container) => container.children(),
        }
    }

    /// Returns the node's mutable children when it accepts children.
    pub(crate) fn children_mut(&mut self) -> Option<&mut Vec<UiNode>> {
        match &mut self.data {
            UiNodeData::Widget(_) => None,
            UiNodeData::Container(container) => Some(container.children_mut()),
        }
    }

    /// Returns whether this node is a container.
    pub(crate) fn is_container(&self) -> bool {
        matches!(self.data, UiNodeData::Container(_))
    }

    /// Counts this node and every retained descendant.
    #[cfg(test)]
    pub(crate) fn debug_node_count(&self) -> usize {
        1 + self.children().iter().map(Self::debug_node_count).sum::<usize>()
    }

    /// Counts old erased public-widget adapters in this subtree.
    #[cfg(test)]
    pub(crate) fn debug_erased_adapter_count(&self) -> usize {
        let here = match &self.data {
            UiNodeData::Widget(widget) => usize::from(widget.debug_is_erased_widget_adapter()),
            UiNodeData::Container(container) => usize::from(container.debug_is_erased_widget_adapter()),
        };
        here + self.children().iter().map(Self::debug_erased_adapter_count).sum::<usize>()
    }

    /// Finds a node in this subtree.
    pub(crate) fn find(&self, id: UiNodeId) -> Option<&UiNode> {
        if self.id() == id {
            return Some(self);
        }
        self.children().iter().find_map(|child| child.find(id))
    }

    /// Finds a mutable node in this subtree.
    pub(crate) fn find_mut(&mut self, id: UiNodeId) -> Option<&mut UiNode> {
        if self.id() == id {
            return Some(self);
        }
        self.children_mut()?.iter_mut().find_map(|child| child.find_mut(id))
    }

    /// Collects this node id and all descendant ids.
    pub(crate) fn collect_ids(&self, ids: &mut Vec<UiNodeId>) {
        ids.push(self.id());
        for child in self.children() {
            child.collect_ids(ids);
        }
    }
}

/// Runtime payload for a common UI node.
pub(crate) enum UiNodeData {
    /// Leaf-node behavior without child membership.
    Widget(Box<dyn NodeBehavior>),
    /// Container behavior with child membership.
    Container(Box<dyn Container>),
}
