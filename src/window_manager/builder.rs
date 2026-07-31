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
//! Transitional builder APIs for assembling unique owning retained UI nodes.

use std::hash::Hash;

use crate::{
    id::hash_id_key,
    render::{CustomRenderHandle, RendererBackend},
    sizing::{SizePolicy, StackDirection},
    ui_node::{
        scroll_viewport_node, scrollbar_nodes, shared_scroll_area_state, Column, ColumnParameters, Disclosure, DisclosureParameters, DisclosureState, Grid,
        GridItem, GridParameters, GridSpan, Row, ScrollArea as UiScrollArea, Stack, UiNode, ScrollAreaOption, UiNodeId,
    },
    widget::WidgetStateOwner,
    TextBlock, TextBlockParameters, TextWrap,
};

/// Transitional identifier returned by projection-era root and result APIs.
///
/// New ownership code should retain typed state handles instead of treating this value as
/// application identity. Each builder insertion still creates a fresh private runtime identity.
pub type NodeId = crate::Id;

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

    /// Runs a test-only inspection against one retained node without exposing a borrow through a
    /// state-owned container's opaque visitor.
    #[cfg(test)]
    pub(crate) fn with_node<R>(&self, id: UiNodeId, f: impl FnOnce(&UiNode) -> R) -> Option<R> {
        let mut f = Some(f);
        self.roots
            .iter()
            .find_map(|root| root.with_node(id, |node| f.take().expect("node visitor invoked twice")(node)))
    }

    /// Consumes this tree into owned root nodes.
    pub(crate) fn into_roots(self) -> Vec<UiNode> {
        self.roots
    }
}

impl From<crate::ui_node::Node> for UiNodeSet {
    fn from(node: crate::ui_node::Node) -> Self {
        Self { roots: vec![node] }
    }
}

impl FromIterator<crate::ui_node::Node> for UiNodeSet {
    fn from_iter<T: IntoIterator<Item = crate::ui_node::Node>>(iter: T) -> Self {
        Self { roots: iter.into_iter().collect() }
    }
}

/// Child collected while the builder is assembling one parent scope.
struct BuilderChild {
    /// Child node.
    node: UiNode,
    /// Temporary projection-edge metadata deleted with this builder in P3.0.
    ///
    /// The value lives on the builder's parent-child edge rather than the generic retained node;
    /// Grid consumes it as `GridItem`, while every other temporary parent preserves the historical
    /// behavior of ignoring it.
    grid_span: GridSpan,
}

/// Stack frame used while the builder collects a group node's children.
struct BuilderFrame {
    /// Child nodes collected inside this frame.
    nodes: Vec<BuilderChild>,
}

impl BuilderFrame {
    fn new() -> Self {
        Self { nodes: Vec::new() }
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
    /// Transitional application key retained only until projection builders are removed in P3.0.
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

    /// Records a transitional builder key.
    ///
    /// Keys no longer reconstruct or reuse owning-node identity; the method remains until the
    /// projection builder is removed in P3.0.
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

    /// Stores a transitional hashed key without changing the node's fresh runtime identity.
    pub fn key<K: Hash>(mut self, key: K) -> Self {
        self.key = Some(hash_builder_key(key));
        self
    }
}

/// Hashes an application-provided key into the builder's id space.
fn hash_builder_key<K: Hash>(key: K) -> u64 {
    hash_id_key(key)
}

/// Builder that moves concrete runtimes into a retained UI node set.
///
/// Every insertion constructs a new non-cloneable [`crate::Node`]. Keys are accepted only for
/// source compatibility during the P1-P3 migration and never reconstruct runtime identity.
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
    /// Adds one concrete state-owning widget runtime as a leaf node.
    pub fn widget<W: WidgetStateOwner>(self, widget: W) -> NodeId {
        self.builder.insert_widget(self.options, widget)
    }

    /// Adds a custom-render widget node.
    pub fn custom_render<B, W>(self, widget: W, renderer: CustomRenderHandle<B>) -> NodeId
    where
        B: RendererBackend,
        W: WidgetStateOwner,
    {
        self.builder.insert_custom_render(self.options, widget, renderer)
    }

    /// Adds a scroll-area node; [`ScrollAreaOption::ENABLE_SCROLL`] enables overflow scrolling.
    pub fn scroll_area(self, opt: ScrollAreaOption, f: impl FnOnce(&mut UiNodeBuilder)) -> NodeId {
        self.builder.insert_scroll_area(self.options, opt, f)
    }

    /// Adds a state-owned framed disclosure header.
    pub fn header(self, label: impl Into<String>, expanded: bool, f: impl FnOnce(&mut UiNodeBuilder)) -> (crate::WidgetStateHandle<DisclosureState>, NodeId) {
        self.builder.insert_header(self.options, label, expanded, f)
    }

    /// Adds a state-owned tree disclosure that indents descendants.
    pub fn tree_node(
        self,
        label: impl Into<String>,
        expanded: bool,
        f: impl FnOnce(&mut UiNodeBuilder),
    ) -> (crate::WidgetStateHandle<DisclosureState>, NodeId) {
        self.builder.insert_tree_node(self.options, label, expanded, f)
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
    /// Legacy root seed retained for source compatibility.
    pub const DEFAULT_ROOT_SEED: u64 = 0x9e37_79b9_7f4a_7c15;

    /// Creates an empty builder with a root scope.
    pub fn new() -> Self {
        Self { frames: vec![BuilderFrame::new()] }
    }

    /// Creates an empty builder.
    ///
    /// `seed` is ignored during the internal P1-P3 migration because node identity is now assigned
    /// once by the process-wide private allocator rather than reconstructed from builder shape.
    pub fn with_seed(_seed: u64) -> Self {
        Self::new()
    }

    /// Builds retained nodes by executing `f` within a fresh builder.
    pub fn build(f: impl FnOnce(&mut Self)) -> UiNodeSet {
        let mut builder = Self::new();
        f(&mut builder);
        builder.finish()
    }

    /// Builds retained nodes while accepting the legacy, now-ignored root seed.
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
        UiNodeSet { roots }
    }

    /// Applies `options` to the next inserted node.
    pub fn node(&mut self, options: NodeOptions) -> NodeBuilder<'_> {
        NodeBuilder { builder: self, options }
    }

    /// Adds one concrete state-owning widget runtime as an unkeyed leaf node.
    pub fn widget<W: WidgetStateOwner>(&mut self, widget: W) -> NodeId {
        self.insert_widget(NodeOptions::new(), widget)
    }

    /// Adds a widget leaf node with optional identity and placement metadata.
    fn insert_widget<W: WidgetStateOwner>(&mut self, options: NodeOptions, widget: W) -> NodeId {
        self.push_node(options, crate::ui_node::Node::widget(widget))
    }

    /// Adds a text block without wrapping.
    pub fn text(&mut self, text: impl Into<String>) -> NodeId {
        let text = text.into();
        let (_, runtime) = TextBlock::create(TextBlockParameters::new(text));
        self.widget(runtime)
    }

    /// Adds a wrapped text block.
    pub fn text_with_wrap(&mut self, text: impl Into<String>, wrap: TextWrap) -> NodeId {
        let text = text.into();
        let (_, runtime) = TextBlock::create(TextBlockParameters::with_wrap(text, wrap));
        self.widget(runtime)
    }

    /// Adds a custom-render widget node.
    pub fn custom_render<B, W>(&mut self, widget: W, renderer: CustomRenderHandle<B>) -> NodeId
    where
        B: RendererBackend,
        W: WidgetStateOwner,
    {
        self.insert_custom_render(NodeOptions::new(), widget, renderer)
    }

    /// Adds a custom-render widget node with optional identity and placement metadata.
    fn insert_custom_render<B, W>(&mut self, options: NodeOptions, widget: W, renderer: CustomRenderHandle<B>) -> NodeId
    where
        B: RendererBackend,
        W: WidgetStateOwner,
    {
        self.push_node(options, crate::ui_node::Node::custom_render(widget, renderer))
    }

    /// Adds an unkeyed scroll-area node; [`ScrollAreaOption::ENABLE_SCROLL`] enables overflow scrolling.
    pub fn scroll_area(&mut self, opt: ScrollAreaOption, f: impl FnOnce(&mut Self)) -> NodeId {
        self.insert_scroll_area(NodeOptions::new(), opt, f)
    }

    /// Adds a scroll area node with optional identity and placement metadata.
    fn insert_scroll_area(&mut self, options: NodeOptions, opt: ScrollAreaOption, f: impl FnOnce(&mut Self)) -> NodeId {
        self.frames.push(BuilderFrame::new());
        f(self);
        let frame = self.frames.pop().expect("scroll viewport frame missing");

        let state = shared_scroll_area_state();
        let scroll_enabled = opt.intersects(ScrollAreaOption::ENABLE_SCROLL);
        // Synthetic scroll descendants now receive their own process-unique IDs. The parent value
        // remains a transitional argument until P2.2 removes the synthetic composition entirely.
        let transitional_parent = crate::Id::new(1);
        let viewport = scroll_viewport_node(transitional_parent, state.clone(), scroll_enabled, Self::child_nodes(frame.nodes));

        let scrollbars = scrollbar_nodes(transitional_parent, state.clone(), scroll_enabled);
        let mut children = Vec::with_capacity(1 + scrollbars.len());
        children.push(viewport);
        children.extend(scrollbars);

        let node = crate::ui_node::Node::legacy_container(Box::new(UiScrollArea::new(state, opt, children)));
        self.push_node(options, node)
    }

    /// Adds an unkeyed state-owned framed disclosure header.
    pub fn header(&mut self, label: impl Into<String>, expanded: bool, f: impl FnOnce(&mut Self)) -> (crate::WidgetStateHandle<DisclosureState>, NodeId) {
        self.insert_header(NodeOptions::new(), label, expanded, f)
    }

    fn insert_header(
        &mut self,
        options: NodeOptions,
        label: impl Into<String>,
        expanded: bool,
        f: impl FnOnce(&mut Self),
    ) -> (crate::WidgetStateHandle<DisclosureState>, NodeId) {
        self.frames.push(BuilderFrame::new());
        f(self);
        let children = Self::child_nodes(self.frames.pop().expect("disclosure child frame missing").nodes);
        let (state, node) = Disclosure::create(DisclosureParameters::header(label, expanded, children));
        let id = self.push_node(options, node);
        (state, id)
    }

    /// Adds an unkeyed state-owned tree disclosure.
    pub fn tree_node(&mut self, label: impl Into<String>, expanded: bool, f: impl FnOnce(&mut Self)) -> (crate::WidgetStateHandle<DisclosureState>, NodeId) {
        self.insert_tree_node(NodeOptions::new(), label, expanded, f)
    }

    fn insert_tree_node(
        &mut self,
        options: NodeOptions,
        label: impl Into<String>,
        expanded: bool,
        f: impl FnOnce(&mut Self),
    ) -> (crate::WidgetStateHandle<DisclosureState>, NodeId) {
        self.frames.push(BuilderFrame::new());
        f(self);
        let children = Self::child_nodes(self.frames.pop().expect("tree disclosure child frame missing").nodes);
        let (state, node) = Disclosure::create(DisclosureParameters::tree(label, expanded, children));
        let id = self.push_node(options, node);
        (state, id)
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
        self.push_legacy_group(
            options,
            Box::new(Row {
                widths: widths.to_vec(),
                height,
                children: Vec::new(),
            }),
            f,
        )
    }

    /// Adds an unkeyed grid flow group.
    pub fn grid(&mut self, widths: &[SizePolicy], heights: &[SizePolicy], f: impl FnOnce(&mut Self)) -> NodeId {
        self.insert_grid(NodeOptions::new(), widths, heights, f)
    }

    /// Adds a grid flow group with optional identity and placement metadata.
    fn insert_grid(&mut self, options: NodeOptions, widths: &[SizePolicy], heights: &[SizePolicy], f: impl FnOnce(&mut Self)) -> NodeId {
        self.frames.push(BuilderFrame::new());
        f(self);
        let frame = self.frames.pop().expect("grid frame missing");
        let items = frame
            .nodes
            .into_iter()
            .map(|child| GridItem::spanned(child.node, child.grid_span.columns(), child.grid_span.rows()));
        let (_state, node) = Grid::create(GridParameters::new(widths.iter().copied(), heights.iter().copied(), items));
        self.push_node(options, node)
    }

    /// Adds an unkeyed nested column scope.
    pub fn column(&mut self, f: impl FnOnce(&mut Self)) -> NodeId {
        self.insert_column(NodeOptions::new(), f)
    }

    /// Adds a nested column scope with optional identity and placement metadata.
    fn insert_column(&mut self, options: NodeOptions, f: impl FnOnce(&mut Self)) -> NodeId {
        self.frames.push(BuilderFrame::new());
        f(self);
        let children = Self::child_nodes(self.frames.pop().expect("column child frame missing").nodes);
        let (_state, node) = Column::create(ColumnParameters::new(children));
        self.push_node(options, node)
    }

    /// Adds an unkeyed stack scope.
    pub fn stack(&mut self, width: SizePolicy, height: SizePolicy, direction: StackDirection, f: impl FnOnce(&mut Self)) -> NodeId {
        self.insert_stack(NodeOptions::new(), width, height, direction, f)
    }

    /// Adds a stack scope with optional identity and placement metadata.
    fn insert_stack(&mut self, options: NodeOptions, width: SizePolicy, height: SizePolicy, direction: StackDirection, f: impl FnOnce(&mut Self)) -> NodeId {
        self.push_legacy_group(
            options,
            Box::new(Stack {
                width,
                height,
                direction,
                children: Vec::new(),
            }),
            f,
        )
    }

    /// Applies pre-insertion placement and pushes one already-owned node.
    fn push_node(&mut self, options: NodeOptions, node: crate::ui_node::Node) -> NodeId {
        let node = node.with_policy(options.policy);
        let id = node.id();
        self.current_frame_mut().nodes.push(BuilderChild { node, grid_span: options.grid_span });
        id
    }

    /// Temporary helper for Row/Stack/ScrollArea before their state-owned migrations.
    fn push_legacy_group(&mut self, options: NodeOptions, mut container: Box<dyn crate::ui_node::LegacyContainer>, f: impl FnOnce(&mut Self)) -> NodeId {
        self.frames.push(BuilderFrame::new());
        f(self);
        let frame = self.frames.pop().expect("child frame missing");
        let children = Self::child_nodes(frame.nodes);
        *container.children_mut() = children;
        self.push_node(options, crate::ui_node::Node::legacy_container(container))
    }

    /// Extracts owned child nodes from builder-only placement metadata.
    fn child_nodes(children: Vec<BuilderChild>) -> Vec<UiNode> {
        children.into_iter().map(|child| child.node).collect()
    }

    /// Returns the frame currently receiving new nodes.
    fn current_frame_mut(&mut self) -> &mut BuilderFrame {
        self.frames.last_mut().expect("ui node builder frame missing")
    }
}
