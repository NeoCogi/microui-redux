use crate::context::{TreeCustomRender, WidgetStateHandleDyn};
use crate::{Dimensioni, GridSpan, Id, Recti, Vec2i};
use crate::input::ControlState;

use super::ContainerTrait;

/// Stable runtime node identifier.
pub(crate) type UiNodeId = Id;

/// Container/widget child viewport state.
///
/// `visible_rect` and `virtual_clip` are both screen-space rectangles. The effective clip used for
/// children is the intersection of the parent clip and `virtual_clip`; `translation` maps virtual
/// content into the visible rect for scrolling containers.
#[derive(Copy, Clone, Debug, Default)]
pub(crate) struct ClientArea {
    /// Real visible viewport available to child content.
    pub(crate) visible_rect: Recti,
    /// Virtual content extent represented by this client area.
    pub(crate) virtual_size: Dimensioni,
    /// Real-space clip contributed by this client area before ancestor clipping.
    pub(crate) virtual_clip: Recti,
    /// Virtual-to-real translation, usually negative scroll offset.
    pub(crate) translation: Vec2i,
}

impl ClientArea {
    /// Builds a non-scrolled client area whose virtual size matches its visible rect.
    pub(crate) fn from_rect(rect: Recti) -> Self {
        Self {
            visible_rect: rect,
            virtual_size: Dimensioni::new(rect.width.max(0), rect.height.max(0)),
            virtual_clip: rect,
            translation: Vec2i::default(),
        }
    }

    /// Returns the clip that should be applied after ancestor clipping.
    pub(crate) fn effective_clip(&self, parent_clip: Recti) -> Recti {
        parent_clip.intersect(&self.virtual_clip).unwrap_or_default()
    }
}

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
    /// Explicit client viewport state used by composable containers.
    pub(crate) client_area: ClientArea,
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
            client_area: ClientArea::default(),
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
