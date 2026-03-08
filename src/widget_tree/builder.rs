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
//! Builder APIs for assembling retained widget trees with stable IDs.

use std::{
    cell::RefCell,
    collections::hash_map::DefaultHasher,
    hash::{Hash, Hasher},
    rc::Rc,
};

use rs_math3d::Dimensioni;

use crate::{
    input::{ContainerOption, WidgetBehaviourOption},
    layout::{SizePolicy, StackDirection},
    widget::Widget,
    ContainerHandle, Custom, CustomRenderArgs, Node, TextBlock, TextWrap,
};

use super::{erased_widget_state, widget_handle, NodeId, Policy, TreeCustomRender, WidgetHandle, WidgetTree, WidgetTreeNode, WidgetTreeNodeKind};

struct BuilderFrame {
    scope_seed: u64,
    next_auto: u64,
    nodes: Vec<WidgetTreeNode>,
}

impl BuilderFrame {
    fn root(seed: u64) -> Self {
        Self {
            scope_seed: seed,
            next_auto: 0,
            nodes: Vec::new(),
        }
    }

    fn child(seed: u64) -> Self {
        Self {
            scope_seed: seed,
            next_auto: 0,
            nodes: Vec::new(),
        }
    }
}

#[derive(Copy, Clone, Debug, PartialEq)]
/// Optional metadata applied when inserting one retained node.
///
/// This keeps identity and placement concerns in one place so the builder API
/// does not need separate `keyed_*` and `*_with_policy` method families.
pub struct NodeOptions {
    policy: Policy,
    key: Option<u64>,
}

impl Default for NodeOptions {
    fn default() -> Self {
        Self::new()
    }
}

impl NodeOptions {
    /// Creates default options with automatic placement and no explicit key.
    pub const fn new() -> Self {
        Self { policy: Policy::auto(), key: None }
    }

    /// Creates options with an explicit placement policy.
    pub const fn with_policy(policy: Policy) -> Self {
        Self { policy, key: None }
    }

    /// Creates options keyed from the provided value.
    pub fn keyed<K: Hash>(key: K) -> Self {
        Self::new().key(key)
    }

    /// Replaces the placement policy.
    pub const fn policy(mut self, policy: Policy) -> Self {
        self.policy = policy;
        self
    }

    /// Stores a stable hashed key for this node.
    pub fn key<K: Hash>(mut self, key: K) -> Self {
        self.key = Some(hash_builder_key(key));
        self
    }
}

fn hash_builder_key<K: Hash>(key: K) -> u64 {
    let mut hasher = DefaultHasher::new();
    key.hash(&mut hasher);
    hasher.finish()
}

/// Builder that creates a retained widget tree.
///
/// Unkeyed methods derive IDs from sibling order and remain stable only while
/// the surrounding structure stays in the same order. Use `*_with` together
/// with [`NodeOptions::keyed`] for dynamic or reorderable children.
pub struct WidgetTreeBuilder {
    frames: Vec<BuilderFrame>,
}

impl Default for WidgetTreeBuilder {
    fn default() -> Self {
        Self::new()
    }
}

impl WidgetTreeBuilder {
    /// Default root seed used by [`WidgetTreeBuilder::new`].
    pub const DEFAULT_ROOT_SEED: u64 = 0x9e37_79b9_7f4a_7c15;

    /// Creates an empty builder with a root scope.
    pub fn new() -> Self {
        Self::with_seed(Self::DEFAULT_ROOT_SEED)
    }

    /// Creates an empty builder whose root IDs are derived from `seed`.
    pub fn with_seed(seed: u64) -> Self {
        Self { frames: vec![BuilderFrame::root(seed)] }
    }

    /// Builds a retained tree by executing `f` within a fresh builder.
    pub fn build(f: impl FnOnce(&mut Self)) -> WidgetTree {
        let mut builder = Self::new();
        f(&mut builder);
        builder.finish()
    }

    /// Builds a retained tree whose root IDs are derived from `seed`.
    pub fn build_with_seed(seed: u64, f: impl FnOnce(&mut Self)) -> WidgetTree {
        let mut builder = Self::with_seed(seed);
        f(&mut builder);
        builder.finish()
    }

    /// Finishes the builder and returns the resulting tree.
    pub fn finish(mut self) -> WidgetTree {
        debug_assert_eq!(self.frames.len(), 1, "widget tree builder scopes must be balanced");
        let frame = self.frames.pop().expect("root frame missing");
        WidgetTree { roots: frame.nodes }
    }

    /// Adds an unkeyed widget leaf node.
    pub fn widget<W: Widget + 'static>(&mut self, widget: WidgetHandle<W>) -> NodeId {
        self.widget_with(NodeOptions::new(), widget)
    }

    /// Adds a widget leaf node with optional identity and placement metadata.
    pub fn widget_with<W: Widget + 'static>(&mut self, options: NodeOptions, widget: WidgetHandle<W>) -> NodeId {
        self.push_leaf(options, WidgetTreeNodeKind::Widget { widget: erased_widget_state(widget) })
    }

    /// Adds a text block without wrapping.
    pub fn text(&mut self, text: impl Into<String>) -> NodeId {
        let text = text.into();
        self.widget(widget_handle(TextBlock::new(text)))
    }

    /// Adds a wrapped text block.
    pub fn text_with_wrap(&mut self, text: impl Into<String>, wrap: TextWrap) -> NodeId {
        let text = text.into();
        self.widget(widget_handle(TextBlock::with_wrap(text, wrap)))
    }

    /// Adds a custom-render widget node.
    pub fn custom_render<F>(&mut self, state: WidgetHandle<Custom>, f: F) -> NodeId
    where
        F: FnMut(Dimensioni, &CustomRenderArgs) + 'static,
    {
        self.custom_render_with(NodeOptions::new(), state, f)
    }

    /// Adds a custom-render widget node with optional identity and placement metadata.
    pub fn custom_render_with<F>(&mut self, options: NodeOptions, state: WidgetHandle<Custom>, f: F) -> NodeId
    where
        F: FnMut(Dimensioni, &CustomRenderArgs) + 'static,
    {
        let render: TreeCustomRender = Rc::new(RefCell::new(Box::new(f)));
        self.push_leaf(options, WidgetTreeNodeKind::CustomRender { state, render })
    }

    /// Adds an unkeyed embedded container node.
    pub fn container(&mut self, handle: ContainerHandle, opt: ContainerOption, behaviour: WidgetBehaviourOption, f: impl FnOnce(&mut Self)) -> NodeId {
        self.container_with(NodeOptions::new(), handle, opt, behaviour, f)
    }

    /// Adds an embedded container node with optional identity and placement metadata.
    pub fn container_with(
        &mut self,
        options: NodeOptions,
        handle: ContainerHandle,
        opt: ContainerOption,
        behaviour: WidgetBehaviourOption,
        f: impl FnOnce(&mut Self),
    ) -> NodeId {
        self.push_group(options, WidgetTreeNodeKind::Container { handle, opt, behaviour }, f)
    }

    /// Adds an unkeyed collapsible header node.
    pub fn header(&mut self, state: WidgetHandle<Node>, f: impl FnOnce(&mut Self)) -> NodeId {
        self.header_with(NodeOptions::new(), state, f)
    }

    /// Adds a collapsible header node with optional identity and placement metadata.
    pub fn header_with(&mut self, options: NodeOptions, state: WidgetHandle<Node>, f: impl FnOnce(&mut Self)) -> NodeId {
        self.push_group(options, WidgetTreeNodeKind::Header { state }, f)
    }

    /// Adds an unkeyed tree node that indents its children while expanded.
    pub fn tree_node(&mut self, state: WidgetHandle<Node>, f: impl FnOnce(&mut Self)) -> NodeId {
        self.tree_node_with(NodeOptions::new(), state, f)
    }

    /// Adds a tree node with optional identity and placement metadata.
    pub fn tree_node_with(&mut self, options: NodeOptions, state: WidgetHandle<Node>, f: impl FnOnce(&mut Self)) -> NodeId {
        self.push_group(options, WidgetTreeNodeKind::Tree { state }, f)
    }

    /// Adds an unkeyed row flow group.
    pub fn row(&mut self, widths: &[SizePolicy], height: SizePolicy, f: impl FnOnce(&mut Self)) -> NodeId {
        self.row_with(NodeOptions::new(), widths, height, f)
    }

    /// Adds a row flow group with optional identity and placement metadata.
    pub fn row_with(&mut self, options: NodeOptions, widths: &[SizePolicy], height: SizePolicy, f: impl FnOnce(&mut Self)) -> NodeId {
        self.push_group(options, WidgetTreeNodeKind::Row { widths: widths.to_vec(), height }, f)
    }

    /// Adds an unkeyed grid flow group.
    pub fn grid(&mut self, widths: &[SizePolicy], heights: &[SizePolicy], f: impl FnOnce(&mut Self)) -> NodeId {
        self.grid_with(NodeOptions::new(), widths, heights, f)
    }

    /// Adds a grid flow group with optional identity and placement metadata.
    pub fn grid_with(&mut self, options: NodeOptions, widths: &[SizePolicy], heights: &[SizePolicy], f: impl FnOnce(&mut Self)) -> NodeId {
        self.push_group(
            options,
            WidgetTreeNodeKind::Grid {
                widths: widths.to_vec(),
                heights: heights.to_vec(),
            },
            f,
        )
    }

    /// Adds an unkeyed nested column scope.
    pub fn column(&mut self, f: impl FnOnce(&mut Self)) -> NodeId {
        self.column_with(NodeOptions::new(), f)
    }

    /// Adds a nested column scope with optional identity and placement metadata.
    pub fn column_with(&mut self, options: NodeOptions, f: impl FnOnce(&mut Self)) -> NodeId {
        self.push_group(options, WidgetTreeNodeKind::Column, f)
    }

    /// Adds an unkeyed stack scope.
    pub fn stack(&mut self, width: SizePolicy, height: SizePolicy, direction: StackDirection, f: impl FnOnce(&mut Self)) -> NodeId {
        self.stack_with(NodeOptions::new(), width, height, direction, f)
    }

    /// Adds a stack scope with optional identity and placement metadata.
    pub fn stack_with(&mut self, options: NodeOptions, width: SizePolicy, height: SizePolicy, direction: StackDirection, f: impl FnOnce(&mut Self)) -> NodeId {
        self.push_group(options, WidgetTreeNodeKind::Stack { width, height, direction }, f)
    }

    fn push_leaf(&mut self, options: NodeOptions, kind: WidgetTreeNodeKind) -> NodeId {
        let id = self.alloc_id(kind.tag(), options.key);
        self.current_frame_mut().nodes.push(WidgetTreeNode {
            id,
            policy: options.policy,
            kind,
            children: Vec::new(),
        });
        id
    }

    fn push_group(&mut self, options: NodeOptions, kind: WidgetTreeNodeKind, f: impl FnOnce(&mut Self)) -> NodeId {
        let id = self.alloc_id(kind.tag(), options.key);
        self.frames.push(BuilderFrame::child(id.raw() as u64));
        f(self);
        let frame = self.frames.pop().expect("child frame missing");
        self.current_frame_mut().nodes.push(WidgetTreeNode {
            id,
            policy: options.policy,
            kind,
            children: frame.nodes,
        });
        id
    }

    fn alloc_id(&mut self, tag: u8, key: Option<u64>) -> NodeId {
        let frame = self.current_frame_mut();
        let ordinal = frame.next_auto;
        frame.next_auto += 1;

        let mut hasher = DefaultHasher::new();
        frame.scope_seed.hash(&mut hasher);
        tag.hash(&mut hasher);
        match key {
            Some(key) => {
                1u8.hash(&mut hasher);
                key.hash(&mut hasher);
            }
            None => {
                0u8.hash(&mut hasher);
                ordinal.hash(&mut hasher);
            }
        }
        NodeId::new(hasher.finish())
    }

    fn current_frame_mut(&mut self) -> &mut BuilderFrame {
        self.frames.last_mut().expect("widget tree builder frame missing")
    }
}
