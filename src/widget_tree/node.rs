//! Retained node definitions and placement metadata.

use crate::{
    input::{ContainerOption, WidgetBehaviourOption},
    layout::{SizePolicy, StackDirection},
    ContainerHandle, Custom, Id, Node,
};

use super::{TreeCustomRender, TreeRun, WidgetHandle, WidgetStateHandleDyn};

/// Stable identifier assigned to a retained or runtime node.
pub type NodeId = Id;

/// Placement policy metadata attached to a retained node.
#[derive(Copy, Clone, Debug, PartialEq)]
pub struct Policy {
    /// Width policy associated with the node.
    pub width: SizePolicy,
    /// Height policy associated with the node.
    pub height: SizePolicy,
}

impl Policy {
    /// Creates a policy from explicit width and height rules.
    pub const fn new(width: SizePolicy, height: SizePolicy) -> Self {
        Self { width, height }
    }

    /// Uses automatic sizing on both axes.
    pub const fn auto() -> Self {
        Self::new(SizePolicy::Auto, SizePolicy::Auto)
    }

    /// Uses fixed sizing on both axes.
    pub const fn fixed(width: i32, height: i32) -> Self {
        Self::new(SizePolicy::Fixed(width), SizePolicy::Fixed(height))
    }

    /// Uses a fixed width and automatic height.
    pub const fn fixed_width(width: i32) -> Self {
        Self::new(SizePolicy::Fixed(width), SizePolicy::Auto)
    }

    /// Uses a fixed height and automatic width.
    pub const fn fixed_height(height: i32) -> Self {
        Self::new(SizePolicy::Auto, SizePolicy::Fixed(height))
    }

    /// Uses remainder sizing on both axes.
    pub const fn fill() -> Self {
        Self::new(SizePolicy::Remainder(0), SizePolicy::Remainder(0))
    }
}

/// Kind of a retained node emitted by [`super::WidgetTreeBuilder`].
pub(crate) enum WidgetTreeNodeKind {
    /// Leaf node that dispatches widget state through the normal widget pipeline.
    Widget {
        /// Retained widget state handle.
        widget: Box<dyn WidgetStateHandleDyn>,
    },
    /// Leaf node that behaves like [`crate::Container::widget_custom_render`].
    CustomRender {
        /// Retained widget state handle.
        state: WidgetHandle<Custom>,
        /// Deferred rendering callback enqueued after interaction handling.
        render: TreeCustomRender,
    },
    /// Leaf node that executes arbitrary container code.
    Run {
        /// Callback invoked during tree traversal.
        run: TreeRun,
    },
    /// Embedded container/panel node with its own child subtree.
    Container {
        /// Container handle used for the embedded panel.
        handle: ContainerHandle,
        /// Container rendering options.
        opt: ContainerOption,
        /// Behaviour options applied while embedded.
        behaviour: WidgetBehaviourOption,
    },
    /// Collapsible header node with optional child content.
    Header {
        /// Header state handle that owns the expanded/collapsed state.
        state: WidgetHandle<Node>,
    },
    /// Tree node with automatic indentation while expanded.
    Tree {
        /// Tree state handle that owns the expanded/collapsed state.
        state: WidgetHandle<Node>,
    },
    /// Horizontal row flow group.
    Row {
        /// Track widths applied to children.
        widths: Vec<SizePolicy>,
        /// Shared row height policy.
        height: SizePolicy,
    },
    /// Grid flow group that emits children row-major.
    Grid {
        /// Column width policies.
        widths: Vec<SizePolicy>,
        /// Row height policies.
        heights: Vec<SizePolicy>,
    },
    /// Nested column scope.
    Column,
    /// Vertical stack scope.
    Stack {
        /// Width policy applied to each emitted stack item.
        width: SizePolicy,
        /// Height policy applied to each emitted stack item.
        height: SizePolicy,
        /// Stack direction.
        direction: StackDirection,
    },
}

impl WidgetTreeNodeKind {
    pub(super) fn tag(&self) -> u8 {
        match self {
            Self::Widget { .. } => 1,
            Self::CustomRender { .. } => 2,
            Self::Run { .. } => 3,
            Self::Container { .. } => 4,
            Self::Header { .. } => 5,
            Self::Tree { .. } => 6,
            Self::Row { .. } => 7,
            Self::Grid { .. } => 8,
            Self::Column => 9,
            Self::Stack { .. } => 10,
        }
    }
}

/// A single node in a retained widget tree.
pub struct WidgetTreeNode {
    pub(super) id: NodeId,
    pub(super) policy: Policy,
    pub(super) kind: WidgetTreeNodeKind,
    pub(super) children: Vec<WidgetTreeNode>,
}

impl WidgetTreeNode {
    /// Returns the stable node identifier.
    pub fn id(&self) -> NodeId {
        self.id
    }

    /// Returns the node's policy metadata.
    pub fn policy(&self) -> Policy {
        self.policy
    }

    /// Returns the node kind.
    #[cfg(test)]
    pub(crate) fn kind(&self) -> &WidgetTreeNodeKind {
        &self.kind
    }

    /// Returns the node's child nodes.
    pub fn children(&self) -> &[WidgetTreeNode] {
        &self.children
    }

    pub(crate) fn parts(&self) -> (NodeId, &WidgetTreeNodeKind, &[WidgetTreeNode]) {
        (self.id, &self.kind, &self.children)
    }
}

/// Completed retained widget tree.
#[derive(Default)]
pub struct WidgetTree {
    pub(super) roots: Vec<WidgetTreeNode>,
}

impl WidgetTree {
    /// Returns the root nodes of the tree.
    pub fn roots(&self) -> &[WidgetTreeNode] {
        &self.roots
    }
}
