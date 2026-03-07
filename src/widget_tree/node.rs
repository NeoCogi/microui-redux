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
//! Retained node definitions and placement metadata.

use crate::{
    input::{ContainerOption, WidgetBehaviourOption},
    layout::{SizePolicy, StackDirection},
    ContainerHandle, Custom, Id, Node,
};

use super::{TreeCustomRender, WidgetHandle, WidgetStateHandleDyn};

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
            Self::Container { .. } => 3,
            Self::Header { .. } => 4,
            Self::Tree { .. } => 5,
            Self::Row { .. } => 6,
            Self::Grid { .. } => 7,
            Self::Column => 8,
            Self::Stack { .. } => 9,
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
