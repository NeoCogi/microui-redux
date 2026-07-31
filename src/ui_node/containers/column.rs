use std::{cell::RefCell, rc::Rc};

use crate::widget::{runtime_read_state, runtime_update_state};
use crate::{
    AtlasHandle, Dimensioni, Recti, Style, UiInputEvent, Widget, WidgetOption, WidgetPaintCtx, WidgetParameters, WidgetState, WidgetStateHandle,
    WidgetStateOwner, WidgetUpdateCtx,
};

use super::{Children, ChildrenVisitor, ChildrenVisitorMut, Container, ContainerBuilder, ContainerLayoutCtx, ContainerState, Node};

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
/// none can detach an attached node or lend the complete collection.
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
        runtime_read_state(&self.state, "Column::measure", |state| {
            let mut width = 0;
            let mut height: i32 = 0;
            for index in 0..state.children.len() {
                let child = state.children.measure_child(index, style, atlas, available).unwrap_or_default();
                width = width.max(child.width);
                height = height.saturating_add(child.height);
                if index + 1 < state.children.len() {
                    height = height.saturating_add(style.spacing);
                }
            }
            Dimensioni::new(width.max(0), height.max(0))
        })
    }

    fn update(&mut self, _ctx: &mut WidgetUpdateCtx<'_>, _input: Vec<UiInputEvent>) {}

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

fn layout_column(ctx: &mut ContainerLayoutCtx<'_>, children: &mut Children, rect: Recti) {
    let count = children.len();
    let spacing = ctx.style().spacing;
    let available_height = rect.height.saturating_sub(spacing.saturating_mul(count.saturating_sub(1) as i32));
    let mut preferred = Vec::with_capacity(count);
    let mut policies = Vec::with_capacity(count);
    for index in 0..count {
        let child_size = children
            .measure_child(index, ctx.style(), ctx.atlas(), Dimensioni::new(rect.width, available_height))
            .unwrap_or_default();
        preferred.push(child_size.height);
        policies.push(ctx.child_policy(children, index).map(|policy| policy.height).unwrap_or(crate::SizePolicy::Auto));
    }
    let placements = super::super::resolve_axis_placements(&policies, &preferred, available_height);
    let mut y = rect.y;
    for index in 0..count {
        let placement = placements.get(index).copied().unwrap_or_default();
        // `offered` is deliberately not always the final allocation. `layout_child` owns the one
        // application of the child's policy; `advance` only positions the following sibling.
        let child_rect = Recti::new(rect.x, y, rect.width, placement.offered);
        let _ = ctx.layout_child(children, index, child_rect);
        y = y.saturating_add(placement.advance).saturating_add(spacing);
    }
}
