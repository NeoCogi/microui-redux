use crate::context::{TreeCustomRender, WidgetStateHandleDyn};
use crate::{Dimensioni, GridSpan, Id, Recti};
use crate::input::ControlState;

use super::ContainerTrait;

/// Stable runtime node identifier.
pub(crate) type UiNodeId = Id;

/// Common runtime node state shared by widgets and containers.
pub(crate) struct UiNode {
    /// Stable runtime node id.
    pub(crate) id: UiNodeId,
    /// Parent node id when this node is nested under a container.
    pub(crate) parent: Option<UiNodeId>,
    /// Full screen-space node bounds.
    pub(crate) rect: Recti,
    /// Child/content area computed by the runtime.
    pub(crate) client: Recti,
    /// Effective visible clip after ancestor clips.
    pub(crate) clip: Recti,
    /// Measured child/content size.
    pub(crate) content_size: Dimensioni,
    /// Whether this node participates in traversal.
    pub(crate) visible: bool,
    /// Whether this node can interact.
    pub(crate) enabled: bool,
    /// Last control state produced for this node.
    pub(crate) control: ControlState,
    /// Placement policy used by runtime layout passes.
    pub(crate) policy: crate::Policy,
    /// Grid span used when this node is a child of a grid container.
    pub(crate) grid_span: GridSpan,
    /// Node-specific payload.
    pub(crate) data: UiNodeData,
}

impl UiNode {
    /// Creates a node with default geometry and traversal state.
    pub(crate) fn new(id: UiNodeId, parent: Option<UiNodeId>, policy: crate::Policy, grid_span: GridSpan, data: UiNodeData) -> Self {
        Self {
            id,
            parent,
            rect: Recti::default(),
            client: Recti::default(),
            clip: Recti::default(),
            content_size: Dimensioni::default(),
            visible: true,
            enabled: true,
            control: ControlState::default(),
            policy,
            grid_span,
            data,
        }
    }

    /// Returns the node's container children when it is a container.
    pub(crate) fn children(&self) -> &[UiNodeId] {
        match &self.data {
            UiNodeData::Widget { .. } => &[],
            UiNodeData::Container { children, .. } => children,
        }
    }

    /// Returns the node's mutable container children when it is a container.
    pub(crate) fn children_mut(&mut self) -> Option<&mut Vec<UiNodeId>> {
        match &mut self.data {
            UiNodeData::Widget { .. } => None,
            UiNodeData::Container { children, .. } => Some(children),
        }
    }
}

/// Runtime payload for a common UI node.
///
/// Container child membership lives here, not in the concrete container behavior object. Nodes are
/// born under one parent and may be removed with their subtree, but the retained node runtime does
/// not support reparenting. This keeps the single-parent invariant local to the runtime graph APIs.
pub(crate) enum UiNodeData {
    /// Leaf widget node.
    Widget {
        /// Type-erased retained widget state.
        widget: Box<dyn WidgetStateHandleDyn>,
        /// Optional custom backend render callback for custom-render leaves.
        custom_render: Option<TreeCustomRender>,
    },
    /// Framework-owned container node.
    Container {
        /// Concrete child-owning container object.
        container: Box<dyn ContainerTrait>,
        /// Child membership. Leaf widgets do not carry this allocation.
        children: Vec<UiNodeId>,
    },
}
