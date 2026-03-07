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
// The retained widget tree owns the long-lived UI structure. Composite nodes
// such as headers, tree nodes, and embedded containers store their child lists
// here and keep stable NodeIds across frames. Each frame the container derives
// a short-lived runtime tree from these retained nodes, uses the previous-frame
// cache for structural pre-handle, then traverses the runtime tree through the
// normal layout and widget paths.

use std::{
    cell::RefCell,
    collections::HashMap,
    collections::hash_map::DefaultHasher,
    hash::{Hash, Hasher},
    rc::Rc,
};

use crate::{
    widget_id_of, AtlasHandle, Container, ContainerHandle, ContainerOption, ControlState, Custom, CustomRenderArgs, Dimensioni, FrameResults, Id, Node, Recti,
    ResourceState, SizePolicy, StackDirection, Style, TextWrap, Vec2i, Widget, WidgetBehaviourOption, WidgetCtx, WidgetId, WidgetOption,
};

/// Shared ownership handle for retained widget state.
pub type WidgetHandle<T> = Rc<RefCell<T>>;

/// Wraps widget state into a retained handle.
pub fn widget_handle<T>(value: T) -> WidgetHandle<T> {
    Rc::new(RefCell::new(value))
}

pub(crate) type TreeRun = Rc<RefCell<Box<dyn FnMut(&mut Container, &mut FrameResults) + 'static>>>;
pub(crate) type TreeCustomRender = Rc<RefCell<Box<dyn FnMut(Dimensioni, &CustomRenderArgs) + 'static>>>;

pub(crate) trait WidgetStateHandleDyn {
    fn widget_id(&self) -> WidgetId;
    fn effective_widget_opt(&self) -> WidgetOption;
    fn effective_behaviour_opt(&self) -> WidgetBehaviourOption;
    fn preferred_size(&self, style: &Style, atlas: &AtlasHandle, avail: Dimensioni) -> Dimensioni;
    fn needs_input_snapshot(&self) -> bool;
    fn handle(&self, ctx: &mut WidgetCtx<'_>, control: &ControlState) -> ResourceState;
}

struct WidgetStateHandle<W: Widget + 'static> {
    handle: WidgetHandle<W>,
}

impl<W: Widget + 'static> WidgetStateHandleDyn for WidgetStateHandle<W> {
    fn widget_id(&self) -> WidgetId {
        let widget = self.handle.borrow();
        widget_id_of(&*widget)
    }

    fn effective_widget_opt(&self) -> WidgetOption {
        let widget = self.handle.borrow();
        widget.effective_widget_opt()
    }

    fn effective_behaviour_opt(&self) -> WidgetBehaviourOption {
        let widget = self.handle.borrow();
        widget.effective_behaviour_opt()
    }

    fn preferred_size(&self, style: &Style, atlas: &AtlasHandle, avail: Dimensioni) -> Dimensioni {
        let widget = self.handle.borrow();
        widget.preferred_size(style, atlas, avail)
    }

    fn needs_input_snapshot(&self) -> bool {
        let widget = self.handle.borrow();
        widget.needs_input_snapshot()
    }

    fn handle(&self, ctx: &mut WidgetCtx<'_>, control: &ControlState) -> ResourceState {
        let mut widget = self.handle.borrow_mut();
        widget.handle(ctx, control)
    }
}

/// Stable identifier assigned to a retained or runtime node.
pub type NodeId = Id;

/// Cached per-frame data for a tree node keyed by [`NodeId`].
///
/// The cache is intentionally geometry-first. Parent nodes such as headers,
/// tree nodes, and embedded containers need the previous frame's rectangles to
/// react to structural input before the current frame's layout runs.
#[derive(Copy, Clone, Debug)]
pub struct NodeCacheEntry {
    /// Outer rectangle assigned to the node.
    pub rect: Recti,
    /// Inner body rectangle, when the node exposes one.
    pub body: Recti,
    /// Content size produced while traversing the node's children.
    pub content_size: Vec2i,
    /// Control state observed while handling the node this frame.
    pub control: ControlState,
    /// Resource state returned by the node this frame.
    pub result: ResourceState,
}

impl Default for NodeCacheEntry {
    fn default() -> Self {
        Self {
            rect: Recti::default(),
            body: Recti::default(),
            content_size: Vec2i::default(),
            control: ControlState::default(),
            result: ResourceState::NONE,
        }
    }
}

/// Previous/current frame cache for widget tree nodes.
///
/// `curr` is cleared at the start of each frame, populated while the runtime
/// tree runs, then swapped into `prev` at frame end.
#[derive(Default)]
pub struct WidgetTreeCache {
    prev: HashMap<NodeId, NodeCacheEntry>,
    curr: HashMap<NodeId, NodeCacheEntry>,
}

impl WidgetTreeCache {
    /// Clears the in-progress frame cache while preserving the previous frame.
    pub fn begin_frame(&mut self) {
        self.curr.clear();
    }

    /// Publishes the current frame cache as the previous frame for the next run.
    pub fn finish_frame(&mut self) {
        std::mem::swap(&mut self.prev, &mut self.curr);
        self.curr.clear();
    }

    /// Drops both previous and current cached node state.
    pub fn clear(&mut self) {
        self.prev.clear();
        self.curr.clear();
    }

    /// Returns the previous frame state for `node_id`.
    pub fn prev(&self, node_id: NodeId) -> Option<&NodeCacheEntry> {
        self.prev.get(&node_id)
    }

    /// Returns the current frame state for `node_id`.
    pub fn current(&self, node_id: NodeId) -> Option<&NodeCacheEntry> {
        self.curr.get(&node_id)
    }

    /// Records the current frame state for `node_id`.
    pub fn record(&mut self, node_id: NodeId, state: NodeCacheEntry) {
        let prev = self.curr.insert(node_id, state);
        debug_assert!(prev.is_none(), "Node {:?} was recorded more than once in the same frame", node_id);
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

/// Kind of a retained node emitted by [`WidgetTreeBuilder`].
pub(crate) enum WidgetTreeNodeKind {
    /// Leaf node that dispatches widget state through the normal widget pipeline.
    Widget {
        /// Retained widget state handle.
        widget: Box<dyn WidgetStateHandleDyn>,
    },
    /// Leaf node that behaves like [`Container::widget_custom_render`].
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
    fn tag(&self) -> u8 {
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
    id: NodeId,
    policy: Policy,
    kind: WidgetTreeNodeKind,
    children: Vec<WidgetTreeNode>,
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
    roots: Vec<WidgetTreeNode>,
}

impl WidgetTree {
    /// Returns the root nodes of the tree.
    pub fn roots(&self) -> &[WidgetTreeNode] {
        &self.roots
    }

    pub(crate) fn runtime_roots(&self) -> Vec<RuntimeTreeNode<'_>> {
        self.roots.iter().map(RuntimeTreeNode::from_retained).collect()
    }
}

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

/// Builder that creates a retained widget tree.
///
/// Unkeyed methods derive IDs from sibling order and remain stable only while
/// the surrounding structure stays in the same order. Keyed methods should be
/// used for dynamic or reorderable children.
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
        self.widget_with_policy(Policy::auto(), widget)
    }

    /// Adds an unkeyed widget leaf node with explicit policy metadata.
    pub fn widget_with_policy<W: Widget + 'static>(&mut self, policy: Policy, widget: WidgetHandle<W>) -> NodeId {
        self.push_leaf(
            policy,
            WidgetTreeNodeKind::Widget {
                widget: Box::new(WidgetStateHandle { handle: widget }),
            },
            None::<u64>,
        )
    }

    /// Adds a keyed widget leaf node.
    pub fn keyed_widget<K: Hash, W: Widget + 'static>(&mut self, key: K, widget: WidgetHandle<W>) -> NodeId {
        self.keyed_widget_with_policy(key, Policy::auto(), widget)
    }

    /// Adds a keyed widget leaf node with explicit policy metadata.
    pub fn keyed_widget_with_policy<K: Hash, W: Widget + 'static>(&mut self, key: K, policy: Policy, widget: WidgetHandle<W>) -> NodeId {
        self.push_leaf(
            policy,
            WidgetTreeNodeKind::Widget {
                widget: Box::new(WidgetStateHandle { handle: widget }),
            },
            Some(key),
        )
    }

    /// Adds an unkeyed callback leaf node.
    pub fn run(&mut self, f: impl FnMut(&mut Container, &mut FrameResults) + 'static) -> NodeId {
        self.run_with_policy(Policy::auto(), f)
    }

    /// Adds an unkeyed callback leaf node with explicit policy metadata.
    pub fn run_with_policy(&mut self, policy: Policy, f: impl FnMut(&mut Container, &mut FrameResults) + 'static) -> NodeId {
        let run: TreeRun = Rc::new(RefCell::new(Box::new(f)));
        self.push_leaf(policy, WidgetTreeNodeKind::Run { run }, None::<u64>)
    }

    /// Adds a keyed callback leaf node.
    pub fn keyed_run<K: Hash>(&mut self, key: K, f: impl FnMut(&mut Container, &mut FrameResults) + 'static) -> NodeId {
        self.keyed_run_with_policy(key, Policy::auto(), f)
    }

    /// Adds a keyed callback leaf node with explicit policy metadata.
    pub fn keyed_run_with_policy<K: Hash>(&mut self, key: K, policy: Policy, f: impl FnMut(&mut Container, &mut FrameResults) + 'static) -> NodeId {
        let run: TreeRun = Rc::new(RefCell::new(Box::new(f)));
        self.push_leaf(policy, WidgetTreeNodeKind::Run { run }, Some(key))
    }

    /// Adds a text label node that uses [`Container::label`] during traversal.
    pub fn label(&mut self, text: impl Into<String>) -> NodeId {
        let text = text.into();
        self.run(move |container, _results| container.label(text.as_str()))
    }

    /// Adds a text block that uses [`Container::text`] during traversal.
    pub fn text(&mut self, text: impl Into<String>) -> NodeId {
        let text = text.into();
        self.run(move |container, _results| container.text(text.as_str()))
    }

    /// Adds a wrapped text block that uses [`Container::text_with_wrap`].
    pub fn text_with_wrap(&mut self, text: impl Into<String>, wrap: TextWrap) -> NodeId {
        let text = text.into();
        self.run(move |container, _results| container.text_with_wrap(text.as_str(), wrap))
    }

    /// Adds a custom-render widget node.
    pub fn custom_render<F>(&mut self, state: WidgetHandle<Custom>, f: F) -> NodeId
    where
        F: FnMut(Dimensioni, &CustomRenderArgs) + 'static,
    {
        self.custom_render_with_policy(Policy::auto(), state, f)
    }

    /// Adds a custom-render widget node with explicit policy metadata.
    pub fn custom_render_with_policy<F>(&mut self, policy: Policy, state: WidgetHandle<Custom>, f: F) -> NodeId
    where
        F: FnMut(Dimensioni, &CustomRenderArgs) + 'static,
    {
        let render: TreeCustomRender = Rc::new(RefCell::new(Box::new(f)));
        self.push_leaf(policy, WidgetTreeNodeKind::CustomRender { state, render }, None::<u64>)
    }

    /// Adds an unkeyed embedded container node.
    pub fn container(&mut self, handle: ContainerHandle, opt: ContainerOption, behaviour: WidgetBehaviourOption, f: impl FnOnce(&mut Self)) -> NodeId {
        self.container_with_policy(Policy::auto(), handle, opt, behaviour, f)
    }

    /// Adds an embedded container node with explicit policy metadata.
    pub fn container_with_policy(
        &mut self,
        policy: Policy,
        handle: ContainerHandle,
        opt: ContainerOption,
        behaviour: WidgetBehaviourOption,
        f: impl FnOnce(&mut Self),
    ) -> NodeId {
        self.push_group(policy, WidgetTreeNodeKind::Container { handle, opt, behaviour }, None::<u64>, f)
    }

    /// Adds a keyed embedded container node.
    pub fn keyed_container<K: Hash>(
        &mut self,
        key: K,
        handle: ContainerHandle,
        opt: ContainerOption,
        behaviour: WidgetBehaviourOption,
        f: impl FnOnce(&mut Self),
    ) -> NodeId {
        self.keyed_container_with_policy(key, Policy::auto(), handle, opt, behaviour, f)
    }

    /// Adds a keyed embedded container node with explicit policy metadata.
    pub fn keyed_container_with_policy<K: Hash>(
        &mut self,
        key: K,
        policy: Policy,
        handle: ContainerHandle,
        opt: ContainerOption,
        behaviour: WidgetBehaviourOption,
        f: impl FnOnce(&mut Self),
    ) -> NodeId {
        self.push_group(policy, WidgetTreeNodeKind::Container { handle, opt, behaviour }, Some(key), f)
    }

    /// Adds an unkeyed collapsible header node.
    pub fn header(&mut self, state: WidgetHandle<Node>, f: impl FnOnce(&mut Self)) -> NodeId {
        self.header_with_policy(Policy::auto(), state, f)
    }

    /// Adds an unkeyed collapsible header node with explicit policy metadata.
    pub fn header_with_policy(&mut self, policy: Policy, state: WidgetHandle<Node>, f: impl FnOnce(&mut Self)) -> NodeId {
        self.push_group(policy, WidgetTreeNodeKind::Header { state }, None::<u64>, f)
    }

    /// Adds a keyed collapsible header node.
    pub fn keyed_header<K: Hash>(&mut self, key: K, state: WidgetHandle<Node>, f: impl FnOnce(&mut Self)) -> NodeId {
        self.keyed_header_with_policy(key, Policy::auto(), state, f)
    }

    /// Adds a keyed collapsible header node with explicit policy metadata.
    pub fn keyed_header_with_policy<K: Hash>(&mut self, key: K, policy: Policy, state: WidgetHandle<Node>, f: impl FnOnce(&mut Self)) -> NodeId {
        self.push_group(policy, WidgetTreeNodeKind::Header { state }, Some(key), f)
    }

    /// Adds an unkeyed tree node that indents its children while expanded.
    pub fn treenode(&mut self, state: WidgetHandle<Node>, f: impl FnOnce(&mut Self)) -> NodeId {
        self.treenode_with_policy(Policy::auto(), state, f)
    }

    /// Adds an unkeyed tree node with explicit policy metadata.
    pub fn treenode_with_policy(&mut self, policy: Policy, state: WidgetHandle<Node>, f: impl FnOnce(&mut Self)) -> NodeId {
        self.push_group(policy, WidgetTreeNodeKind::Tree { state }, None::<u64>, f)
    }

    /// Adds a keyed tree node.
    pub fn keyed_treenode<K: Hash>(&mut self, key: K, state: WidgetHandle<Node>, f: impl FnOnce(&mut Self)) -> NodeId {
        self.keyed_treenode_with_policy(key, Policy::auto(), state, f)
    }

    /// Adds a keyed tree node with explicit policy metadata.
    pub fn keyed_treenode_with_policy<K: Hash>(&mut self, key: K, policy: Policy, state: WidgetHandle<Node>, f: impl FnOnce(&mut Self)) -> NodeId {
        self.push_group(policy, WidgetTreeNodeKind::Tree { state }, Some(key), f)
    }

    /// Adds an unkeyed row flow group.
    pub fn row(&mut self, widths: &[SizePolicy], height: SizePolicy, f: impl FnOnce(&mut Self)) -> NodeId {
        self.row_with_policy(Policy::auto(), widths, height, f)
    }

    /// Adds an unkeyed row flow group with explicit policy metadata.
    pub fn row_with_policy(&mut self, policy: Policy, widths: &[SizePolicy], height: SizePolicy, f: impl FnOnce(&mut Self)) -> NodeId {
        self.push_group(policy, WidgetTreeNodeKind::Row { widths: widths.to_vec(), height }, None::<u64>, f)
    }

    /// Adds a keyed row flow group.
    pub fn keyed_row<K: Hash>(&mut self, key: K, widths: &[SizePolicy], height: SizePolicy, f: impl FnOnce(&mut Self)) -> NodeId {
        self.keyed_row_with_policy(key, Policy::auto(), widths, height, f)
    }

    /// Adds a keyed row flow group with explicit policy metadata.
    pub fn keyed_row_with_policy<K: Hash>(&mut self, key: K, policy: Policy, widths: &[SizePolicy], height: SizePolicy, f: impl FnOnce(&mut Self)) -> NodeId {
        self.push_group(policy, WidgetTreeNodeKind::Row { widths: widths.to_vec(), height }, Some(key), f)
    }

    /// Adds an unkeyed grid flow group.
    pub fn grid(&mut self, widths: &[SizePolicy], heights: &[SizePolicy], f: impl FnOnce(&mut Self)) -> NodeId {
        self.grid_with_policy(Policy::auto(), widths, heights, f)
    }

    /// Adds an unkeyed grid flow group with explicit policy metadata.
    pub fn grid_with_policy(&mut self, policy: Policy, widths: &[SizePolicy], heights: &[SizePolicy], f: impl FnOnce(&mut Self)) -> NodeId {
        self.push_group(
            policy,
            WidgetTreeNodeKind::Grid {
                widths: widths.to_vec(),
                heights: heights.to_vec(),
            },
            None::<u64>,
            f,
        )
    }

    /// Adds a keyed grid flow group.
    pub fn keyed_grid<K: Hash>(&mut self, key: K, widths: &[SizePolicy], heights: &[SizePolicy], f: impl FnOnce(&mut Self)) -> NodeId {
        self.keyed_grid_with_policy(key, Policy::auto(), widths, heights, f)
    }

    /// Adds a keyed grid flow group with explicit policy metadata.
    pub fn keyed_grid_with_policy<K: Hash>(
        &mut self,
        key: K,
        policy: Policy,
        widths: &[SizePolicy],
        heights: &[SizePolicy],
        f: impl FnOnce(&mut Self),
    ) -> NodeId {
        self.push_group(
            policy,
            WidgetTreeNodeKind::Grid {
                widths: widths.to_vec(),
                heights: heights.to_vec(),
            },
            Some(key),
            f,
        )
    }

    /// Adds an unkeyed nested column scope.
    pub fn column(&mut self, f: impl FnOnce(&mut Self)) -> NodeId {
        self.column_with_policy(Policy::auto(), f)
    }

    /// Adds an unkeyed nested column scope with explicit policy metadata.
    pub fn column_with_policy(&mut self, policy: Policy, f: impl FnOnce(&mut Self)) -> NodeId {
        self.push_group(policy, WidgetTreeNodeKind::Column, None::<u64>, f)
    }

    /// Adds a keyed nested column scope.
    pub fn keyed_column<K: Hash>(&mut self, key: K, f: impl FnOnce(&mut Self)) -> NodeId {
        self.keyed_column_with_policy(key, Policy::auto(), f)
    }

    /// Adds a keyed nested column scope with explicit policy metadata.
    pub fn keyed_column_with_policy<K: Hash>(&mut self, key: K, policy: Policy, f: impl FnOnce(&mut Self)) -> NodeId {
        self.push_group(policy, WidgetTreeNodeKind::Column, Some(key), f)
    }

    /// Adds an unkeyed stack scope.
    pub fn stack(&mut self, width: SizePolicy, height: SizePolicy, direction: StackDirection, f: impl FnOnce(&mut Self)) -> NodeId {
        self.stack_with_policy(Policy::auto(), width, height, direction, f)
    }

    /// Adds an unkeyed stack scope with explicit policy metadata.
    pub fn stack_with_policy(&mut self, policy: Policy, width: SizePolicy, height: SizePolicy, direction: StackDirection, f: impl FnOnce(&mut Self)) -> NodeId {
        self.push_group(policy, WidgetTreeNodeKind::Stack { width, height, direction }, None::<u64>, f)
    }

    /// Adds a keyed stack scope.
    pub fn keyed_stack<K: Hash>(&mut self, key: K, width: SizePolicy, height: SizePolicy, direction: StackDirection, f: impl FnOnce(&mut Self)) -> NodeId {
        self.keyed_stack_with_policy(key, Policy::auto(), width, height, direction, f)
    }

    /// Adds a keyed stack scope with explicit policy metadata.
    pub fn keyed_stack_with_policy<K: Hash>(
        &mut self,
        key: K,
        policy: Policy,
        width: SizePolicy,
        height: SizePolicy,
        direction: StackDirection,
        f: impl FnOnce(&mut Self),
    ) -> NodeId {
        self.push_group(policy, WidgetTreeNodeKind::Stack { width, height, direction }, Some(key), f)
    }

    fn push_leaf<K: Hash>(&mut self, policy: Policy, kind: WidgetTreeNodeKind, key: Option<K>) -> NodeId {
        let id = self.alloc_id(kind.tag(), key);
        self.current_frame_mut().nodes.push(WidgetTreeNode { id, policy, kind, children: Vec::new() });
        id
    }

    fn push_group<K: Hash>(&mut self, policy: Policy, kind: WidgetTreeNodeKind, key: Option<K>, f: impl FnOnce(&mut Self)) -> NodeId {
        let id = self.alloc_id(kind.tag(), key);
        self.frames.push(BuilderFrame::child(id.raw() as u64));
        f(self);
        let frame = self.frames.pop().expect("child frame missing");
        self.current_frame_mut().nodes.push(WidgetTreeNode { id, policy, kind, children: frame.nodes });
        id
    }

    fn alloc_id<K: Hash>(&mut self, tag: u8, key: Option<K>) -> NodeId {
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

pub(crate) struct RuntimeTreeNode<'a> {
    id: NodeId,
    kind: RuntimeTreeNodeKind<'a>,
    children: Vec<RuntimeTreeNode<'a>>,
}

impl<'a> RuntimeTreeNode<'a> {
    fn from_retained(node: &'a WidgetTreeNode) -> Self {
        let (_, kind, children) = node.parts();
        let kind = match kind {
            WidgetTreeNodeKind::Widget { widget } => RuntimeTreeNodeKind::Widget { widget: &**widget },
            WidgetTreeNodeKind::CustomRender { state, render } => RuntimeTreeNodeKind::CustomRender { state, render },
            WidgetTreeNodeKind::Run { run } => RuntimeTreeNodeKind::Run { run },
            WidgetTreeNodeKind::Container { handle, opt, behaviour } => RuntimeTreeNodeKind::Container {
                handle: handle.clone(),
                opt: *opt,
                behaviour: *behaviour,
            },
            WidgetTreeNodeKind::Header { state } => RuntimeTreeNodeKind::Header { state },
            WidgetTreeNodeKind::Tree { state } => RuntimeTreeNodeKind::Tree { state },
            WidgetTreeNodeKind::Row { widths, height } => RuntimeTreeNodeKind::Row { widths, height: *height },
            WidgetTreeNodeKind::Grid { widths, heights } => RuntimeTreeNodeKind::Grid { widths, heights },
            WidgetTreeNodeKind::Column => RuntimeTreeNodeKind::Column,
            WidgetTreeNodeKind::Stack { width, height, direction } => RuntimeTreeNodeKind::Stack {
                width: *width,
                height: *height,
                direction: *direction,
            },
        };
        Self {
            id: node.id(),
            kind,
            children: children.iter().map(Self::from_retained).collect(),
        }
    }

    pub(crate) fn id(&self) -> NodeId {
        self.id
    }

    pub(crate) fn kind(&self) -> &RuntimeTreeNodeKind<'a> {
        &self.kind
    }

    pub(crate) fn children(&self) -> &[RuntimeTreeNode<'a>] {
        &self.children
    }
}

pub(crate) enum RuntimeTreeNodeKind<'a> {
    Widget {
        widget: &'a dyn WidgetStateHandleDyn,
    },
    CustomRender {
        state: &'a WidgetHandle<Custom>,
        render: &'a TreeCustomRender,
    },
    Run {
        run: &'a TreeRun,
    },
    Container {
        handle: ContainerHandle,
        opt: ContainerOption,
        behaviour: WidgetBehaviourOption,
    },
    Header {
        state: &'a WidgetHandle<Node>,
    },
    Tree {
        state: &'a WidgetHandle<Node>,
    },
    Row {
        widths: &'a [SizePolicy],
        height: SizePolicy,
    },
    Grid {
        widths: &'a [SizePolicy],
        heights: &'a [SizePolicy],
    },
    Column,
    Stack {
        width: SizePolicy,
        height: SizePolicy,
        direction: StackDirection,
    },
}

#[cfg(test)]
mod tests {
    use crate::{AtlasHandle, AtlasSource, Button, CharEntry, Container, FontEntry, Input, Recti, SourceFormat, Style, Vec2i};
    use std::{cell::RefCell, rc::Rc};

    use super::*;

    const ICON_NAMES: [&str; 6] = ["white", "close", "expand", "collapse", "check", "expand_down"];

    fn make_test_atlas() -> AtlasHandle {
        let pixels: [u8; 4] = [0xFF, 0xFF, 0xFF, 0xFF];
        let icons: Vec<(&str, Recti)> = ICON_NAMES.iter().map(|name| (*name, Recti::new(0, 0, 1, 1))).collect();
        let entries = vec![
            (
                '_',
                CharEntry {
                    offset: Vec2i::new(0, 0),
                    advance: Vec2i::new(8, 0),
                    rect: Recti::new(0, 0, 1, 1),
                },
            ),
            (
                'a',
                CharEntry {
                    offset: Vec2i::new(0, 0),
                    advance: Vec2i::new(8, 0),
                    rect: Recti::new(0, 0, 1, 1),
                },
            ),
            (
                'b',
                CharEntry {
                    offset: Vec2i::new(0, 0),
                    advance: Vec2i::new(8, 0),
                    rect: Recti::new(0, 0, 1, 1),
                },
            ),
        ];
        let fonts = vec![(
            "default",
            FontEntry {
                line_size: 10,
                baseline: 8,
                font_size: 10,
                entries: &entries,
            },
        )];
        let source = AtlasSource {
            width: 1,
            height: 1,
            pixels: &pixels,
            icons: &icons,
            fonts: &fonts,
            format: SourceFormat::Raw,
            slots: &[],
        };
        AtlasHandle::from(&source)
    }

    #[test]
    fn unkeyed_widget_ids_are_stable_for_same_shape() {
        let button_a = widget_handle(Button::new("A"));
        let button_b = widget_handle(Button::new("B"));

        let tree_a = WidgetTreeBuilder::build(|builder| {
            builder.widget(button_a.clone());
            builder.widget(button_b.clone());
        });
        let tree_a_ids: Vec<NodeId> = tree_a.roots().iter().map(WidgetTreeNode::id).collect();
        let tree_b = WidgetTreeBuilder::build(|builder| {
            builder.widget(button_a.clone());
            builder.widget(button_b.clone());
        });
        let tree_b_ids: Vec<NodeId> = tree_b.roots().iter().map(WidgetTreeNode::id).collect();

        assert_eq!(tree_a_ids[0], tree_b_ids[0]);
        assert_eq!(tree_a_ids[1], tree_b_ids[1]);
    }

    #[test]
    fn keyed_widgets_keep_ids_across_reorder() {
        let button_a = widget_handle(Button::new("A"));
        let button_b = widget_handle(Button::new("B"));

        let tree_a = WidgetTreeBuilder::build(|builder| {
            builder.keyed_widget("a", button_a.clone());
            builder.keyed_widget("b", button_b.clone());
        });
        let ids_a: Vec<NodeId> = tree_a.roots().iter().map(WidgetTreeNode::id).collect();
        let tree_b = WidgetTreeBuilder::build(|builder| {
            builder.keyed_widget("b", button_b.clone());
            builder.keyed_widget("a", button_a.clone());
        });
        let ids_b: Vec<NodeId> = tree_b.roots().iter().map(WidgetTreeNode::id).collect();

        assert_eq!(ids_a[0], ids_b[1]);
        assert_eq!(ids_a[1], ids_b[0]);
    }

    #[test]
    fn row_nodes_capture_children_and_track_policy() {
        let button_a = widget_handle(Button::new("A"));
        let button_b = widget_handle(Button::new("B"));

        let tree = WidgetTreeBuilder::build(|builder| {
            builder.row_with_policy(
                Policy::fill(),
                &[SizePolicy::Fixed(40), SizePolicy::Remainder(0)],
                SizePolicy::Fixed(24),
                |builder| {
                    builder.widget(button_a.clone());
                    builder.widget(button_b.clone());
                },
            );
        });

        let row = &tree.roots()[0];
        assert_eq!(row.policy(), Policy::fill());
        assert_eq!(row.children().len(), 2);

        match row.kind() {
            WidgetTreeNodeKind::Row { widths, height } => {
                assert_eq!(widths, &[SizePolicy::Fixed(40), SizePolicy::Remainder(0)]);
                assert_eq!(*height, SizePolicy::Fixed(24));
            }
            _ => panic!("expected row node"),
        }
    }

    #[test]
    fn container_nodes_store_handle_and_children() {
        let atlas = make_test_atlas();
        let input = Rc::new(RefCell::new(Input::default()));
        let handle = ContainerHandle::new(Container::new("panel", atlas, Rc::new(Style::default()), input));
        let leaf = widget_handle((crate::WidgetOption::NONE, WidgetBehaviourOption::NONE));

        let tree = WidgetTreeBuilder::build(|builder| {
            builder.container_with_policy(Policy::fill(), handle.clone(), ContainerOption::NONE, WidgetBehaviourOption::NONE, |builder| {
                builder.widget(leaf.clone());
            });
        });

        let node = &tree.roots()[0];
        assert_eq!(node.children().len(), 1);
        match node.kind() {
            WidgetTreeNodeKind::Container { .. } => {}
            _ => panic!("expected container node"),
        }
    }

    #[test]
    fn run_nodes_are_recorded() {
        let tree = WidgetTreeBuilder::build(|builder| {
            builder.run(|_container, _results| {});
        });

        assert_eq!(tree.roots().len(), 1);
        match tree.roots()[0].kind() {
            WidgetTreeNodeKind::Run { .. } => {}
            _ => panic!("expected run node"),
        }
    }
}
