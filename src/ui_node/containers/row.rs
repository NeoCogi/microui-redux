use std::{cell::RefCell, rc::Rc};

use crate::sizing::SizePolicy;
use crate::widget::{runtime_read_state, runtime_update_state};
use crate::{
    AtlasHandle, Dimensioni, Recti, Style, UiInputEvent, Widget, WidgetOption, WidgetPaintCtx, WidgetParameters, WidgetState, WidgetStateHandle,
    WidgetStateOwner, WidgetUpdateCtx,
};

use super::{Axis, Children, ChildrenVisitor, ChildrenVisitorMut, Container, ContainerBuilder, ContainerLayoutCtx, ContainerState, Node};

/// One-shot construction input for a horizontal row.
///
/// The initial children, index-matched width tracks, and shared item height are copied into
/// [`RowState`] and remain mutable there after mounting.
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
pub struct RowState {
    children: Children,
    widths: Vec<SizePolicy>,
    item_height: SizePolicy,
}

impl WidgetState for RowState {}
impl ContainerState for RowState {}

impl RowState {
    /// Returns the number of owned children.
    pub fn len(&self) -> usize {
        self.children.len()
    }
    /// Returns whether the row owns no children.
    pub fn is_empty(&self) -> bool {
        self.children.is_empty()
    }
    /// Appends one unmounted child.
    pub fn push(&mut self, node: Node) {
        self.children.push(node);
    }
    /// Inserts a child or returns it unchanged when `index > len`.
    #[allow(clippy::result_large_err)]
    pub fn insert(&mut self, index: usize, node: Node) -> Result<(), Node> {
        self.children.insert(index, node)
    }
    /// Drops one indexed child and reports whether it existed.
    pub fn remove_drop(&mut self, index: usize) -> bool {
        self.children.remove_drop(index)
    }
    /// Drops all children.
    pub fn clear(&mut self) {
        self.children.clear();
    }
    /// Replaces all children in iterator order.
    pub fn replace(&mut self, nodes: impl IntoIterator<Item = Node>) {
        self.children.replace(nodes);
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
}

/// Concrete state-owning row runtime.
pub struct RowContainer {
    state: Rc<RefCell<RowState>>,
    opt: WidgetOption,
}

impl Widget for RowContainer {
    fn widget_opt(&self) -> &WidgetOption {
        &self.opt
    }

    fn measure(&self, style: &Style, atlas: &AtlasHandle, available: Dimensioni) -> Dimensioni {
        runtime_read_state(&self.state, "Row::measure", |state| row_size(state, style, atlas, available))
    }

    fn update(&mut self, _ctx: &mut WidgetUpdateCtx<'_>, _input: Option<&UiInputEvent>) {}

    fn paint(&mut self, _ctx: &mut WidgetPaintCtx<'_>) {}
}

impl WidgetStateOwner for RowContainer {
    type State = RowState;
    fn state_handle(&self) -> WidgetStateHandle<Self::State> {
        WidgetStateHandle::new(&self.state)
    }
}

impl Container for RowContainer {
    fn visit_children(&self, visitor: &mut ChildrenVisitor<'_>) {
        runtime_read_state(&self.state, "Row::visit_children", |state| visitor.visit(&state.children));
    }

    fn visit_children_mut(&mut self, visitor: &mut ChildrenVisitorMut<'_>) {
        runtime_update_state(&self.state, "Row::visit_children_mut", |state| visitor.visit(&mut state.children));
    }

    fn layout(&mut self, ctx: &mut ContainerLayoutCtx<'_>, rect: Recti) {
        runtime_update_state(&self.state, "Row::layout", |state| layout_row(ctx, state, rect));
    }
}

/// Builder associating [`RowParameters`] with [`RowContainer`].
pub struct RowBuilder;

impl ContainerBuilder for RowBuilder {
    type Parameters = RowParameters;
    type W = RowContainer;

    fn create_container(parameters: Self::Parameters) -> Self::W {
        RowContainer {
            state: Rc::new(RefCell::new(RowState {
                children: parameters.children,
                widths: parameters.widths,
                item_height: parameters.item_height,
            })),
            opt: WidgetOption::NONE,
        }
    }
}

/// Convenience constructor namespace for horizontal rows.
pub struct Row;

impl Row {
    /// Creates a state-owned row and its weak application capability.
    pub fn create(parameters: RowParameters) -> (WidgetStateHandle<RowState>, Node) {
        let container = RowBuilder::create_container(parameters);
        let state = container.state_handle();
        (state, Node::container(container))
    }
}

/// Resolves shared row height and commits children from left to right.
///
/// Width tracks are replayed because the shared height must be known before any child is placed;
/// replaying them avoids allocating a temporary width collection on every layout frame.
fn layout_row(ctx: &mut ContainerLayoutCtx<'_>, state: &mut RowState, rect: Recti) {
    let count = state.children.len();
    let spacing = ctx.style().spacing.max(0);
    let spacing_total = spacing.saturating_mul(count.saturating_sub(1) as i32);
    let available_width = rect.width.saturating_sub(spacing_total).max(1);
    // First resolve each width and measure content at that actual width. This is what keeps wrapped
    // child height consistent with the widths that layout will commit.
    let mut axis = row_axis(state, ctx.style(), ctx.atlas(), available_width);
    let mut height = 0;
    for index in 0..count {
        let policy = state.widths.get(index).copied().unwrap_or(SizePolicy::Auto);
        let preferred = state
            .children
            .measure_child(index, ctx.style(), ctx.atlas(), Dimensioni::default())
            .unwrap_or_default()
            .width;
        let width = axis.next(policy, preferred).advance;
        let measured_width = state
            .children
            .child_policy(index)
            .unwrap_or_else(crate::Policy::auto)
            .width
            .measurement_bound(width);
        height = height.max(
            state
                .children
                .measure_child(index, ctx.style(), ctx.atlas(), Dimensioni::new(measured_width, 0))
                .unwrap_or_default()
                .height,
        );
    }
    height = state
        .item_height
        .preferred_extent(height.max(super::default_cell_height(ctx.style(), ctx.atlas())), rect.height);

    // Replay the allocation now that the single shared row height is known, placing each child as
    // soon as its width is resolved instead of collecting widths in a temporary Vec.
    let mut axis = row_axis(state, ctx.style(), ctx.atlas(), available_width);
    let mut x = rect.x;
    for index in 0..count {
        let policy = state.widths.get(index).copied().unwrap_or(SizePolicy::Auto);
        let preferred = state
            .children
            .measure_child(index, ctx.style(), ctx.atlas(), Dimensioni::default())
            .unwrap_or_default()
            .width;
        let width = axis.next(policy, preferred).advance;
        let _ = ctx.layout_child(&mut state.children, index, Recti::new(x, rect.y, width, height));
        x = x.saturating_add(width).saturating_add(spacing);
    }
}

/// Builds the scalar width cursor from child preferences and index-matched Row track policies.
fn row_axis(state: &RowState, style: &Style, atlas: &AtlasHandle, available_width: i32) -> Axis {
    Axis::new(
        available_width,
        (0..state.children.len()).map(|index| {
            let preferred = state
                .children
                .measure_child(index, style, atlas, Dimensioni::default())
                .unwrap_or_default()
                .width;
            (state.widths.get(index).copied().unwrap_or(SizePolicy::Auto), preferred)
        }),
    )
}

/// Measures a Row using the same width-track resolution used during layout.
///
/// Children are remeasured at their resolved widths to obtain a correct shared height for wrapped
/// content. Placement policy remains parent-owned and is not folded into child content measurement.
fn row_size(state: &RowState, style: &Style, atlas: &AtlasHandle, available: Dimensioni) -> Dimensioni {
    let count = state.children.len();
    let spacing = style.spacing.max(0);
    let spacing_total = spacing.saturating_mul(count.saturating_sub(1) as i32);
    let available_width = if available.width > 0 {
        available.width.saturating_sub(spacing_total).max(1)
    } else {
        0
    };
    // Resolve width tracks first; each resolved width then becomes the child's wrapping constraint.
    let mut axis = row_axis(state, style, atlas, available_width);
    let mut preferred_height = 0;
    for index in 0..count {
        let policy = state.widths.get(index).copied().unwrap_or(SizePolicy::Auto);
        let preferred = state
            .children
            .measure_child(index, style, atlas, Dimensioni::default())
            .unwrap_or_default()
            .width;
        let width = state
            .children
            .child_policy(index)
            .unwrap_or_else(crate::Policy::auto)
            .width
            .measurement_bound(axis.next(policy, preferred).advance);
        preferred_height = preferred_height.max(
            state
                .children
                .measure_child(index, style, atlas, Dimensioni::new(width, 0))
                .unwrap_or_default()
                .height,
        );
    }
    // An empty or zero-height row retains the standard control-height fallback.
    preferred_height = preferred_height.max(super::default_cell_height(style, atlas));
    let height = state.item_height.preferred_extent(preferred_height, available.height);
    Dimensioni::new(axis.extent(count, spacing), height)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::test_support::test_atlas;
    use crate::{Custom, CustomParameters};

    #[test]
    fn row_state_owns_topology_and_mutable_track_configuration() {
        let first = Custom::create(CustomParameters::new("first"));
        let first_state = first.state_handle();
        let (row, node) = Row::create(RowParameters::new([SizePolicy::Auto], SizePolicy::Auto, [Node::widget(first)]));
        assert_eq!(row.try_read(RowState::len), Some(1));

        row.try_update(|state| {
            state.set_widths([SizePolicy::Weight(1.0), SizePolicy::Weight(2.0)]);
            state.set_item_height(SizePolicy::Fixed(24));
            state.push(Node::widget(Custom::create(CustomParameters::new("second"))));
        })
        .unwrap();
        assert_eq!(
            row.try_read(|state| state.widths().to_vec()),
            Some(vec![SizePolicy::Weight(1.0), SizePolicy::Weight(2.0)])
        );
        assert_eq!(row.try_read(RowState::item_height), Some(SizePolicy::Fixed(24)));
        assert_eq!(row.try_read(RowState::len), Some(2));

        assert_eq!(row.try_update(|state| state.remove_drop(0)), Some(true));
        assert!(!first_state.is_alive());
        drop(node);
        assert!(!row.is_alive());
    }

    #[test]
    fn row_measurement_and_bounded_allocation_share_track_sizing() {
        let style = Style { spacing: 3, ..Style::default() };
        let state = RowState {
            children: [
                Node::widget(Custom::create(CustomParameters::new("left"))),
                Node::widget(Custom::create(CustomParameters::new("right side"))),
            ]
            .into_iter()
            .collect(),
            widths: vec![SizePolicy::Weight(1.0), SizePolicy::Weight(1.0)],
            item_height: SizePolicy::Auto,
        };
        let measured = row_size(&state, &style, &test_atlas(), Dimensioni::default());
        let allocated = row_size(&state, &style, &test_atlas(), measured);
        assert_eq!(allocated.width, measured.width);
        assert_eq!(allocated.height, measured.height);
    }
}
