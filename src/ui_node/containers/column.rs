use std::{cell::RefCell, rc::Rc};

use crate::widget::{runtime_read_state, runtime_update_state};
use crate::{
    AtlasHandle, Dimensioni, Recti, Style, UiInputEvent, Widget, WidgetOption, WidgetPaintCtx, WidgetParameters, WidgetState, WidgetStateHandle,
    WidgetStateOwner, WidgetUpdateCtx,
};

use super::{Axis, Children, ChildrenVisitor, ChildrenVisitorMut, Container, ContainerBuilder, ContainerLayoutCtx, ContainerState, Node};

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
    pub(super) children: Children,
}

impl WidgetState for ColumnState {}
impl ContainerState for ColumnState {}

impl ColumnState {
    /// Returns the number of owned child nodes.
    pub fn len(&self) -> usize {
        self.children.len()
    }

    /// Returns whether the column owns no children.
    pub fn is_empty(&self) -> bool {
        self.children.is_empty()
    }

    /// Appends one still-unmounted node.
    pub fn push(&mut self, node: Node) {
        self.children.push(node);
    }

    /// Inserts a node, returning it unchanged when `index > len`.
    #[allow(clippy::result_large_err)] // The exact unboxed owner is the failure value by contract.
    pub fn insert(&mut self, index: usize, node: Node) -> Result<(), Node> {
        self.children.insert(index, node)
    }

    /// Drops one indexed child owner and reports whether it existed.
    pub fn remove_drop(&mut self, index: usize) -> bool {
        self.children.remove_drop(index)
    }

    /// Drops every current child owner.
    pub fn clear(&mut self) {
        self.children.clear();
    }

    /// Replaces all children in iterator order and drops the previous owners.
    pub fn replace(&mut self, nodes: impl IntoIterator<Item = Node>) {
        self.children.replace(nodes);
    }
}

/// Concrete retained runtime for a vertical column.
///
/// This runtime is the sole persistent strong owner of `ColumnState`.
pub struct ColumnContainer {
    state: Rc<RefCell<ColumnState>>,
    opt: WidgetOption,
}

impl Widget for ColumnContainer {
    fn widget_opt(&self) -> &WidgetOption {
        &self.opt
    }

    fn measure(&self, style: &Style, atlas: &AtlasHandle, available: Dimensioni) -> Dimensioni {
        runtime_read_state(&self.state, "Column::measure", |state| measure_column(&state.children, style, atlas, available))
    }

    fn update(&mut self, _ctx: &mut WidgetUpdateCtx<'_>, _input: Option<&UiInputEvent>) {}

    fn paint(&mut self, _ctx: &mut WidgetPaintCtx<'_>) {}
}

impl WidgetStateOwner for ColumnContainer {
    type State = ColumnState;

    fn state_handle(&self) -> WidgetStateHandle<Self::State> {
        WidgetStateHandle::new(&self.state)
    }
}

impl Container for ColumnContainer {
    fn visit_children(&self, visitor: &mut ChildrenVisitor<'_>) {
        runtime_read_state(&self.state, "Column::visit_children", |state| {
            visitor.visit(&state.children);
        });
    }

    fn visit_children_mut(&mut self, visitor: &mut ChildrenVisitorMut<'_>) {
        runtime_update_state(&self.state, "Column::visit_children_mut", |state| {
            visitor.visit(&mut state.children);
        });
    }

    fn layout(&mut self, ctx: &mut ContainerLayoutCtx<'_>, rect: Recti) {
        runtime_update_state(&self.state, "Column::layout", |state| {
            layout_column(ctx, &mut state.children, rect);
        });
    }
}

/// Builder associating [`ColumnParameters`] with [`ColumnContainer`].
pub struct ColumnBuilder;

impl ContainerBuilder for ColumnBuilder {
    type Parameters = ColumnParameters;
    type W = ColumnContainer;

    fn create_container(parameters: Self::Parameters) -> Self::W {
        ColumnContainer {
            state: Rc::new(RefCell::new(ColumnState { children: parameters.children })),
            opt: WidgetOption::NONE,
        }
    }
}

/// Convenience constructor namespace for vertical columns.
pub struct Column;

impl Column {
    /// Creates a state-owned column and returns its weak state capability plus completed node.
    pub fn create(parameters: ColumnParameters) -> (WidgetStateHandle<ColumnState>, Node) {
        let container = ColumnBuilder::create_container(parameters);
        let state = container.state_handle();
        (state, Node::container(container))
    }
}

/// Resolves and commits a top-to-bottom child layout inside `rect`.
///
/// This is shared with Disclosure because an expanded disclosure body has exactly Column flow.
/// The function uses scalar replay instead of building per-frame policy and height vectors.
pub(super) fn layout_column(ctx: &mut ContainerLayoutCtx<'_>, children: &mut Children, rect: Recti) {
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
