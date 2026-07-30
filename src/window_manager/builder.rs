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
//! Builder APIs for assembling retained UI node sets with stable IDs.

use std::{collections::HashMap, hash::Hash};

use crate::{
    id::{hash_id_key, IdNamespace},
    render::{CustomRenderHandle, RendererBackend},
    sizing::{SizePolicy, StackDirection},
    ui_node::{
        scroll_viewport_node, scrollbar_nodes, shared_scroll_area_state, Column, Disclosure, Grid, Row, ScrollArea as UiScrollArea, Stack, UiNode, UiNodeData,
        ScrollAreaOption, UiNodeId, WidgetNode,
    },
    widget::{Widget, WidgetStateOwner},
    Node, Recti, TextBlock, TextWrap,
};

use super::{erased_widget_state, widget_handle, WidgetHandle};

/// Stable identifier assigned to a retained node.
pub type NodeId = crate::Id;

/// Grid placement span for one retained node inside a grid container.
#[derive(Copy, Clone, Debug, Eq, PartialEq)]
pub struct GridSpan {
    /// Number of grid columns occupied by the node.
    pub columns: usize,
    /// Number of grid rows occupied by the node.
    pub rows: usize,
}

impl GridSpan {
    /// Default one-cell grid placement.
    pub const ONE: Self = Self { columns: 1, rows: 1 };

    /// Creates a grid span, clamping zero-sized spans to one track.
    pub const fn new(columns: usize, rows: usize) -> Self {
        Self {
            columns: if columns == 0 { 1 } else { columns },
            rows: if rows == 0 { 1 } else { rows },
        }
    }
}

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

/// Completed retained UI node set.
#[derive(Default)]
pub struct UiNodeSet {
    /// Root nodes submitted to the context.
    roots: Vec<UiNode>,
}

impl UiNodeSet {
    /// Returns the root ids in this set.
    pub fn roots(&self) -> Vec<UiNodeId> {
        self.roots.iter().map(UiNode::id).collect()
    }

    /// Returns a retained node by id.
    #[cfg(test)]
    pub(crate) fn node(&self, id: UiNodeId) -> Option<&UiNode> {
        self.roots.iter().find_map(|root| root.find(id))
    }

    /// Consumes this tree into owned root nodes.
    pub(crate) fn into_roots(self) -> Vec<UiNode> {
        self.roots
    }
}

const TAG_WIDGET: u8 = 1;
const TAG_CUSTOM_RENDER: u8 = 2;
const TAG_SCROLL_AREA: u8 = 3;
const TAG_HEADER: u8 = 4;
const TAG_TREE: u8 = 5;
const TAG_ROW: u8 = 6;
const TAG_GRID: u8 = 7;
const TAG_COLUMN: u8 = 8;
const TAG_STACK: u8 = 9;

/// Child collected while the builder is assembling one parent scope.
struct BuilderChild {
    /// Child node.
    node: UiNode,
    /// Grid placement span requested for this child by the builder call.
    grid_span: GridSpan,
}

/// Stack frame used while the builder collects a group node's children.
struct BuilderFrame {
    /// Seed mixed into automatic child ids for this scope.
    scope_seed: u64,
    /// Next ordinal for unkeyed automatic children.
    next_auto: u64,
    /// Child nodes collected inside this frame.
    nodes: Vec<BuilderChild>,
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
    /// Grid span used when the inserted node is placed in a grid parent.
    grid_span: GridSpan,
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
        Self {
            policy: Policy::auto(),
            grid_span: GridSpan::ONE,
            key: None,
        }
    }

    /// Creates options with an explicit placement policy.
    pub const fn with_policy(policy: Policy) -> Self {
        Self {
            policy,
            grid_span: GridSpan::ONE,
            key: None,
        }
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

    /// Replaces the grid span used when this node is placed in a grid parent.
    pub const fn grid_span(mut self, columns: usize, rows: usize) -> Self {
        self.grid_span = GridSpan::new(columns, rows);
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

/// Builder that creates a retained UI node set.
///
/// Unkeyed methods derive IDs from the order of other unkeyed siblings and
/// remain stable only while that unkeyed structure stays in the same order.
/// Keyed nodes use a separate identity path and do not advance the unkeyed
/// sibling counter, so inserting a keyed node does not shift later unkeyed IDs.
/// Use [`UiNodeBuilder::node`] together with [`NodeOptions::keyed`] for dynamic or
/// reorderable children.
pub struct UiNodeBuilder {
    /// Stack of open builder scopes.
    frames: Vec<BuilderFrame>,
}

/// Builder adapter that applies one [`NodeOptions`] value to the next inserted node.
pub struct NodeBuilder<'a> {
    /// Builder receiving the next node.
    builder: &'a mut UiNodeBuilder,
    /// Options consumed by the next insertion.
    options: NodeOptions,
}

impl<'a> NodeBuilder<'a> {
    /// Adds a widget leaf node.
    pub fn widget<W: Widget + 'static>(self, widget: impl Into<WidgetHandle<W>>) -> NodeId {
        self.builder.insert_widget(self.options, widget)
    }

    /// Adds a concrete state-owning widget through the temporary P1 projection bridge.
    #[doc(hidden)]
    pub fn state_widget<W: WidgetStateOwner>(self, widget: W) -> NodeId {
        self.builder.insert_state_widget(self.options, widget)
    }

    /// Adds a custom-render widget node.
    pub fn custom_render<B, W>(self, state: impl Into<WidgetHandle<W>>, renderer: CustomRenderHandle<B>) -> NodeId
    where
        B: RendererBackend,
        W: Widget + 'static,
    {
        self.builder.insert_custom_render(self.options, state, renderer)
    }

    /// Adds a scroll-area node; [`ScrollAreaOption::ENABLE_SCROLL`] enables overflow scrolling.
    pub fn scroll_area(self, opt: ScrollAreaOption, f: impl FnOnce(&mut UiNodeBuilder)) -> NodeId {
        self.builder.insert_scroll_area(self.options, opt, f)
    }

    /// Adds a collapsible header node.
    pub fn header(self, state: impl Into<WidgetHandle<Node>>, f: impl FnOnce(&mut UiNodeBuilder)) -> NodeId {
        self.builder.insert_header(self.options, state, f)
    }

    /// Adds a tree node that indents its children while expanded.
    pub fn tree_node(self, state: impl Into<WidgetHandle<Node>>, f: impl FnOnce(&mut UiNodeBuilder)) -> NodeId {
        self.builder.insert_tree_node(self.options, state, f)
    }

    /// Adds a row flow group.
    pub fn row(self, widths: &[SizePolicy], height: SizePolicy, f: impl FnOnce(&mut UiNodeBuilder)) -> NodeId {
        self.builder.insert_row(self.options, widths, height, f)
    }

    /// Adds a grid flow group.
    pub fn grid(self, widths: &[SizePolicy], heights: &[SizePolicy], f: impl FnOnce(&mut UiNodeBuilder)) -> NodeId {
        self.builder.insert_grid(self.options, widths, heights, f)
    }

    /// Adds a nested column scope.
    pub fn column(self, f: impl FnOnce(&mut UiNodeBuilder)) -> NodeId {
        self.builder.insert_column(self.options, f)
    }

    /// Adds a stack scope.
    pub fn stack(self, width: SizePolicy, height: SizePolicy, direction: StackDirection, f: impl FnOnce(&mut UiNodeBuilder)) -> NodeId {
        self.builder.insert_stack(self.options, width, height, direction, f)
    }
}

impl Default for UiNodeBuilder {
    fn default() -> Self {
        Self::new()
    }
}

impl UiNodeBuilder {
    /// Default root seed used by [`UiNodeBuilder::new`].
    pub const DEFAULT_ROOT_SEED: u64 = 0x9e37_79b9_7f4a_7c15;

    /// Creates an empty builder with a root scope.
    pub fn new() -> Self {
        Self::with_seed(Self::DEFAULT_ROOT_SEED)
    }

    /// Creates an empty builder whose root IDs are derived from `seed`.
    pub fn with_seed(seed: u64) -> Self {
        Self { frames: vec![BuilderFrame::root(seed)] }
    }

    /// Builds retained nodes by executing `f` within a fresh builder.
    pub fn build(f: impl FnOnce(&mut Self)) -> UiNodeSet {
        let mut builder = Self::new();
        f(&mut builder);
        builder.finish()
    }

    /// Builds a retained nodes whose root IDs are derived from `seed`.
    pub fn build_with_seed(seed: u64, f: impl FnOnce(&mut Self)) -> UiNodeSet {
        let mut builder = Self::with_seed(seed);
        f(&mut builder);
        builder.finish()
    }

    /// Finishes the builder and returns the resulting node set.
    pub fn finish(mut self) -> UiNodeSet {
        debug_assert_eq!(self.frames.len(), 1, "ui node builder scopes must be balanced");
        let frame = self.frames.pop().expect("root frame missing");
        let roots = Self::child_nodes(frame.nodes);
        Self::validate_unique_node_ids(&roots);
        UiNodeSet { roots }
    }

    /// Applies `options` to the next inserted node.
    pub fn node(&mut self, options: NodeOptions) -> NodeBuilder<'_> {
        NodeBuilder { builder: self, options }
    }

    /// Adds an unkeyed widget leaf node.
    pub fn widget<W: Widget + 'static>(&mut self, widget: impl Into<WidgetHandle<W>>) -> NodeId {
        self.insert_widget(NodeOptions::new(), widget)
    }

    /// Adds a concrete state-owning widget through the temporary P1 projection bridge.
    ///
    /// The final owning [`crate::Node`] insertion surface replaces this staging method in P1.3.
    #[doc(hidden)]
    pub fn state_widget<W: WidgetStateOwner>(&mut self, widget: W) -> NodeId {
        self.insert_state_widget(NodeOptions::new(), widget)
    }

    /// Adds a widget leaf node with optional identity and placement metadata.
    fn insert_widget<W: Widget + 'static>(&mut self, options: NodeOptions, widget: impl Into<WidgetHandle<W>>) -> NodeId {
        let widget = widget.into();
        self.push_leaf(
            options,
            TAG_WIDGET,
            UiNodeData::Widget(Box::new(WidgetNode::legacy(erased_widget_state(widget), None))),
        )
    }

    /// Erases a concrete state-owning runtime only at the existing projection boundary.
    fn insert_state_widget<W: WidgetStateOwner>(&mut self, options: NodeOptions, widget: W) -> NodeId {
        self.push_leaf(options, TAG_WIDGET, UiNodeData::Widget(Box::new(WidgetNode::direct(widget))))
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
    pub fn custom_render<B, W>(&mut self, state: impl Into<WidgetHandle<W>>, renderer: CustomRenderHandle<B>) -> NodeId
    where
        B: RendererBackend,
        W: Widget + 'static,
    {
        self.insert_custom_render(NodeOptions::new(), state, renderer)
    }

    /// Adds a custom-render widget node with optional identity and placement metadata.
    fn insert_custom_render<B, W>(&mut self, options: NodeOptions, state: impl Into<WidgetHandle<W>>, renderer: CustomRenderHandle<B>) -> NodeId
    where
        B: RendererBackend,
        W: Widget + 'static,
    {
        let state = state.into();
        self.push_leaf(
            options,
            TAG_CUSTOM_RENDER,
            UiNodeData::Widget(Box::new(WidgetNode::legacy(erased_widget_state(state), Some(renderer.key)))),
        )
    }

    /// Adds an unkeyed scroll-area node; [`ScrollAreaOption::ENABLE_SCROLL`] enables overflow scrolling.
    pub fn scroll_area(&mut self, opt: ScrollAreaOption, f: impl FnOnce(&mut Self)) -> NodeId {
        self.insert_scroll_area(NodeOptions::new(), opt, f)
    }

    /// Adds a scroll area node with optional identity and placement metadata.
    fn insert_scroll_area(&mut self, options: NodeOptions, opt: ScrollAreaOption, f: impl FnOnce(&mut Self)) -> NodeId {
        let id = self.alloc_id(TAG_SCROLL_AREA, options.key);
        let state = shared_scroll_area_state();
        self.frames.push(BuilderFrame {
            scope_seed: id.raw() as u64,
            next_auto: 0,
            nodes: Vec::new(),
        });
        f(self);
        let frame = self.frames.pop().expect("scroll viewport frame missing");

        let scroll_enabled = opt.intersects(ScrollAreaOption::ENABLE_SCROLL);
        let viewport = scroll_viewport_node(id, state.clone(), scroll_enabled, Self::child_nodes(frame.nodes));

        let scrollbars = scrollbar_nodes(id, state.clone(), scroll_enabled);
        let mut children = Vec::with_capacity(1 + scrollbars.len());
        children.push(viewport);
        children.extend(scrollbars);

        let node = self.create_node_with_id(id, options, UiNodeData::Container(Box::new(UiScrollArea::new(state, opt, children))));
        Self::validate_unique_node_ids(std::slice::from_ref(&node));
        self.current_frame_mut().nodes.push(BuilderChild { node, grid_span: options.grid_span });
        id
    }

    /// Adds an unkeyed collapsible header node.
    pub fn header(&mut self, state: impl Into<WidgetHandle<Node>>, f: impl FnOnce(&mut Self)) -> NodeId {
        self.insert_header(NodeOptions::new(), state, f)
    }

    /// Adds a collapsible header node with optional identity and placement metadata.
    fn insert_header(&mut self, options: NodeOptions, state: impl Into<WidgetHandle<Node>>, f: impl FnOnce(&mut Self)) -> NodeId {
        let state = state.into();
        self.push_group(
            options,
            TAG_HEADER,
            UiNodeData::Container(Box::new(Disclosure {
                state,
                indent_children: false,
                header_rect: Recti::default(),
                content_layout: Column::default(),
            })),
            f,
        )
    }

    /// Adds an unkeyed tree node that indents its children while expanded.
    pub fn tree_node(&mut self, state: impl Into<WidgetHandle<Node>>, f: impl FnOnce(&mut Self)) -> NodeId {
        self.insert_tree_node(NodeOptions::new(), state, f)
    }

    /// Adds a tree node with optional identity and placement metadata.
    fn insert_tree_node(&mut self, options: NodeOptions, state: impl Into<WidgetHandle<Node>>, f: impl FnOnce(&mut Self)) -> NodeId {
        let state = state.into();
        self.push_group(
            options,
            TAG_TREE,
            UiNodeData::Container(Box::new(Disclosure {
                state,
                indent_children: true,
                header_rect: Recti::default(),
                content_layout: Column::default(),
            })),
            f,
        )
    }

    /// Adds an unkeyed row flow group.
    pub fn row(&mut self, widths: &[SizePolicy], height: SizePolicy, f: impl FnOnce(&mut Self)) -> NodeId {
        self.insert_row(NodeOptions::new(), widths, height, f)
    }

    /// Adds a row flow group with optional identity and placement metadata.
    fn insert_row(&mut self, options: NodeOptions, widths: &[SizePolicy], height: SizePolicy, f: impl FnOnce(&mut Self)) -> NodeId {
        let mut options = options;
        if options.policy.height == SizePolicy::Auto {
            options.policy.height = height;
        }
        self.push_group(
            options,
            TAG_ROW,
            UiNodeData::Container(Box::new(Row {
                widths: widths.to_vec(),
                height,
                children: Vec::new(),
            })),
            f,
        )
    }

    /// Adds an unkeyed grid flow group.
    pub fn grid(&mut self, widths: &[SizePolicy], heights: &[SizePolicy], f: impl FnOnce(&mut Self)) -> NodeId {
        self.insert_grid(NodeOptions::new(), widths, heights, f)
    }

    /// Adds a grid flow group with optional identity and placement metadata.
    fn insert_grid(&mut self, options: NodeOptions, widths: &[SizePolicy], heights: &[SizePolicy], f: impl FnOnce(&mut Self)) -> NodeId {
        let id = self.alloc_id(TAG_GRID, options.key);
        self.frames.push(BuilderFrame::child(id.raw() as u64));
        f(self);
        let frame = self.frames.pop().expect("grid frame missing");
        let spans = frame.nodes.iter().map(|child| child.grid_span).collect();
        let children = Self::child_nodes(frame.nodes);
        let node = self.create_node_with_id(
            id,
            options,
            UiNodeData::Container(Box::new(Grid {
                widths: widths.to_vec(),
                heights: heights.to_vec(),
                spans,
                children,
            })),
        );
        self.current_frame_mut().nodes.push(BuilderChild { node, grid_span: options.grid_span });
        id
    }

    /// Adds an unkeyed nested column scope.
    pub fn column(&mut self, f: impl FnOnce(&mut Self)) -> NodeId {
        self.insert_column(NodeOptions::new(), f)
    }

    /// Adds a nested column scope with optional identity and placement metadata.
    fn insert_column(&mut self, options: NodeOptions, f: impl FnOnce(&mut Self)) -> NodeId {
        self.push_group(options, TAG_COLUMN, UiNodeData::Container(Box::new(Column::default())), f)
    }

    /// Adds an unkeyed stack scope.
    pub fn stack(&mut self, width: SizePolicy, height: SizePolicy, direction: StackDirection, f: impl FnOnce(&mut Self)) -> NodeId {
        self.insert_stack(NodeOptions::new(), width, height, direction, f)
    }

    /// Adds a stack scope with optional identity and placement metadata.
    fn insert_stack(&mut self, options: NodeOptions, width: SizePolicy, height: SizePolicy, direction: StackDirection, f: impl FnOnce(&mut Self)) -> NodeId {
        self.push_group(
            options,
            TAG_STACK,
            UiNodeData::Container(Box::new(Stack {
                width,
                height,
                direction,
                children: Vec::new(),
            })),
            f,
        )
    }

    /// Pushes a leaf node into the current builder frame.
    fn push_leaf(&mut self, options: NodeOptions, tag: u8, data: UiNodeData) -> NodeId {
        let (id, node) = self.create_node(tag, options, data);
        // Widget nodes have no child frame; they become siblings in the current frame directly.
        self.current_frame_mut().nodes.push(BuilderChild { node, grid_span: options.grid_span });
        id
    }

    /// Pushes a group node after collecting children in a nested builder frame.
    fn push_group(&mut self, options: NodeOptions, tag: u8, mut data: UiNodeData, f: impl FnOnce(&mut Self)) -> NodeId {
        let id = self.alloc_id(tag, options.key);
        // Child auto-ids are scoped by the group id, so sibling insertion outside the group does
        // not affect descendants.
        self.frames.push(BuilderFrame::child(id.raw() as u64));
        f(self);
        let frame = self.frames.pop().expect("child frame missing");
        let children = Self::child_nodes(frame.nodes);
        if let UiNodeData::Container(container) = &mut data {
            *container.children_mut() = children;
        }
        let node = self.create_node_with_id(id, options, data);
        self.current_frame_mut().nodes.push(BuilderChild { node, grid_span: options.grid_span });
        id
    }

    /// Creates a retained node and assigns its stable id inside the window-manager builder.
    fn create_node(&mut self, tag: u8, options: NodeOptions, data: UiNodeData) -> (NodeId, UiNode) {
        let id = self.alloc_id(tag, options.key);
        let node = self.create_node_with_id(id, options, data);
        (id, node)
    }

    /// Creates a retained node from an already allocated window-manager node id.
    fn create_node_with_id(&self, id: NodeId, options: NodeOptions, data: UiNodeData) -> UiNode {
        UiNode::new(id, options.policy, data)
    }

    /// Extracts owned child nodes from builder-only placement metadata.
    fn child_nodes(children: Vec<BuilderChild>) -> Vec<UiNode> {
        children.into_iter().map(|child| child.node).collect()
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
        IdNamespace::UINODE_BUILDER.id([frame.scope_seed, tag as u64, key_kind, key_value])
    }

    /// Returns the frame currently receiving new nodes.
    fn current_frame_mut(&mut self) -> &mut BuilderFrame {
        self.frames.last_mut().expect("ui node builder frame missing")
    }

    /// Rejects duplicate retained IDs before a tree can enter runtime traversal.
    fn validate_unique_node_ids(nodes: &[UiNode]) {
        let mut seen = HashMap::new();
        Self::collect_node_ids(nodes, "root", &mut seen);
    }

    /// Recursively collects node ids and panics if the same id appears twice.
    fn collect_node_ids(nodes: &[UiNode], parent_path: &str, seen: &mut HashMap<NodeId, String>) {
        for (index, node) in nodes.iter().enumerate() {
            let node_id = node.id();
            let path = format!("{parent_path}/{index}:{}", node_id.raw());
            if let Some(first_path) = seen.insert(node_id, path.clone()) {
                panic!(
                    "duplicate retained node id {:?} in UiNodeBuilder output; first node: {}; duplicate node: {}. Use distinct NodeOptions::keyed(...) values for siblings with the same kind.",
                    node_id, first_path, path
                );
            }
            Self::collect_node_ids(node.children(), &path, seen);
        }
    }
}
