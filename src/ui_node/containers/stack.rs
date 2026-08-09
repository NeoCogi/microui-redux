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

use crate::ui_node::sizing::SizePolicy;
use crate::ui_node::children::ChildrenHandle;
use crate::{
    Container, ContainerWidget, Dimensioni, MeasureCtx, Recti, TypedWidgetHandle, UiInputEvent, Widget, WidgetOption, WidgetPaintCtx, WidgetParameters,
    WidgetUpdateCtx,
};

use super::{Axis, Children, ContainerLayoutCtx, Node};

/// Direction used by stack flows when emitting vertical cells.
#[derive(Copy, Clone, Debug, PartialEq, Eq)]
pub enum StackDirection {
    /// Place cells from the current row start downward.
    TopToBottom,
    /// Place cells from the bottom of the current scope upward.
    BottomToTop,
}

impl Default for StackDirection {
    fn default() -> Self {
        Self::TopToBottom
    }
}

/// One-shot construction input for a directional stack.
pub struct StackParameters {
    children: Children,
    item_width: SizePolicy,
    item_height: SizePolicy,
    direction: StackDirection,
}

impl WidgetParameters for StackParameters {}

impl StackParameters {
    /// Creates a stack with shared item policies and ordered children.
    pub fn new(item_width: SizePolicy, item_height: SizePolicy, direction: StackDirection, children: impl IntoIterator<Item = Node>) -> Self {
        Self {
            children: children.into_iter().collect(),
            item_width,
            item_height,
            direction,
        }
    }
}

/// Application-facing state for a directional stack.
///
/// This is the sole mounted authority for ordered membership, shared item width/height policies,
/// and traversal direction.
pub struct Stack {
    /// Weak access to topology owned by the enclosing retained container.
    children: ChildrenHandle,
    item_width: SizePolicy,
    item_height: SizePolicy,
    direction: StackDirection,
}

impl Stack {
    /// Returns the number of owned children.
    pub fn len(&self) -> Option<usize> {
        self.children.len()
    }
    /// Returns whether the stack owns no children.
    pub fn is_empty(&self) -> Option<bool> {
        self.children.is_empty()
    }
    /// Appends one unmounted child.
    pub fn push(&mut self, node: Node) -> Result<(), Node> {
        self.children.try_push(node)
    }
    /// Inserts a child or returns it unchanged when `index > len`.
    #[allow(clippy::result_large_err)]
    pub fn insert(&mut self, index: usize, node: Node) -> Result<(), Node> {
        self.children.try_insert(index, node)
    }
    /// Drops one indexed child and reports whether it existed.
    pub fn remove_drop(&mut self, index: usize) -> Option<bool> {
        self.children.try_remove_drop(index)
    }
    /// Drops all children.
    pub fn clear(&mut self) -> Option<()> {
        self.children.try_clear()
    }
    /// Replaces all children in iterator order.
    pub fn replace<I>(&mut self, nodes: I) -> Result<(), I>
    where
        I: IntoIterator<Item = Node>,
    {
        self.children.try_replace(nodes)
    }

    /// Returns the shared item-width policy.
    pub fn item_width(&self) -> SizePolicy {
        self.item_width
    }
    /// Replaces the shared item-width policy.
    pub fn set_item_width(&mut self, width: SizePolicy) {
        self.item_width = width;
    }
    /// Returns the shared item-height policy.
    pub fn item_height(&self) -> SizePolicy {
        self.item_height
    }
    /// Replaces the shared item-height policy.
    pub fn set_item_height(&mut self, height: SizePolicy) {
        self.item_height = height;
    }
    /// Returns the current traversal direction.
    pub fn direction(&self) -> StackDirection {
        self.direction
    }
    /// Replaces the traversal direction without rebuilding children.
    pub fn set_direction(&mut self, direction: StackDirection) {
        self.direction = direction;
    }
    /// Creates a child-owning stack and its weak typed widget handle.
    pub fn create(parameters: StackParameters) -> (TypedWidgetHandle<Self>, Node) {
        let children = Rc::new(RefCell::new(parameters.children));
        let widget = Self {
            children: ChildrenHandle::new(&children),
            item_width: parameters.item_width,
            item_height: parameters.item_height,
            direction: parameters.direction,
        };
        let (handle, container) = Container::from_shared(children, widget);
        (handle, Node::container(container))
    }
}

impl ContainerWidget for Stack {
    fn measure(&self, ctx: &MeasureCtx<'_>, children: &Children, available: Dimensioni) -> Dimensioni {
        stack_size(ctx, self, children, available)
    }

    fn place(&mut self, ctx: &mut ContainerLayoutCtx<'_>, children: &mut Children, rect: Recti) {
        layout_stack(ctx, self, children, rect);
    }
}

impl Widget for Stack {
    fn widget_opt(&self) -> &WidgetOption {
        &WidgetOption::NO_INTERACT
    }

    fn update(&mut self, _ctx: &mut WidgetUpdateCtx<'_>, _input: Option<&UiInputEvent>) {}

    fn paint(&mut self, _ctx: &mut WidgetPaintCtx<'_>) {}
}

/// Commits a vertical Stack in its configured traversal direction.
///
/// Stack differs from Column by applying one shared width policy and one shared height policy to
/// every child. Direction changes placement order only; sizing remains index-stable.
fn layout_stack(ctx: &mut ContainerLayoutCtx<'_>, state: &mut Stack, children: &mut Children, rect: Recti) {
    // Direction changes traversal order only. Width and height remain index-stable, so changing
    // direction never remaps policies to different children.
    let count = children.len();
    let spacing = ctx.style().spacing.max(0);
    // Establish one item width from the widest intrinsic child before measuring wrapped heights.
    let preferred_width = (0..count)
        .map(|index| ctx.measure_child(children, index, Dimensioni::default()).unwrap_or_default().width)
        .max()
        .unwrap_or_default();
    let width = state.item_width.preferred_extent(preferred_width, rect.width);
    // gap_count = child_count - 1; spacing_total = spacing * gap_count.
    let gap_count = count.saturating_sub(1) as i32;
    let spacing_total = spacing.saturating_mul(gap_count);
    // track_height = max(container_height - spacing_total, 1).
    let available_height = rect.height.saturating_sub(spacing_total).max(1);
    // The scalar axis holds only shared allocation totals; individual heights are replayed below.
    let mut axis = stack_axis(state, count, available_height, |index| layout_stack_child_height(ctx, children, index, width));
    match state.direction {
        StackDirection::TopToBottom => {
            let mut y = rect.y;
            for index in 0..count {
                let preferred = layout_stack_child_height(ctx, children, index, width);
                let height = axis.next(state.item_height, preferred).advance;
                let _ = ctx.layout_child(children, index, Recti::new(rect.x, y, width, height));
                // next_y = current_y + child_height + spacing.
                y = y.saturating_add(height).saturating_add(spacing);
            }
        }
        StackDirection::BottomToTop => {
            // Child zero is anchored at the bottom, followed by later children above it.
            // bottom_y = container_y + container_height.
            let mut y = rect.y.saturating_add(rect.height);
            for index in 0..count {
                let preferred = layout_stack_child_height(ctx, children, index, width);
                let height = axis.next(state.item_height, preferred).advance;
                // child_y = previous_y - child_height.
                y = y.saturating_sub(height);
                let _ = ctx.layout_child(children, index, Recti::new(rect.x, y, width, height));
                // next_y = child_y - spacing.
                y = y.saturating_sub(spacing);
            }
        }
    }
}

/// Measures one Stack child height at the shared item width.
///
/// The child's own width policy determines the content-measurement bound but is applied to final
/// geometry later by the generic node layout path.
fn layout_stack_child_height(ctx: &ContainerLayoutCtx<'_>, children: &Children, index: usize, width: i32) -> i32 {
    // Measure with the resolved shared width so wrapping contributes the height placement will use.
    let child_width = children.child_policy(index).unwrap_or_else(crate::Policy::auto).width.measurement_bound(width);
    ctx.measure_child(children, index, Dimensioni::new(child_width, 0)).unwrap_or_default().height
}

/// Measures one Stack child height from the immutable measurement phase.
fn measure_stack_child_height(ctx: &MeasureCtx<'_>, children: &Children, index: usize, width: i32) -> i32 {
    let child_width = children.child_policy(index).unwrap_or_else(crate::Policy::auto).width.measurement_bound(width);
    ctx.measure_child(children, index, Dimensioni::new(child_width, 0)).unwrap_or_default().height
}

/// Builds the scalar vertical cursor for all Stack children at one resolved item width.
fn stack_axis(state: &Stack, count: usize, available_height: i32, mut preferred_height: impl FnMut(usize) -> i32) -> Axis {
    // Every child uses the same policy but contributes its own width-constrained preference.
    Axis::new(available_height, (0..count).map(|index| (state.item_height, preferred_height(index))))
}

/// Measures the preferred Stack extent without mutating or retaining sizing results.
fn stack_size(ctx: &MeasureCtx<'_>, state: &Stack, children: &Children, available: Dimensioni) -> Dimensioni {
    // Aggregate the same shared policies used by placement without retaining per-child geometry.
    let count = children.len();
    if count == 0 {
        return Dimensioni::default();
    }
    let spacing = ctx.style().spacing.max(0);
    // Width must be resolved before height because child text may wrap at the shared width.
    let preferred_width = (0..count)
        .map(|index| ctx.measure_child(children, index, Dimensioni::default()).unwrap_or_default().width)
        .max()
        .unwrap_or_default();
    let width = state.item_width.preferred_extent(preferred_width, available.width);
    // gap_count = child_count - 1; spacing_total = spacing * gap_count.
    let gap_count = count.saturating_sub(1) as i32;
    let spacing_total = spacing.saturating_mul(gap_count);
    let available_height = if available.height > 0 {
        // track_height = max(available_height - spacing_total, 1).
        available.height.saturating_sub(spacing_total).max(1)
    } else {
        0
    };
    let mut axis = stack_axis(state, count, available_height, |index| measure_stack_child_height(ctx, children, index, width));
    if available_height == 0 {
        // The construction pass already contains every intrinsic child height.
        return Dimensioni::new(width, axis.intrinsic_extent(count, spacing));
    }
    // Bounded policies require ordered replay so Remainder observes earlier siblings.
    for index in 0..count {
        axis.next(state.item_height, measure_stack_child_height(ctx, children, index, width));
    }
    Dimensioni::new(width, axis.extent(count, spacing))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn stack_direction_and_item_policies_mutate_without_rebuilding_children() {
        let (stack, node) = Stack::create(StackParameters::new(
            SizePolicy::Auto,
            SizePolicy::Fixed(20),
            StackDirection::TopToBottom,
            std::iter::empty(),
        ));
        stack
            .try_update(|state| {
                state.set_direction(StackDirection::BottomToTop);
                state.set_item_width(SizePolicy::Remainder(0));
                state.set_item_height(SizePolicy::Fixed(28));
            })
            .unwrap();
        assert_eq!(stack.try_read(Stack::direction), Some(StackDirection::BottomToTop));
        assert_eq!(stack.try_read(Stack::item_width), Some(SizePolicy::Remainder(0)));
        assert_eq!(stack.try_read(Stack::item_height), Some(SizePolicy::Fixed(28)));
        drop(node);
        assert!(!stack.is_alive());
    }
}
