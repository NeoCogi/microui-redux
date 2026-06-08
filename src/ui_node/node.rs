use crate::{Dimensioni, GridSpan, Id, Recti, SizePolicy, Vec2i};

use super::NodeBehavior;

/// Stable runtime node identifier.
pub(crate) type UiNodeId = Id;

/// Runtime-level structural role for layout and traversal policy.
///
/// TODO: remove this once root chrome is represented by ordinary composed nodes instead of a
/// runtime-recognized structural kind.
#[derive(Copy, Clone, Debug, Eq, PartialEq)]
pub(crate) enum UiNodeKind {
    /// Ordinary retained widget/container node.
    Normal,
    /// Synthetic root-window body node.
    RootWindow,
}

/// Internal layout/role metadata for runtime nodes.
#[derive(Copy, Clone, Debug)]
pub(crate) struct UiNodeMetadata {
    /// Runtime structural role independent of concrete behavior type.
    pub(crate) kind: UiNodeKind,
    /// Optional vertical policy contributed to a parent column by this node.
    ///
    /// TODO: remove this once layout contribution is represented by a coherent node layout model
    /// instead of a row-specific compatibility field.
    pub(crate) vertical_child_policy: Option<SizePolicy>,
}

impl Default for UiNodeMetadata {
    fn default() -> Self {
        Self {
            kind: UiNodeKind::Normal,
            vertical_child_policy: None,
        }
    }
}

/// Persistent layout result for one runtime node.
///
/// During the migration `frame`, `control`, and `content.viewport` are still screen-space compatible.
/// The target model keeps `frame` in parent content coordinates and derives screen geometry from
/// stack traversal state.
#[derive(Copy, Clone, Debug)]
pub(crate) struct NodeLayout {
    /// Node allocation in the parent content coordinate space.
    pub(crate) frame: Recti,
    /// Behavior-owned rect used for container controls/body painting during the migration.
    pub(crate) control: Recti,
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
            control: Recti::default(),
            content: ContentSpace::default(),
            content_size: Dimensioni::default(),
            propagate_child_overflow: true,
        }
    }
}

impl NodeLayout {
    /// Builds a simple non-scrolled layout.
    pub(crate) fn from_rect(rect: Recti, parent_clip: Recti, content_size: Dimensioni) -> Self {
        Self::from_parts(
            rect,
            rect,
            rect,
            parent_clip,
            Dimensioni::new(rect.width.max(0), rect.height.max(0)),
            content_size,
        )
    }

    /// Builds a layout from explicit frame, control rect, viewport, inherited clip, and virtual size.
    pub(crate) fn from_parts(frame: Recti, control: Recti, viewport: Recti, parent_clip: Recti, virtual_size: Dimensioni, content_size: Dimensioni) -> Self {
        Self {
            frame,
            control,
            content: ContentSpace::new(viewport, parent_clip, virtual_size),
            content_size,
            propagate_child_overflow: true,
        }
    }

    /// Returns this layout with an updated behavior-owned control rect.
    pub(crate) fn with_control(mut self, control: Recti) -> Self {
        self.control = control;
        self
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
    /// Builds content-space data from a viewport, inherited clip, and virtual extent.
    pub(crate) fn new(viewport: Recti, parent_clip: Recti, virtual_size: Dimensioni) -> Self {
        Self {
            content_to_parent_translation: Vec2i::default(),
            viewport: parent_clip.intersect(&viewport).unwrap_or_default(),
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
pub(crate) struct UiNode {
    /// Stable runtime node id.
    pub(crate) id: UiNodeId,
    /// Parent node id when this node is nested under a container.
    pub(crate) parent: Option<UiNodeId>,
    /// Full screen-space node bounds.
    pub(crate) rect: Recti,
    /// Persistent layout result mirrored from compatibility geometry during migration.
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
    /// Grid span used when this node is a child of a grid container.
    pub(crate) grid_span: GridSpan,
    /// Internal layout/role metadata.
    pub(crate) metadata: UiNodeMetadata,
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
            layout: NodeLayout::default(),
            visible: true,
            enabled: true,
            hovered: false,
            focused: false,
            clicked: false,
            active: false,
            scroll_delta: None,
            policy,
            grid_span,
            metadata: UiNodeMetadata::default(),
            data,
        }
    }

    /// Writes layout as the source of truth and mirrors the temporary rect alias.
    pub(crate) fn set_layout(&mut self, layout: NodeLayout) {
        self.layout = layout;
        self.rect = layout.frame;
    }

    /// Writes a simple non-scrolled layout and mirrors compatibility geometry from it.
    pub(crate) fn set_layout_from_rect(&mut self, rect: Recti, parent_clip: Recti, content_size: Dimensioni) {
        self.set_layout(NodeLayout::from_rect(rect, parent_clip, content_size));
    }

    /// Returns the node's children when it accepts children.
    pub(crate) fn children(&self) -> &[UiNodeId] {
        match &self.data {
            UiNodeData::Leaf { .. } => &[],
            UiNodeData::Branch { children, .. } => children,
        }
    }

    /// Returns the node's mutable children when it accepts children.
    pub(crate) fn children_mut(&mut self) -> Option<&mut Vec<UiNodeId>> {
        match &mut self.data {
            UiNodeData::Leaf { .. } => None,
            UiNodeData::Branch { children, .. } => Some(children),
        }
    }

}

/// Runtime payload for a common UI node.
///
/// Container child membership lives here, not in the concrete container behavior object. Nodes are
/// born under one parent and may be removed with their subtree, but the retained node runtime does
/// not support reparenting. This keeps the single-parent invariant local to the runtime graph APIs.
pub(crate) enum UiNodeData {
    /// Node behavior without child membership.
    Leaf {
        /// Concrete retained node behavior.
        behavior: Box<dyn NodeBehavior>,
    },
    /// Node behavior with child membership.
    Branch {
        /// Concrete retained node behavior.
        behavior: Box<dyn NodeBehavior>,
        /// Child membership.
        children: Vec<UiNodeId>,
    },
}
