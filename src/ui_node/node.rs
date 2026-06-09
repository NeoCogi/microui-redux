use crate::{Dimensioni, Id, Recti, Vec2i};

use super::{Container, Widget};

/// Stable runtime node identifier.
pub(crate) type UiNodeId = Id;

/// Persistent layout result for one runtime node.
#[derive(Copy, Clone, Debug)]
pub(crate) struct NodeLayout {
    /// Node allocation in the parent content coordinate space.
    pub(crate) frame: Recti,
    /// Child content coordinate space exposed by this node.
    pub(crate) content: ContentSpace,
    /// Measured/assigned virtual content size in content coordinates.
    pub(crate) content_size: Dimensioni,
    /// Whether child overflow contributes to this node's parent-visible content size.
    pub(crate) propagate_child_overflow: bool,
}

impl Default for NodeLayout {
    fn default() -> Self {
        Self {
            frame: Recti::default(),
            content: ContentSpace::default(),
            content_size: Dimensioni::default(),
            propagate_child_overflow: true,
        }
    }
}

impl NodeLayout {
    /// Builds a simple non-scrolled layout.
    pub(crate) fn from_rect(rect: Recti, content_size: Dimensioni) -> Self {
        Self::from_parts(rect, rect, Dimensioni::new(rect.width.max(0), rect.height.max(0)), content_size)
    }

    /// Builds a layout from explicit frame, child viewport, and virtual size.
    pub(crate) fn from_parts(frame: Recti, viewport: Recti, virtual_size: Dimensioni, content_size: Dimensioni) -> Self {
        Self {
            frame,
            content: ContentSpace::new(viewport, virtual_size),
            content_size,
            propagate_child_overflow: true,
        }
    }

    /// Returns this layout with an updated content size.
    pub(crate) fn with_content_size(mut self, content_size: Dimensioni) -> Self {
        self.content_size = content_size;
        self.content.virtual_size = content_size;
        self
    }

    /// Returns this layout with updated child-overflow propagation.
    pub(crate) fn with_child_overflow_propagation(mut self, propagate_child_overflow: bool) -> Self {
        self.propagate_child_overflow = propagate_child_overflow;
        self
    }
}

/// Child content coordinate space exposed by one node.
#[derive(Copy, Clone, Debug, Default)]
pub(crate) struct ContentSpace {
    /// Translation from this node's child content coordinates to parent content coordinates.
    pub(crate) content_to_parent_translation: Vec2i,
    /// Visible child viewport in parent content coordinates.
    pub(crate) viewport: Recti,
    /// Virtual child content extent in content coordinates.
    pub(crate) virtual_size: Dimensioni,
}

impl ContentSpace {
    /// Builds content-space data from a parent-local viewport and virtual extent.
    pub(crate) fn new(viewport: Recti, virtual_size: Dimensioni) -> Self {
        Self {
            content_to_parent_translation: Vec2i::default(),
            viewport,
            virtual_size,
        }
    }
}

/// Stack-only traversal state derived while walking the node tree.
#[derive(Copy, Clone, Debug)]
pub(crate) struct TraversalState {
    /// Translation from the current content coordinate space to screen coordinates.
    pub(crate) content_to_screen_translation: Vec2i,
    /// Inherited effective clip in screen coordinates.
    pub(crate) screen_clip: Recti,
}

impl TraversalState {
    /// Creates a root traversal state.
    pub(crate) fn root(screen_clip: Recti) -> Self {
        Self {
            content_to_screen_translation: Vec2i::default(),
            screen_clip,
        }
    }

    /// Creates a root traversal state with a screen-space origin.
    pub(crate) fn root_at(origin: Vec2i, screen_clip: Recti) -> Self {
        Self {
            content_to_screen_translation: origin,
            screen_clip,
        }
    }

    /// Enters a node's child content space.
    pub(crate) fn enter(self, layout: NodeLayout) -> Self {
        let screen_viewport = translate_rect(layout.content.viewport, self.content_to_screen_translation);
        Self {
            content_to_screen_translation: self.content_to_screen_translation + layout.content.content_to_parent_translation,
            screen_clip: self.screen_clip.intersect(&screen_viewport).unwrap_or_default(),
        }
    }

    /// Derives a screen-space frame for the node allocation.
    pub(crate) fn screen_frame(self, layout: NodeLayout) -> Recti {
        translate_rect(layout.frame, self.content_to_screen_translation)
    }

    /// Derives a screen-space rect from the current content coordinate space.
    pub(crate) fn screen_rect(self, rect: Recti) -> Recti {
        translate_rect(rect, self.content_to_screen_translation)
    }
}

fn translate_rect(rect: Recti, offset: Vec2i) -> Recti {
    Recti::new(rect.x + offset.x, rect.y + offset.y, rect.width, rect.height)
}

/// Common runtime node state shared by widgets and containers.
pub(crate) struct UiNodeState {
    /// Stable runtime node id.
    id: UiNodeId,
    /// Persistent layout result for traversal.
    pub(crate) layout: NodeLayout,
    /// Whether this node participates in traversal.
    pub(crate) visible: bool,
    /// Whether this node can interact.
    pub(crate) enabled: bool,
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
                enabled: true,
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
    /// Widget behavior without child membership.
    Widget(Box<dyn Widget>),
    /// Container behavior with child membership.
    Container(Box<dyn Container>),
}
