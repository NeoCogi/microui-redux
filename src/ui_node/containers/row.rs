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

/// One-shot construction input for a horizontal row.
///
/// The initial children, index-matched width tracks, and shared item height are copied into
/// [`Row`] and remain mutable there after mounting.
pub struct RowParameters {
    children: Children,
    widths: Vec<SizePolicy>,
    item_height: SizePolicy,
}

impl WidgetParameters for RowParameters {}

impl RowParameters {
    /// Creates a row with index-matched width policies and one shared item height policy.
    pub fn new(widths: impl IntoIterator<Item = SizePolicy>, item_height: SizePolicy, children: impl IntoIterator<Item = Node>) -> Self {
        Self {
            children: children.into_iter().collect(),
            widths: widths.into_iter().collect(),
            item_height,
        }
    }
}

/// Application-facing state for a horizontal row.
///
/// This is the sole mounted authority for ordered membership, index-matched width tracks, and the
/// shared item-height policy. Missing width entries use [`SizePolicy::Auto`].
pub struct Row {
    /// Weak topology access kept separate from index-matched row configuration.
    children: ChildrenHandle,
    widths: Vec<SizePolicy>,
    item_height: SizePolicy,
}

impl Row {
    /// Returns the number of owned children.
    pub fn len(&self) -> Option<usize> {
        self.children.len()
    }
    /// Returns whether the row owns no children.
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

    /// Returns the index-matched row track policies.
    pub fn widths(&self) -> &[SizePolicy] {
        &self.widths
    }
    /// Replaces the row track policies.
    pub fn set_widths(&mut self, widths: impl IntoIterator<Item = SizePolicy>) {
        self.widths = widths.into_iter().collect();
    }
    /// Returns the shared item-height policy.
    pub fn item_height(&self) -> SizePolicy {
        self.item_height
    }
    /// Replaces the shared item-height policy.
    pub fn set_item_height(&mut self, height: SizePolicy) {
        self.item_height = height;
    }
    /// Creates a child-owning row and its weak typed widget handle.
    pub fn create(parameters: RowParameters) -> (TypedWidgetHandle<Self>, Node) {
        let children = Rc::new(RefCell::new(parameters.children));
        let widget = Self {
            children: ChildrenHandle::new(&children),
            widths: parameters.widths,
            item_height: parameters.item_height,
        };
        let (handle, container) = Container::from_shared(children, widget);
        (handle, Node::container(container))
    }
}

impl ContainerWidget for Row {
    fn measure(&self, ctx: &MeasureCtx<'_>, children: &Children, available: Dimensioni) -> Dimensioni {
        row_size(ctx, self, children, available)
    }

    fn place(&mut self, ctx: &mut ContainerLayoutCtx<'_>, children: &mut Children, rect: Recti) {
        layout_row(ctx, self, children, rect);
    }
}

impl Widget for Row {
    fn widget_opt(&self) -> &WidgetOption {
        &WidgetOption::NO_INTERACT
    }

    fn update(&mut self, _ctx: &mut WidgetUpdateCtx<'_>, _input: Option<&UiInputEvent>) {}

    fn paint(&mut self, _ctx: &mut WidgetPaintCtx<'_>) {}
}

/// Resolves shared row height and commits children from left to right.
///
/// Width tracks are replayed because the shared height must be known before any child is placed;
/// replaying them avoids allocating a temporary width collection on every layout frame.
fn layout_row(ctx: &mut ContainerLayoutCtx<'_>, state: &mut Row, children: &mut Children, rect: Recti) {
    // Resolve horizontal slots and vertical preference from one state snapshot. No child borrow or
    // parallel geometry collection survives this call.
    let count = children.len();
    let spacing = ctx.style().spacing.max(0);
    // gap_count = child_count - 1; spacing_total = spacing * gap_count.
    let gap_count = count.saturating_sub(1) as i32;
    let spacing_total = spacing.saturating_mul(gap_count);
    // track_width = max(container_width - spacing_total, 1).
    let available_width = rect.width.saturating_sub(spacing_total).max(1);
    // First resolve each width and measure content at that actual width. This is what keeps wrapped
    // child height consistent with the widths that layout will commit.
    let mut axis = row_axis(state, children, available_width, |index| {
        ctx.measure_child(children, index, Dimensioni::default()).unwrap_or_default().width
    });
    let mut height = 0;
    for index in 0..count {
        let policy = state.widths.get(index).copied().unwrap_or(SizePolicy::Auto);
        let preferred = ctx.measure_child(children, index, Dimensioni::default()).unwrap_or_default().width;
        let width = axis.next(policy, preferred).advance;
        let measured_width = children.child_policy(index).unwrap_or_else(crate::Policy::auto).width.measurement_bound(width);
        height = height.max(
            ctx.measure_child(children, index, Dimensioni::new(measured_width, 0))
                .unwrap_or_default()
                .height,
        );
    }
    height = state
        .item_height
        .preferred_extent(height.max(super::default_cell_height(ctx.style(), ctx.atlas())), rect.height);

    // Replay the allocation now that the single shared row height is known, placing each child as
    // soon as its width is resolved instead of collecting widths in a temporary Vec.
    let mut axis = row_axis(state, children, available_width, |index| {
        ctx.measure_child(children, index, Dimensioni::default()).unwrap_or_default().width
    });
    let mut x = rect.x;
    for index in 0..count {
        let policy = state.widths.get(index).copied().unwrap_or(SizePolicy::Auto);
        let preferred = ctx.measure_child(children, index, Dimensioni::default()).unwrap_or_default().width;
        let width = axis.next(policy, preferred).advance;
        let _ = ctx.layout_child(children, index, Recti::new(x, rect.y, width, height));
        // next_x = current_x + child_width + spacing.
        x = x.saturating_add(width).saturating_add(spacing);
    }
}

/// Builds the scalar width cursor from child preferences and index-matched Row track policies.
fn row_axis(state: &Row, children: &Children, available_width: i32, mut preferred_width: impl FnMut(usize) -> i32) -> Axis {
    // Axis stores scalar allocation totals only; individual widths are replayed when needed.
    Axis::new(
        available_width,
        (0..children.len()).map(|index| {
            let preferred = preferred_width(index);
            (state.widths.get(index).copied().unwrap_or(SizePolicy::Auto), preferred)
        }),
    )
}

/// Measures a Row using the same width-track resolution used during layout.
///
/// Children are remeasured at their resolved widths to obtain a correct shared height for wrapped
/// content. Placement policy remains parent-owned and is not folded into child content measurement.
fn row_size(ctx: &MeasureCtx<'_>, state: &Row, children: &Children, available: Dimensioni) -> Dimensioni {
    // Mirror placement policy and return only aggregate preferred geometry.
    let count = children.len();
    let spacing = ctx.style().spacing.max(0);
    // gap_count = child_count - 1; spacing_total = spacing * gap_count.
    let gap_count = count.saturating_sub(1) as i32;
    let spacing_total = spacing.saturating_mul(gap_count);
    let available_width = if available.width > 0 {
        // track_width = max(available_width - spacing_total, 1).
        available.width.saturating_sub(spacing_total).max(1)
    } else {
        0
    };
    // Resolve width tracks first; each resolved width then becomes the child's wrapping constraint.
    let mut axis = row_axis(state, children, available_width, |index| {
        ctx.measure_child(children, index, Dimensioni::default()).unwrap_or_default().width
    });
    let mut preferred_height = 0;
    for index in 0..count {
        let policy = state.widths.get(index).copied().unwrap_or(SizePolicy::Auto);
        let preferred = ctx.measure_child(children, index, Dimensioni::default()).unwrap_or_default().width;
        let width = children
            .child_policy(index)
            .unwrap_or_else(crate::Policy::auto)
            .width
            .measurement_bound(axis.next(policy, preferred).advance);
        preferred_height = preferred_height.max(ctx.measure_child(children, index, Dimensioni::new(width, 0)).unwrap_or_default().height);
    }
    // An empty or zero-height row retains the standard control-height fallback.
    preferred_height = preferred_height.max(super::default_cell_height(ctx.style(), ctx.atlas()));
    let height = state.item_height.preferred_extent(preferred_height, available.height);
    Dimensioni::new(axis.extent(count, spacing), height)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::test_support::test_atlas;
    use crate::{Custom, CustomParameters, Style};

    #[test]
    fn row_widget_exposes_topology_and_mutable_track_configuration() {
        let first = Custom::create(CustomParameters::new("first"));
        let (first_state, first) = Node::typed_widget(first);
        let (row, node) = Row::create(RowParameters::new([SizePolicy::Auto], SizePolicy::Auto, [first]));
        assert_eq!(row.try_read(Row::len), Some(Some(1)));

        row.try_update(|state| {
            state.set_widths([SizePolicy::Weight(1.0), SizePolicy::Weight(2.0)]);
            state.set_item_height(SizePolicy::Fixed(24));
            assert!(state.push(Node::widget(Custom::create(CustomParameters::new("second")))).is_ok());
        })
        .unwrap();
        assert_eq!(
            row.try_read(|state| state.widths().to_vec()),
            Some(vec![SizePolicy::Weight(1.0), SizePolicy::Weight(2.0)])
        );
        assert_eq!(row.try_read(Row::item_height), Some(SizePolicy::Fixed(24)));
        assert_eq!(row.try_read(Row::len), Some(Some(2)));

        assert_eq!(row.try_update(|state| state.remove_drop(0)), Some(Some(true)));
        assert!(!first_state.is_alive());
        drop(node);
        assert!(!row.is_alive());
    }

    #[test]
    fn row_measurement_and_bounded_allocation_share_track_sizing() {
        let style = Style { spacing: 3, ..Style::default() };
        let atlas = test_atlas();
        let ctx = MeasureCtx::new(&style, &atlas, 0);
        let children: Children = [
            Node::widget(Custom::create(CustomParameters::new("left"))),
            Node::widget(Custom::create(CustomParameters::new("right side"))),
        ]
        .into_iter()
        .collect();
        let topology = Rc::new(RefCell::new(Children::new()));
        let state = Row {
            children: ChildrenHandle::new(&topology),
            widths: vec![SizePolicy::Weight(1.0), SizePolicy::Weight(1.0)],
            item_height: SizePolicy::Auto,
        };
        let measured = row_size(&ctx, &state, &children, Dimensioni::default());
        let allocated = row_size(&ctx, &state, &children, measured);
        assert_eq!(allocated.width, measured.width);
        assert_eq!(allocated.height, measured.height);
    }
}
