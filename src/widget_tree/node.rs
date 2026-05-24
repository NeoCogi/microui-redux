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
    input::{ContainerOption, ScrollBehavior},
    layout::{SizePolicy, StackDirection},
    Custom, Id, Node, ScrollAreaHandle,
};

use super::{TreeCustomRender, WidgetHandle, WidgetStateHandleDyn};

/// Stable identifier assigned to a retained node.
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

#[derive(Copy, Clone, Debug, Eq, PartialEq)]
/// Identifier for live state stored outside the retained node description.
pub(crate) struct TreeResourceId(usize);

/// Live state owned by a retained tree.
///
/// Tree nodes describe structure and stable IDs; this registry owns the state handles and callbacks
/// needed to execute that structure.
pub(crate) enum WidgetTreeResource {
    Widget(Box<dyn WidgetStateHandleDyn>),
    CustomRender { state: WidgetHandle<Custom>, render: TreeCustomRender },
    ScrollArea(ScrollAreaHandle),
    Node(WidgetHandle<Node>),
}

#[derive(Default)]
/// Registry for live resources referenced by retained tree nodes.
pub(crate) struct WidgetTreeResources {
    entries: Vec<WidgetTreeResource>,
}

impl WidgetTreeResources {
    pub(crate) fn push(&mut self, resource: WidgetTreeResource) -> TreeResourceId {
        let id = TreeResourceId(self.entries.len());
        self.entries.push(resource);
        id
    }

    pub(crate) fn widget(&self, id: TreeResourceId) -> &dyn WidgetStateHandleDyn {
        match &self.entries[id.0] {
            WidgetTreeResource::Widget(widget) => &**widget,
            _ => panic!("tree resource {:?} is not a widget", id),
        }
    }

    pub(crate) fn custom_render(&self, id: TreeResourceId) -> (&WidgetHandle<Custom>, &TreeCustomRender) {
        match &self.entries[id.0] {
            WidgetTreeResource::CustomRender { state, render } => (state, render),
            _ => panic!("tree resource {:?} is not a custom-render resource", id),
        }
    }

    pub(crate) fn scroll_area(&self, id: TreeResourceId) -> &ScrollAreaHandle {
        match &self.entries[id.0] {
            WidgetTreeResource::ScrollArea(handle) => handle,
            _ => panic!("tree resource {:?} is not a scroll-area resource", id),
        }
    }

    pub(crate) fn node(&self, id: TreeResourceId) -> &WidgetHandle<Node> {
        match &self.entries[id.0] {
            WidgetTreeResource::Node(state) => state,
            _ => panic!("tree resource {:?} is not a node resource", id),
        }
    }
}

/// Kind of a retained node emitted by [`super::WidgetTreeBuilder`].
pub(crate) enum WidgetTreeNodeKind {
    /// Leaf node that dispatches widget state through the normal widget pipeline.
    Widget {
        /// Resource registry entry for the retained widget state handle.
        resource: TreeResourceId,
    },
    /// Leaf node that records a deferred custom-render callback.
    CustomRender {
        /// Resource registry entry for custom widget state and backend callback.
        resource: TreeResourceId,
    },
    /// Scrollable child subtree with its own retained scroll-area state.
    ScrollArea {
        /// Resource registry entry for the scroll-area handle.
        resource: TreeResourceId,
        /// Scroll area rendering options.
        opt: ContainerOption,
        /// Scroll behavior applied while traversing the scroll area.
        scroll_behavior: ScrollBehavior,
    },
    /// Collapsible header node with optional child content.
    Header {
        /// Resource registry entry for the header state handle.
        resource: TreeResourceId,
    },
    /// Tree node with automatic indentation while expanded.
    Tree {
        /// Resource registry entry for the tree node state handle.
        resource: TreeResourceId,
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
    /// Returns a compact name used in validation diagnostics.
    pub(super) fn name(&self) -> &'static str {
        match self {
            Self::Widget { .. } => "widget",
            Self::CustomRender { .. } => "custom_render",
            Self::ScrollArea { .. } => "scroll_area",
            Self::Header { .. } => "header",
            Self::Tree { .. } => "tree_node",
            Self::Row { .. } => "row",
            Self::Grid { .. } => "grid",
            Self::Column => "column",
            Self::Stack { .. } => "stack",
        }
    }

    /// Returns the stable kind discriminator used by builder id hashing.
    pub(super) fn tag(&self) -> u8 {
        match self {
            Self::Widget { .. } => 1,
            Self::CustomRender { .. } => 2,
            Self::ScrollArea { .. } => 3,
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

    /// Returns internal node parts for container traversal without exposing the enum publicly.
    pub(crate) fn parts(&self) -> (NodeId, &WidgetTreeNodeKind, &[WidgetTreeNode]) {
        (self.id, &self.kind, &self.children)
    }
}

/// Completed retained widget tree.
#[derive(Default)]
pub struct WidgetTree {
    pub(super) roots: Vec<WidgetTreeNode>,
    pub(crate) resources: WidgetTreeResources,
}

impl WidgetTree {
    /// Returns the root nodes of the tree.
    pub fn roots(&self) -> &[WidgetTreeNode] {
        &self.roots
    }

    pub(crate) fn resources(&self) -> &WidgetTreeResources {
        &self.resources
    }
}
