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

use std::{cell::RefCell, collections::HashMap, hash::Hash, rc::Rc};

use rs_math3d::Dimensioni;

use crate::{
    id::{hash_id_key, IdNamespace},
    input::{ContainerOption, ScrollBehavior},
    layout::{SizePolicy, StackDirection},
    widget::Widget,
    Custom, CustomRenderArgs, Node, ScrollAreaHandle, TextBlock, TextWrap,
};

use super::{
    erased_widget_state, widget_handle, NodeId, Policy, TreeCustomRender, WidgetHandle, WidgetTree, WidgetTreeNode, WidgetTreeNodeKind, WidgetTreeResource,
    WidgetTreeResources,
};

/// Stack frame used while the builder collects a group node's children.
struct BuilderFrame {
    /// Seed mixed into automatic child ids for this scope.
    scope_seed: u64,
    /// Next ordinal for unkeyed automatic children.
    next_auto: u64,
    /// Nodes collected inside this frame.
    nodes: Vec<WidgetTreeNode>,
}

impl BuilderFrame {
    /// Creates the root builder frame.
    fn root(seed: u64) -> Self {
        Self {
            scope_seed: seed,
            next_auto: 0,
            nodes: Vec::new(),
        }
    }

    /// Creates a child frame whose automatic ids are scoped by its parent node id.
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
    /// Layout policy applied to the inserted node.
    policy: Policy,
    /// Optional application-supplied identity key.
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

/// Hashes an application-provided key into the builder's id space.
fn hash_builder_key<K: Hash>(key: K) -> u64 {
    hash_id_key(key)
}

/// Builder that creates a retained widget tree.
///
/// Unkeyed methods derive IDs from the order of other unkeyed siblings and
/// remain stable only while that unkeyed structure stays in the same order.
/// Keyed nodes use a separate identity path and do not advance the unkeyed
/// sibling counter, so inserting a keyed node does not shift later unkeyed IDs.
/// Use [`WidgetTreeBuilder::node`] together with [`NodeOptions::keyed`] for dynamic or
/// reorderable children.
pub struct WidgetTreeBuilder {
    /// Stack of open builder scopes.
    frames: Vec<BuilderFrame>,
    /// Resource registry populated while nodes are inserted.
    resources: WidgetTreeResources,
}

/// Builder adapter that applies one [`NodeOptions`] value to the next inserted node.
pub struct NodeBuilder<'a> {
    /// Builder receiving the next node.
    builder: &'a mut WidgetTreeBuilder,
    /// Options consumed by the next insertion.
    options: NodeOptions,
}

impl<'a> NodeBuilder<'a> {
    /// Adds a widget leaf node.
    pub fn widget<W: Widget + 'static>(self, widget: impl Into<WidgetHandle<W>>) -> NodeId {
        self.builder.insert_widget(self.options, widget)
    }

    /// Adds a custom-render widget node.
    pub fn custom_render<F>(self, state: impl Into<WidgetHandle<Custom>>, f: F) -> NodeId
    where
        F: FnMut(Dimensioni, &CustomRenderArgs) + 'static,
    {
        self.builder.insert_custom_render(self.options, state, f)
    }

    /// Adds a scroll area node.
    pub fn scroll_area(
        self,
        handle: impl Into<ScrollAreaHandle>,
        opt: ContainerOption,
        scroll_behavior: ScrollBehavior,
        f: impl FnOnce(&mut WidgetTreeBuilder),
    ) -> NodeId {
        self.builder.insert_scroll_area(self.options, handle, opt, scroll_behavior, f)
    }

    /// Adds a collapsible header node.
    pub fn header(self, state: impl Into<WidgetHandle<Node>>, f: impl FnOnce(&mut WidgetTreeBuilder)) -> NodeId {
        self.builder.insert_header(self.options, state, f)
    }

    /// Adds a tree node that indents its children while expanded.
    pub fn tree_node(self, state: impl Into<WidgetHandle<Node>>, f: impl FnOnce(&mut WidgetTreeBuilder)) -> NodeId {
        self.builder.insert_tree_node(self.options, state, f)
    }

    /// Adds a row flow group.
    pub fn row(self, widths: &[SizePolicy], height: SizePolicy, f: impl FnOnce(&mut WidgetTreeBuilder)) -> NodeId {
        self.builder.insert_row(self.options, widths, height, f)
    }

    /// Adds a grid flow group.
    pub fn grid(self, widths: &[SizePolicy], heights: &[SizePolicy], f: impl FnOnce(&mut WidgetTreeBuilder)) -> NodeId {
        self.builder.insert_grid(self.options, widths, heights, f)
    }

    /// Adds a nested column scope.
    pub fn column(self, f: impl FnOnce(&mut WidgetTreeBuilder)) -> NodeId {
        self.builder.insert_column(self.options, f)
    }

    /// Adds a stack scope.
    pub fn stack(self, width: SizePolicy, height: SizePolicy, direction: StackDirection, f: impl FnOnce(&mut WidgetTreeBuilder)) -> NodeId {
        self.builder.insert_stack(self.options, width, height, direction, f)
    }
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
        Self {
            frames: vec![BuilderFrame::root(seed)],
            resources: WidgetTreeResources::default(),
        }
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
        Self::validate_unique_node_ids(&frame.nodes);
        WidgetTree {
            roots: frame.nodes,
            resources: self.resources,
        }
    }

    /// Applies `options` to the next inserted node.
    pub fn node(&mut self, options: NodeOptions) -> NodeBuilder<'_> {
        NodeBuilder { builder: self, options }
    }

    /// Adds an unkeyed widget leaf node.
    pub fn widget<W: Widget + 'static>(&mut self, widget: impl Into<WidgetHandle<W>>) -> NodeId {
        self.insert_widget(NodeOptions::new(), widget)
    }

    /// Adds a widget leaf node with optional identity and placement metadata.
    fn insert_widget<W: Widget + 'static>(&mut self, options: NodeOptions, widget: impl Into<WidgetHandle<W>>) -> NodeId {
        let widget = widget.into();
        let resource = self.resources.push(WidgetTreeResource::Widget(erased_widget_state(widget)));
        self.push_leaf(options, WidgetTreeNodeKind::Widget { resource })
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
    pub fn custom_render<F>(&mut self, state: impl Into<WidgetHandle<Custom>>, f: F) -> NodeId
    where
        F: FnMut(Dimensioni, &CustomRenderArgs) + 'static,
    {
        self.insert_custom_render(NodeOptions::new(), state, f)
    }

    /// Adds a custom-render widget node with optional identity and placement metadata.
    fn insert_custom_render<F>(&mut self, options: NodeOptions, state: impl Into<WidgetHandle<Custom>>, f: F) -> NodeId
    where
        F: FnMut(Dimensioni, &CustomRenderArgs) + 'static,
    {
        let state = state.into();
        let render: TreeCustomRender = Rc::new(RefCell::new(Box::new(f)));
        let resource = self.resources.push(WidgetTreeResource::CustomRender { state, render });
        self.push_leaf(options, WidgetTreeNodeKind::CustomRender { resource })
    }

    /// Adds an unkeyed scroll area node.
    pub fn scroll_area(
        &mut self,
        handle: impl Into<ScrollAreaHandle>,
        opt: ContainerOption,
        scroll_behavior: ScrollBehavior,
        f: impl FnOnce(&mut Self),
    ) -> NodeId {
        self.insert_scroll_area(NodeOptions::new(), handle, opt, scroll_behavior, f)
    }

    /// Adds a scroll area node with optional identity and placement metadata.
    fn insert_scroll_area(
        &mut self,
        options: NodeOptions,
        handle: impl Into<ScrollAreaHandle>,
        opt: ContainerOption,
        scroll_behavior: ScrollBehavior,
        f: impl FnOnce(&mut Self),
    ) -> NodeId {
        let handle = handle.into();
        let resource = self.resources.push(WidgetTreeResource::ScrollArea(handle));
        self.push_group(options, WidgetTreeNodeKind::ScrollArea { resource, opt, scroll_behavior }, f)
    }

    /// Adds an unkeyed collapsible header node.
    pub fn header(&mut self, state: impl Into<WidgetHandle<Node>>, f: impl FnOnce(&mut Self)) -> NodeId {
        self.insert_header(NodeOptions::new(), state, f)
    }

    /// Adds a collapsible header node with optional identity and placement metadata.
    fn insert_header(&mut self, options: NodeOptions, state: impl Into<WidgetHandle<Node>>, f: impl FnOnce(&mut Self)) -> NodeId {
        let state = state.into();
        let resource = self.resources.push(WidgetTreeResource::Node(state));
        self.push_group(options, WidgetTreeNodeKind::Header { resource }, f)
    }

    /// Adds an unkeyed tree node that indents its children while expanded.
    pub fn tree_node(&mut self, state: impl Into<WidgetHandle<Node>>, f: impl FnOnce(&mut Self)) -> NodeId {
        self.insert_tree_node(NodeOptions::new(), state, f)
    }

    /// Adds a tree node with optional identity and placement metadata.
    fn insert_tree_node(&mut self, options: NodeOptions, state: impl Into<WidgetHandle<Node>>, f: impl FnOnce(&mut Self)) -> NodeId {
        let state = state.into();
        let resource = self.resources.push(WidgetTreeResource::Node(state));
        self.push_group(options, WidgetTreeNodeKind::Tree { resource }, f)
    }

    /// Adds an unkeyed row flow group.
    pub fn row(&mut self, widths: &[SizePolicy], height: SizePolicy, f: impl FnOnce(&mut Self)) -> NodeId {
        self.insert_row(NodeOptions::new(), widths, height, f)
    }

    /// Adds a row flow group with optional identity and placement metadata.
    fn insert_row(&mut self, options: NodeOptions, widths: &[SizePolicy], height: SizePolicy, f: impl FnOnce(&mut Self)) -> NodeId {
        self.push_group(options, WidgetTreeNodeKind::Row { widths: widths.to_vec(), height }, f)
    }

    /// Adds an unkeyed grid flow group.
    pub fn grid(&mut self, widths: &[SizePolicy], heights: &[SizePolicy], f: impl FnOnce(&mut Self)) -> NodeId {
        self.insert_grid(NodeOptions::new(), widths, heights, f)
    }

    /// Adds a grid flow group with optional identity and placement metadata.
    fn insert_grid(&mut self, options: NodeOptions, widths: &[SizePolicy], heights: &[SizePolicy], f: impl FnOnce(&mut Self)) -> NodeId {
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
        self.insert_column(NodeOptions::new(), f)
    }

    /// Adds a nested column scope with optional identity and placement metadata.
    fn insert_column(&mut self, options: NodeOptions, f: impl FnOnce(&mut Self)) -> NodeId {
        self.push_group(options, WidgetTreeNodeKind::Column, f)
    }

    /// Adds an unkeyed stack scope.
    pub fn stack(&mut self, width: SizePolicy, height: SizePolicy, direction: StackDirection, f: impl FnOnce(&mut Self)) -> NodeId {
        self.insert_stack(NodeOptions::new(), width, height, direction, f)
    }

    /// Adds a stack scope with optional identity and placement metadata.
    fn insert_stack(&mut self, options: NodeOptions, width: SizePolicy, height: SizePolicy, direction: StackDirection, f: impl FnOnce(&mut Self)) -> NodeId {
        self.push_group(options, WidgetTreeNodeKind::Stack { width, height, direction }, f)
    }

    /// Pushes a leaf node into the current builder frame.
    fn push_leaf(&mut self, options: NodeOptions, kind: WidgetTreeNodeKind) -> NodeId {
        let id = self.alloc_id(kind.tag(), options.key);
        // Leaf nodes have no child frame; they become siblings in the current frame directly.
        self.current_frame_mut().nodes.push(WidgetTreeNode {
            id,
            policy: options.policy,
            kind,
            children: Vec::new(),
        });
        id
    }

    /// Pushes a group node after collecting children in a nested builder frame.
    fn push_group(&mut self, options: NodeOptions, kind: WidgetTreeNodeKind, f: impl FnOnce(&mut Self)) -> NodeId {
        let id = self.alloc_id(kind.tag(), options.key);
        // Child auto-ids are scoped by the group id, so sibling insertion outside the group does
        // not affect descendants.
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

    /// Allocates a stable node id from the current scope, node kind, and optional user key.
    fn alloc_id(&mut self, tag: u8, key: Option<u64>) -> NodeId {
        let frame = self.current_frame_mut();
        let ordinal = match key {
            Some(_) => None,
            None => {
                // Unkeyed ids use sibling order and therefore advance the per-frame auto counter.
                let ordinal = frame.next_auto;
                frame.next_auto += 1;
                Some(ordinal)
            }
        };

        let (key_kind, key_value) = match key {
            Some(key) => {
                // Keyed ids use a different discriminator so they do not collide with unkeyed ids.
                (1, key)
            }
            None => (0, ordinal.expect("unkeyed ordinal missing")),
        };
        IdNamespace::WIDGET_TREE_BUILDER.id([frame.scope_seed, tag as u64, key_kind, key_value])
    }

    /// Returns the frame currently receiving new nodes.
    fn current_frame_mut(&mut self) -> &mut BuilderFrame {
        self.frames.last_mut().expect("widget tree builder frame missing")
    }

    /// Rejects duplicate retained IDs before a tree can enter runtime traversal.
    fn validate_unique_node_ids(nodes: &[WidgetTreeNode]) {
        let mut seen = HashMap::new();
        Self::collect_node_ids(nodes, "root", &mut seen);
    }

    /// Recursively collects node ids and panics if the same id appears twice.
    fn collect_node_ids(nodes: &[WidgetTreeNode], parent_path: &str, seen: &mut HashMap<NodeId, String>) {
        for (index, node) in nodes.iter().enumerate() {
            let (node_id, kind, children) = node.parts();
            let path = format!("{parent_path}/{index}:{}", kind.name());
            if let Some(first_path) = seen.insert(node_id, path.clone()) {
                panic!(
                    "duplicate retained node id {:?} in WidgetTreeBuilder output; first node: {}; duplicate node: {}. Use distinct NodeOptions::keyed(...) values for siblings with the same kind.",
                    node_id, first_path, path
                );
            }
            Self::collect_node_ids(children, &path, seen);
        }
    }
}
