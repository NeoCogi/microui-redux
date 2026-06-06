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

use std::{cell::RefCell, collections::HashMap, hash::Hash, rc::Rc};

use rs_math3d::Dimensioni;

use crate::{
    id::{hash_id_key, IdNamespace},
    input::{ContainerOption, ScrollBehavior},
    sizing::{SizePolicy, StackDirection},
    ui_node::{
        scroll_viewport_node, scrollbar_nodes, Column, Disclosure, Grid, Row, ScrollArea as UiScrollArea, Stack, UiNode, UiNodeData, UiNodeId, WidgetNode,
    },
    widget::Widget,
    Custom, CustomRenderArgs, Node, TextBlock, TextWrap,
};

use super::{erased_widget_state, widget_handle, TreeCustomRender, WidgetHandle};

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
    /// Root node ids submitted to the context.
    roots: Vec<UiNodeId>,
    /// Runtime nodes owned by the retained nodes.
    nodes: HashMap<UiNodeId, UiNode>,
}

impl UiNodeSet {
    /// Returns the root ids in this set.
    pub fn roots(&self) -> &[UiNodeId] {
        &self.roots
    }

    /// Returns a retained node by id.
    #[cfg(test)]
    pub(crate) fn node(&self, id: UiNodeId) -> Option<&UiNode> {
        self.nodes.get(&id)
    }

    /// Consumes this tree into root ids and runtime nodes.
    pub(crate) fn into_parts(self) -> (Vec<UiNodeId>, HashMap<UiNodeId, UiNode>) {
        (self.roots, self.nodes)
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

/// Stack frame used while the builder collects a group node's children.
struct BuilderFrame {
    /// Parent node for children collected in this frame.
    parent: Option<UiNodeId>,
    /// Seed mixed into automatic child ids for this scope.
    scope_seed: u64,
    /// Next ordinal for unkeyed automatic children.
    next_auto: u64,
    /// Child node ids collected inside this frame.
    nodes: Vec<UiNodeId>,
}

impl BuilderFrame {
    /// Creates the root builder frame.
    fn root(seed: u64) -> Self {
        Self {
            scope_seed: seed,
            parent: None,
            next_auto: 0,
            nodes: Vec::new(),
        }
    }

    /// Creates a child frame whose automatic ids are scoped by its parent node id.
    fn child(parent: UiNodeId, seed: u64) -> Self {
        Self {
            parent: Some(parent),
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
    /// Nodes emitted by the builder, keyed by stable id.
    nodes: HashMap<UiNodeId, UiNode>,
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

    /// Adds a custom-render widget node.
    pub fn custom_render<F>(self, state: impl Into<WidgetHandle<Custom>>, f: F) -> NodeId
    where
        F: FnMut(Dimensioni, &CustomRenderArgs) + 'static,
    {
        self.builder.insert_custom_render(self.options, state, f)
    }

    /// Adds a scroll area node.
    pub fn scroll_area(self, opt: ContainerOption, scroll_behavior: ScrollBehavior, f: impl FnOnce(&mut UiNodeBuilder)) -> NodeId {
        self.builder.insert_scroll_area(self.options, opt, scroll_behavior, f)
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
        Self {
            frames: vec![BuilderFrame::root(seed)],
            nodes: HashMap::new(),
        }
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
        Self::validate_unique_node_ids(&frame.nodes);
        UiNodeSet { roots: frame.nodes, nodes: self.nodes }
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
        self.push_leaf(
            options,
            TAG_WIDGET,
            UiNodeData::Leaf {
                behavior: Box::new(WidgetNode {
                    widget: erased_widget_state(widget),
                    custom_render: None,
                    pending_events: Vec::new(),
                }),
            },
        )
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
        self.push_leaf(
            options,
            TAG_CUSTOM_RENDER,
            UiNodeData::Leaf {
                behavior: Box::new(WidgetNode {
                    widget: erased_widget_state(state),
                    custom_render: Some(render),
                    pending_events: Vec::new(),
                }),
            },
        )
    }

    /// Adds an unkeyed scroll area node.
    pub fn scroll_area(&mut self, opt: ContainerOption, scroll_behavior: ScrollBehavior, f: impl FnOnce(&mut Self)) -> NodeId {
        self.insert_scroll_area(NodeOptions::new(), opt, scroll_behavior, f)
    }

    /// Adds a scroll area node with optional identity and placement metadata.
    fn insert_scroll_area(&mut self, options: NodeOptions, opt: ContainerOption, scroll_behavior: ScrollBehavior, f: impl FnOnce(&mut Self)) -> NodeId {
        let id = self.alloc_id(TAG_SCROLL_AREA, options.key);
        let parent = self.current_frame().parent;
        let viewport = scroll_viewport_node(id, scroll_behavior, Vec::new());
        let viewport_id = viewport.id;

        self.frames.push(BuilderFrame {
            parent: Some(viewport_id),
            scope_seed: id.raw() as u64,
            next_auto: 0,
            nodes: Vec::new(),
        });
        f(self);
        let frame = self.frames.pop().expect("scroll viewport frame missing");

        let mut viewport = scroll_viewport_node(id, scroll_behavior, frame.nodes);
        viewport.parent = Some(id);

        let mut internal_children = Vec::new();
        internal_children.push(viewport_id);
        let scrollbars = scrollbar_nodes(id, scroll_behavior);
        internal_children.extend(scrollbars.iter().map(|node| node.id));

        let node = UiNode::new(
            id,
            parent,
            options.policy,
            options.grid_span,
            UiNodeData::Branch {
                behavior: Box::new(UiScrollArea::new(scroll_behavior, opt)),
                children: Vec::new(),
                internal_children,
            },
        );
        if self.nodes.contains_key(&id) || self.nodes.contains_key(&viewport_id) || scrollbars.iter().any(|node| self.nodes.contains_key(&node.id)) {
            panic!(
                "duplicate retained node id {:?} in UiNodeBuilder output. Use distinct NodeOptions::keyed(...) values for siblings with the same kind.",
                id
            );
        }
        self.nodes.insert(id, node);
        self.nodes.insert(viewport_id, viewport);
        for scrollbar in scrollbars {
            self.nodes.insert(scrollbar.id, scrollbar);
        }
        self.current_frame_mut().nodes.push(id);
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
            UiNodeData::Branch {
                behavior: Box::new(Disclosure {
                    state,
                    indent_children: false,
                    content_layout: Column,
                }),
                children: Vec::new(),
                internal_children: Vec::new(),
            },
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
            UiNodeData::Branch {
                behavior: Box::new(Disclosure {
                    state,
                    indent_children: true,
                    content_layout: Column,
                }),
                children: Vec::new(),
                internal_children: Vec::new(),
            },
            f,
        )
    }

    /// Adds an unkeyed row flow group.
    pub fn row(&mut self, widths: &[SizePolicy], height: SizePolicy, f: impl FnOnce(&mut Self)) -> NodeId {
        self.insert_row(NodeOptions::new(), widths, height, f)
    }

    /// Adds a row flow group with optional identity and placement metadata.
    fn insert_row(&mut self, options: NodeOptions, widths: &[SizePolicy], height: SizePolicy, f: impl FnOnce(&mut Self)) -> NodeId {
        let id = self.push_group(
            options,
            TAG_ROW,
            UiNodeData::Branch {
                behavior: Box::new(Row { widths: widths.to_vec(), height }),
                children: Vec::new(),
                internal_children: Vec::new(),
            },
            f,
        );
        if let Some(node) = self.nodes.get_mut(&id) {
            node.metadata.vertical_child_policy = Some(height);
        }
        id
    }

    /// Adds an unkeyed grid flow group.
    pub fn grid(&mut self, widths: &[SizePolicy], heights: &[SizePolicy], f: impl FnOnce(&mut Self)) -> NodeId {
        self.insert_grid(NodeOptions::new(), widths, heights, f)
    }

    /// Adds a grid flow group with optional identity and placement metadata.
    fn insert_grid(&mut self, options: NodeOptions, widths: &[SizePolicy], heights: &[SizePolicy], f: impl FnOnce(&mut Self)) -> NodeId {
        self.push_group(
            options,
            TAG_GRID,
            UiNodeData::Branch {
                behavior: Box::new(Grid {
                    widths: widths.to_vec(),
                    heights: heights.to_vec(),
                }),
                children: Vec::new(),
                internal_children: Vec::new(),
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
        self.push_group(
            options,
            TAG_COLUMN,
            UiNodeData::Branch {
                behavior: Box::new(Column),
                children: Vec::new(),
                internal_children: Vec::new(),
            },
            f,
        )
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
            UiNodeData::Branch {
                behavior: Box::new(Stack { width, height, direction }),
                children: Vec::new(),
                internal_children: Vec::new(),
            },
            f,
        )
    }

    /// Pushes a leaf node into the current builder frame.
    fn push_leaf(&mut self, options: NodeOptions, tag: u8, data: UiNodeData) -> NodeId {
        let id = self.alloc_id(tag, options.key);
        let parent = self.current_frame().parent;
        let node = UiNode::new(id, parent, options.policy, options.grid_span, data);
        // Leaf nodes have no child frame; they become siblings in the current frame directly.
        if self.nodes.contains_key(&id) {
            panic!(
                "duplicate retained node id {:?} in UiNodeBuilder output. Use distinct NodeOptions::keyed(...) values for siblings with the same kind.",
                id
            );
        }
        self.nodes.insert(id, node);
        self.current_frame_mut().nodes.push(id);
        id
    }

    /// Pushes a group node after collecting children in a nested builder frame.
    fn push_group(&mut self, options: NodeOptions, tag: u8, mut data: UiNodeData, f: impl FnOnce(&mut Self)) -> NodeId {
        let id = self.alloc_id(tag, options.key);
        let parent = self.current_frame().parent;
        // Child auto-ids are scoped by the group id, so sibling insertion outside the group does
        // not affect descendants.
        self.frames.push(BuilderFrame::child(id, id.raw() as u64));
        f(self);
        let frame = self.frames.pop().expect("child frame missing");
        if let UiNodeData::Branch { children, .. } = &mut data {
            *children = frame.nodes;
        }
        let node = UiNode::new(id, parent, options.policy, options.grid_span, data);
        if self.nodes.contains_key(&id) {
            panic!(
                "duplicate retained node id {:?} in UiNodeBuilder output. Use distinct NodeOptions::keyed(...) values for siblings with the same kind.",
                id
            );
        }
        self.nodes.insert(id, node);
        self.current_frame_mut().nodes.push(id);
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
        IdNamespace::UINODE_BUILDER.id([frame.scope_seed, tag as u64, key_kind, key_value])
    }

    /// Returns the frame currently receiving new nodes.
    fn current_frame_mut(&mut self) -> &mut BuilderFrame {
        self.frames.last_mut().expect("ui node builder frame missing")
    }

    /// Returns the frame currently receiving new nodes.
    fn current_frame(&self) -> &BuilderFrame {
        self.frames.last().expect("ui node builder frame missing")
    }

    /// Rejects duplicate retained IDs before a tree can enter runtime traversal.
    fn validate_unique_node_ids(nodes: &[UiNodeId]) {
        let mut seen = HashMap::new();
        Self::collect_node_ids(nodes, "root", &mut seen);
    }

    /// Recursively collects node ids and panics if the same id appears twice.
    fn collect_node_ids(nodes: &[UiNodeId], parent_path: &str, seen: &mut HashMap<NodeId, String>) {
        for (index, node_id) in nodes.iter().enumerate() {
            let path = format!("{parent_path}/{index}:{}", node_id.raw());
            if let Some(first_path) = seen.insert(*node_id, path.clone()) {
                panic!(
                    "duplicate retained node id {:?} in UiNodeBuilder output; first node: {}; duplicate node: {}. Use distinct NodeOptions::keyed(...) values for siblings with the same kind.",
                    node_id, first_path, path
                );
            }
        }
    }
}
