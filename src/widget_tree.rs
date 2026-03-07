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
// The widget tree provides a structured, per-frame description of UI content
// on top of the existing immediate-mode widget APIs. Callers use it to build
// rows, grids, stacks, headers, tree nodes, embedded containers, and leaf
// widgets into one executable tree, which `Container::build_tree` and
// `Container::widget_tree` then traverse through the normal widget/layout
// handling paths.

use std::{
    collections::hash_map::DefaultHasher,
    hash::{Hash, Hasher},
};

use crate::{
    widget_id_of, Container, ContainerHandle, ContainerOption, FrameResults, Id, Node, SizePolicy, StackDirection, Widget, WidgetBehaviourOption, WidgetId,
    WidgetRef,
};

/// Stable identifier assigned to a node built by [`WidgetTreeBuilder`].
pub type NodeId = Id;

/// Placement policy metadata attached to a tree node.
///
/// The current runtime tree preserves the crate's existing immediate layout
/// semantics, so parent flows still decide how space is allocated. These
/// policies are recorded on the tree for future measure/layout work and for
/// callers that want to tag nodes with intended sizing rules.
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

type TreeRun<'a> = Box<dyn FnMut(&mut Container, &mut FrameResults) + 'a>;

/// Kind of a node emitted by [`WidgetTreeBuilder`].
pub enum WidgetTreeNodeKind<'a> {
    /// Leaf node that dispatches a widget through the normal widget pipeline.
    Widget {
        /// Pointer identity of the widget state.
        widget_id: WidgetId,
        /// Borrowed widget state to dispatch.
        widget: WidgetRef<'a>,
    },
    /// Leaf node that executes arbitrary container code.
    ///
    /// This is the escape hatch for direct drawing, text helpers, and other
    /// container APIs that are not represented as `Widget` states.
    Run {
        /// Callback invoked during tree traversal.
        run: TreeRun<'a>,
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
        /// Header state object that owns the expanded/collapsed state.
        state: &'a mut Node,
    },
    /// Tree node with automatic indentation while expanded.
    Tree {
        /// Tree state object that owns the expanded/collapsed state.
        state: &'a mut Node,
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

impl<'a> WidgetTreeNodeKind<'a> {
    fn tag(&self) -> u8 {
        match self {
            Self::Widget { .. } => 1,
            Self::Run { .. } => 2,
            Self::Container { .. } => 3,
            Self::Header { .. } => 4,
            Self::Tree { .. } => 5,
            Self::Row { .. } => 6,
            Self::Grid { .. } => 7,
            Self::Column => 8,
            Self::Stack { .. } => 9,
        }
    }

    /// Returns the underlying widget identity when the node dispatches a widget.
    pub fn widget_id(&self) -> Option<WidgetId> {
        match self {
            Self::Widget { widget_id, .. } => Some(*widget_id),
            Self::Header { state } | Self::Tree { state } => Some(widget_id_of(&**state)),
            _ => None,
        }
    }
}

/// A single node in a widget tree.
pub struct WidgetTreeNode<'a> {
    id: NodeId,
    policy: Policy,
    kind: WidgetTreeNodeKind<'a>,
    children: Vec<WidgetTreeNode<'a>>,
}

impl<'a> WidgetTreeNode<'a> {
    /// Returns the stable node identifier.
    pub fn id(&self) -> NodeId {
        self.id
    }

    /// Returns the node's policy metadata.
    pub fn policy(&self) -> Policy {
        self.policy
    }

    /// Returns the node kind.
    pub fn kind(&self) -> &WidgetTreeNodeKind<'a> {
        &self.kind
    }

    /// Returns the node's child nodes.
    pub fn children(&self) -> &[WidgetTreeNode<'a>] {
        &self.children
    }

    pub(crate) fn parts_mut(&mut self) -> (&mut WidgetTreeNodeKind<'a>, &mut Vec<WidgetTreeNode<'a>>) {
        (&mut self.kind, &mut self.children)
    }
}

/// Completed widget tree built for a frame.
#[derive(Default)]
pub struct WidgetTree<'a> {
    roots: Vec<WidgetTreeNode<'a>>,
}

impl<'a> WidgetTree<'a> {
    /// Returns the root nodes of the tree.
    pub fn roots(&self) -> &[WidgetTreeNode<'a>] {
        &self.roots
    }

    /// Returns the root nodes mutably for execution or inspection.
    pub fn roots_mut(&mut self) -> &mut [WidgetTreeNode<'a>] {
        &mut self.roots
    }

    /// Consumes the tree and returns the owned root nodes.
    pub fn into_roots(self) -> Vec<WidgetTreeNode<'a>> {
        self.roots
    }
}

struct BuilderFrame<'a> {
    scope_seed: u64,
    next_auto: u64,
    nodes: Vec<WidgetTreeNode<'a>>,
}

impl<'a> BuilderFrame<'a> {
    fn root() -> Self {
        Self {
            scope_seed: 0x9e37_79b9_7f4a_7c15,
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

/// Builder that creates an executable tree of widgets, container nodes, and
/// layout scopes.
///
/// Unkeyed methods derive IDs from sibling order and remain stable only while
/// the surrounding structure stays in the same order. Keyed methods should be
/// used for dynamic or reorderable children.
pub struct WidgetTreeBuilder<'a> {
    frames: Vec<BuilderFrame<'a>>,
}

impl<'a> Default for WidgetTreeBuilder<'a> {
    fn default() -> Self {
        Self::new()
    }
}

impl<'a> WidgetTreeBuilder<'a> {
    /// Creates an empty builder with a root scope.
    pub fn new() -> Self {
        Self { frames: vec![BuilderFrame::root()] }
    }

    /// Builds a tree by executing `f` within a fresh builder.
    pub fn build(f: impl FnOnce(&mut Self)) -> WidgetTree<'a> {
        let mut builder = Self::new();
        f(&mut builder);
        builder.finish()
    }

    /// Finishes the builder and returns the resulting tree.
    pub fn finish(mut self) -> WidgetTree<'a> {
        debug_assert_eq!(self.frames.len(), 1, "widget tree builder scopes must be balanced");
        let frame = self.frames.pop().expect("root frame missing");
        WidgetTree { roots: frame.nodes }
    }

    /// Adds an unkeyed widget leaf node.
    pub fn widget<W: Widget + 'a>(&mut self, widget: &'a mut W) -> NodeId {
        self.widget_with_policy(Policy::auto(), widget)
    }

    /// Adds an unkeyed widget leaf node with explicit policy metadata.
    pub fn widget_with_policy<W: Widget + 'a>(&mut self, policy: Policy, widget: &'a mut W) -> NodeId {
        self.push_widget(policy, widget_id_of(widget), widget as WidgetRef<'a>, None::<u64>)
    }

    /// Adds a keyed widget leaf node.
    pub fn keyed_widget<K: Hash, W: Widget + 'a>(&mut self, key: K, widget: &'a mut W) -> NodeId {
        self.keyed_widget_with_policy(key, Policy::auto(), widget)
    }

    /// Adds a keyed widget leaf node with explicit policy metadata.
    pub fn keyed_widget_with_policy<K: Hash, W: Widget + 'a>(&mut self, key: K, policy: Policy, widget: &'a mut W) -> NodeId {
        self.push_widget(policy, widget_id_of(widget), widget as WidgetRef<'a>, Some(key))
    }

    /// Adds each widget reference in `runs` as a sibling widget node.
    pub fn widgets(&mut self, runs: &'a mut [WidgetRef<'a>]) {
        for widget in runs.iter_mut() {
            let widget_id = widget_id_of(&**widget);
            self.push_widget(Policy::auto(), widget_id, &mut **widget, None::<u64>);
        }
    }

    /// Adds an unkeyed callback leaf node.
    pub fn run(&mut self, f: impl FnMut(&mut Container, &mut FrameResults) + 'a) -> NodeId {
        self.run_with_policy(Policy::auto(), f)
    }

    /// Adds an unkeyed callback leaf node with explicit policy metadata.
    pub fn run_with_policy(&mut self, policy: Policy, f: impl FnMut(&mut Container, &mut FrameResults) + 'a) -> NodeId {
        self.push_leaf(policy, WidgetTreeNodeKind::Run { run: Box::new(f) }, None::<u64>)
    }

    /// Adds a keyed callback leaf node.
    pub fn keyed_run<K: Hash>(&mut self, key: K, f: impl FnMut(&mut Container, &mut FrameResults) + 'a) -> NodeId {
        self.keyed_run_with_policy(key, Policy::auto(), f)
    }

    /// Adds a keyed callback leaf node with explicit policy metadata.
    pub fn keyed_run_with_policy<K: Hash>(&mut self, key: K, policy: Policy, f: impl FnMut(&mut Container, &mut FrameResults) + 'a) -> NodeId {
        self.push_leaf(policy, WidgetTreeNodeKind::Run { run: Box::new(f) }, Some(key))
    }

    /// Adds a text label node that uses [`Container::label`] during traversal.
    pub fn label(&mut self, text: &'a str) -> NodeId {
        self.run(move |container, _results| container.label(text))
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
    pub fn header(&mut self, state: &'a mut Node, f: impl FnOnce(&mut Self)) -> NodeId {
        self.header_with_policy(Policy::auto(), state, f)
    }

    /// Adds an unkeyed collapsible header node with explicit policy metadata.
    pub fn header_with_policy(&mut self, policy: Policy, state: &'a mut Node, f: impl FnOnce(&mut Self)) -> NodeId {
        self.push_group(policy, WidgetTreeNodeKind::Header { state }, None::<u64>, f)
    }

    /// Adds a keyed collapsible header node.
    pub fn keyed_header<K: Hash>(&mut self, key: K, state: &'a mut Node, f: impl FnOnce(&mut Self)) -> NodeId {
        self.keyed_header_with_policy(key, Policy::auto(), state, f)
    }

    /// Adds a keyed collapsible header node with explicit policy metadata.
    pub fn keyed_header_with_policy<K: Hash>(&mut self, key: K, policy: Policy, state: &'a mut Node, f: impl FnOnce(&mut Self)) -> NodeId {
        self.push_group(policy, WidgetTreeNodeKind::Header { state }, Some(key), f)
    }

    /// Adds an unkeyed tree node that indents its children while expanded.
    pub fn treenode(&mut self, state: &'a mut Node, f: impl FnOnce(&mut Self)) -> NodeId {
        self.treenode_with_policy(Policy::auto(), state, f)
    }

    /// Adds an unkeyed tree node with explicit policy metadata.
    pub fn treenode_with_policy(&mut self, policy: Policy, state: &'a mut Node, f: impl FnOnce(&mut Self)) -> NodeId {
        self.push_group(policy, WidgetTreeNodeKind::Tree { state }, None::<u64>, f)
    }

    /// Adds a keyed tree node.
    pub fn keyed_treenode<K: Hash>(&mut self, key: K, state: &'a mut Node, f: impl FnOnce(&mut Self)) -> NodeId {
        self.keyed_treenode_with_policy(key, Policy::auto(), state, f)
    }

    /// Adds a keyed tree node with explicit policy metadata.
    pub fn keyed_treenode_with_policy<K: Hash>(&mut self, key: K, policy: Policy, state: &'a mut Node, f: impl FnOnce(&mut Self)) -> NodeId {
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

    fn push_widget<K: Hash>(&mut self, policy: Policy, widget_id: WidgetId, widget: WidgetRef<'a>, key: Option<K>) -> NodeId {
        self.push_leaf(policy, WidgetTreeNodeKind::Widget { widget_id, widget }, key)
    }

    fn push_leaf<K: Hash>(&mut self, policy: Policy, kind: WidgetTreeNodeKind<'a>, key: Option<K>) -> NodeId {
        let id = self.alloc_id(kind.tag(), key);
        self.current_frame_mut().nodes.push(WidgetTreeNode { id, policy, kind, children: Vec::new() });
        id
    }

    fn push_group<K: Hash>(&mut self, policy: Policy, kind: WidgetTreeNodeKind<'a>, key: Option<K>, f: impl FnOnce(&mut Self)) -> NodeId {
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

    fn current_frame_mut(&mut self) -> &mut BuilderFrame<'a> {
        self.frames.last_mut().expect("widget tree builder frame missing")
    }
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
        let mut button_a = Button::new("A");
        let mut button_b = Button::new("B");

        let tree_a = WidgetTreeBuilder::build(|builder| {
            builder.widget(&mut button_a);
            builder.widget(&mut button_b);
        });
        let tree_a_ids: Vec<NodeId> = tree_a.roots().iter().map(WidgetTreeNode::id).collect();
        drop(tree_a);
        let tree_b = WidgetTreeBuilder::build(|builder| {
            builder.widget(&mut button_a);
            builder.widget(&mut button_b);
        });
        let tree_b_ids: Vec<NodeId> = tree_b.roots().iter().map(WidgetTreeNode::id).collect();

        assert_eq!(tree_a_ids[0], tree_b_ids[0]);
        assert_eq!(tree_a_ids[1], tree_b_ids[1]);
    }

    #[test]
    fn keyed_widgets_keep_ids_across_reorder() {
        let mut button_a = Button::new("A");
        let mut button_b = Button::new("B");

        let tree_a = WidgetTreeBuilder::build(|builder| {
            builder.keyed_widget("a", &mut button_a);
            builder.keyed_widget("b", &mut button_b);
        });
        let ids_a: Vec<NodeId> = tree_a.roots().iter().map(WidgetTreeNode::id).collect();
        drop(tree_a);
        let tree_b = WidgetTreeBuilder::build(|builder| {
            builder.keyed_widget("b", &mut button_b);
            builder.keyed_widget("a", &mut button_a);
        });
        let ids_b: Vec<NodeId> = tree_b.roots().iter().map(WidgetTreeNode::id).collect();

        assert_eq!(ids_a[0], ids_b[1]);
        assert_eq!(ids_a[1], ids_b[0]);
    }

    #[test]
    fn row_nodes_capture_children_and_track_policy() {
        let mut button_a = Button::new("A");
        let mut button_b = Button::new("B");

        let tree = WidgetTreeBuilder::build(|builder| {
            builder.row_with_policy(
                Policy::fill(),
                &[SizePolicy::Fixed(40), SizePolicy::Remainder(0)],
                SizePolicy::Fixed(24),
                |builder| {
                    builder.widget(&mut button_a);
                    builder.widget(&mut button_b);
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
        let mut leaf = (crate::WidgetOption::NONE, WidgetBehaviourOption::NONE);

        let tree = WidgetTreeBuilder::build(|builder| {
            builder.container_with_policy(Policy::fill(), handle.clone(), ContainerOption::NONE, WidgetBehaviourOption::NONE, |builder| {
                builder.widget(&mut leaf);
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
