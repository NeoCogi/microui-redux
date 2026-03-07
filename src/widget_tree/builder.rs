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
    widget::{FrameResults, Widget},
    Container, ContainerHandle, Custom, CustomRenderArgs, Node, TextWrap,
};

use super::{erased_widget_state, NodeId, Policy, TreeCustomRender, TreeRun, WidgetHandle, WidgetTree, WidgetTreeNode, WidgetTreeNodeKind};

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
        self.push_leaf(policy, WidgetTreeNodeKind::Widget { widget: erased_widget_state(widget) }, None::<u64>)
    }

    /// Adds a keyed widget leaf node.
    pub fn keyed_widget<K: Hash, W: Widget + 'static>(&mut self, key: K, widget: WidgetHandle<W>) -> NodeId {
        self.keyed_widget_with_policy(key, Policy::auto(), widget)
    }

    /// Adds a keyed widget leaf node with explicit policy metadata.
    pub fn keyed_widget_with_policy<K: Hash, W: Widget + 'static>(&mut self, key: K, policy: Policy, widget: WidgetHandle<W>) -> NodeId {
        self.push_leaf(policy, WidgetTreeNodeKind::Widget { widget: erased_widget_state(widget) }, Some(key))
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
