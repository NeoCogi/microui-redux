//
// Copyright 2023-Present (c) Raja Lehtihet & Wael El Oraiby
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

use std::{cell::RefCell, rc::Rc};

use crate::ui_node::children::ChildrenHandle;
use crate::{AtlasHandle, Container, Dimensioni, Layout, Recti, Style, WidgetOption, WidgetParameters, WidgetState, WidgetStateHandle};

use super::{Axis, Children, ContainerLayoutCtx, Node};

/// One-shot construction input for a vertical [`Column`].
#[derive(Default)]
pub struct ColumnParameters {
    children: Children,
}

impl WidgetParameters for ColumnParameters {}

impl ColumnParameters {
    /// Creates a column that owns `children` in iterator order.
    pub fn new(children: impl IntoIterator<Item = Node>) -> Self {
        Self { children: children.into_iter().collect() }
    }
}

/// Application-facing state for a vertical column.
///
/// Child ownership is private. The inherent methods commit unique nodes or drop existing owners;
/// none can detach an attached node or lend the complete collection. Ordered membership is the
/// Column's complete mounted configuration; spacing remains Style-owned.
pub struct ColumnState {
    /// Weak topology capability; the concrete container remains the only strong child owner.
    children: ChildrenHandle,
}

impl WidgetState for ColumnState {}

impl ColumnState {
    /// Returns the number of owned child nodes.
    pub fn len(&self) -> Option<usize> {
        self.children.len()
    }

    /// Returns whether the column owns no children.
    pub fn is_empty(&self) -> Option<bool> {
        self.children.is_empty()
    }

    /// Appends one still-unmounted node.
    pub fn push(&mut self, node: Node) -> Result<(), Node> {
        self.children.try_push(node)
    }

    /// Inserts a node, returning it unchanged when `index > len`.
    #[allow(clippy::result_large_err)] // The exact unboxed owner is the failure value by contract.
    pub fn insert(&mut self, index: usize, node: Node) -> Result<(), Node> {
        self.children.try_insert(index, node)
    }

    /// Drops one indexed child owner and reports whether it existed.
    pub fn remove_drop(&mut self, index: usize) -> Option<bool> {
        self.children.try_remove_drop(index)
    }

    /// Drops every current child owner.
    pub fn clear(&mut self) -> Option<()> {
        self.children.try_clear()
    }

    /// Replaces all children in iterator order and drops the previous owners.
    pub fn replace<I>(&mut self, nodes: I) -> Result<(), I>
    where
        I: IntoIterator<Item = Node>,
    {
        self.children.try_replace(nodes)
    }
}

/// Geometry-only policy for a vertical column.
pub struct ColumnLayout {
    _state: Rc<RefCell<ColumnState>>,
}

impl Layout for ColumnLayout {
    fn measure(&self, children: &Children, style: &Style, atlas: &AtlasHandle, available: Dimensioni) -> Dimensioni {
        // Column geometry depends only on the authoritative child sequence and shared Style.
        measure_column(children, style, atlas, available)
    }

    fn place(&mut self, ctx: &mut ContainerLayoutCtx<'_>, children: &mut Children, rect: Recti) {
        // Delegate to the shared flow used by Disclosure bodies so both paths remain identical.
        layout_column(ctx, children, rect);
    }
}

/// Convenience constructor namespace for vertical columns.
pub struct Column;

impl Column {
    /// Creates a state-owned column and returns its weak state capability plus completed node.
    ///
    /// The child cell is allocated first because [`ColumnState`] needs a weak mutation capability
    /// for that exact collection. The layout retains the strong state allocation, while the
    /// completed [`Container`] becomes the only persistent strong owner of the children.
    pub fn create(parameters: ColumnParameters) -> (WidgetStateHandle<ColumnState>, Node) {
        // Prepare the single child allocation shared by traversal and weak typed-state mutation.
        let children = Rc::new(RefCell::new(parameters.children));
        // State owns no nodes; its handle expires as soon as the enclosing layout is dropped.
        let state = Rc::new(RefCell::new(ColumnState { children: ChildrenHandle::new(&children) }));
        // Capture the public weak handle before moving the strong state owner into the layout.
        let handle = WidgetStateHandle::new(&state);
        // Move child and state ownership into one concrete Container, then finish the owning Node.
        let container = Container::from_shared(children, ColumnLayout { _state: state }, WidgetOption::NONE);
        (handle, Node::container(container))
    }
}

/// Resolves and commits a top-to-bottom child layout inside `rect`.
///
/// This is shared with Disclosure because an expanded disclosure body has exactly Column flow.
/// The function uses scalar replay instead of building per-frame policy and height vectors.
pub(super) fn layout_column(ctx: &mut ContainerLayoutCtx<'_>, children: &mut Children, rect: Recti) {
    // Spacing consumes room between tracks, never inside a child's allocated rectangle.
    let spacing = ctx.style().spacing.max(0);
    let count = children.len();
    // Spacing is outside track allocation, so children divide only the remaining height.
    let spacing_total = spacing.saturating_mul(count.saturating_sub(1) as i32);
    let available_height = rect.height.saturating_sub(spacing_total).max(1);

    // First pass: summarize policies and preferred heights without retaining per-child data.
    let mut axis = Axis::new(
        available_height,
        (0..count).map(|index| {
            let policy = children.child_policy(index).unwrap_or_else(crate::Policy::auto);
            let width = policy.width.measurement_bound(rect.width.max(1));
            let preferred = children
                .measure_child(index, ctx.style(), ctx.atlas(), Dimensioni::new(width, available_height))
                .unwrap_or_default()
                .height;
            (policy.height, preferred)
        }),
    );

    // Second pass: resolve each ordered slot and commit it immediately. `offered` may differ from
    // `advance` because child layout remains responsible for applying the node policy once.
    let mut y = rect.y;
    for index in 0..count {
        let policy = children.child_policy(index).unwrap_or_else(crate::Policy::auto);
        let width = policy.width.measurement_bound(rect.width.max(1));
        let preferred = children
            .measure_child(index, ctx.style(), ctx.atlas(), Dimensioni::new(width, available_height))
            .unwrap_or_default()
            .height;
        let slot = axis.next(policy.height, preferred);
        let child_rect = Recti::new(rect.x, y, rect.width, slot.offered);
        let _ = ctx.layout_child(children, index, child_rect);
        y = y.saturating_add(slot.advance).saturating_add(spacing);
    }
}

/// Measures the preferred extent of a top-to-bottom child sequence.
///
/// Width is the widest policy-adjusted child. Height uses the same ordered axis allocation as
/// layout, including spacing, but does not retain or mutate any sizing state.
pub(super) fn measure_column(children: &Children, style: &Style, atlas: &AtlasHandle, available: Dimensioni) -> Dimensioni {
    // Use the same spacing and track policy math as placement so preferred and committed geometry
    // cannot disagree when the parent supplies a finite height.
    let spacing = style.spacing.max(0);
    let count = children.len();
    let mut width = 0;
    // A positive bound is divided among tracks after spacing; zero stays the intrinsic marker.
    let spacing_total = spacing.saturating_mul(count.saturating_sub(1) as i32);
    let available_height = if available.height > 0 {
        available.height.saturating_sub(spacing_total).max(1)
    } else {
        0
    };
    // The summary pass also computes the widest child while it gathers vertical track inputs.
    let mut axis = Axis::new(
        available_height,
        (0..count).map(|index| {
            let policy = children.child_policy(index).unwrap_or_else(crate::Policy::auto);
            let child_width = policy.width.measurement_bound(available.width);
            let child = children.measure_child(index, style, atlas, Dimensioni::new(child_width, 0)).unwrap_or_default();
            width = width.max(policy.width.preferred_extent(child.width, available.width));
            (policy.height, child.height)
        }),
    );
    if available_height == 0 {
        // Axis::new already accumulated the intrinsic total, so unbounded auto-size needs no replay.
        return Dimensioni::new(width, axis.intrinsic_extent(count, spacing));
    }

    // Bounded policies such as Remainder depend on sibling order and are replayed through the
    // scalar cursor. No child result escapes this query or becomes mutable widget state.
    for index in 0..count {
        let policy = children.child_policy(index).unwrap_or_else(crate::Policy::auto);
        let child_width = policy.width.measurement_bound(available.width);
        let preferred = children
            .measure_child(index, style, atlas, Dimensioni::new(child_width, 0))
            .unwrap_or_default()
            .height;
        axis.next(policy.height, preferred);
    }
    Dimensioni::new(width, axis.extent(count, spacing))
}
