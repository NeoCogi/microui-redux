use std::{cell::RefCell, rc::Rc};

use crate::sizing::SizePolicy;
use crate::widget::{runtime_read_state, runtime_update_state};
use crate::{
    AtlasHandle, Dimensioni, Recti, Style, UiInputEvent, Widget, WidgetOption, WidgetPaintCtx, WidgetParameters, WidgetState, WidgetStateHandle,
    WidgetStateOwner, WidgetUpdateCtx,
};

use super::{Children, ChildrenVisitor, ChildrenVisitorMut, Container, ContainerBuilder, ContainerLayoutCtx, ContainerState, Node};

/// One-shot construction input for a horizontal row.
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
        runtime_read_state(&self.state, "Row::measure", |state| {
            let mut preferred_widths = Vec::with_capacity(state.children.len());
            let mut preferred_height = 0;
            for index in 0..state.children.len() {
                let size = state.children.measure_child(index, style, atlas, available).unwrap_or_default();
                preferred_widths.push(size.width);
                preferred_height = preferred_height.max(size.height);
            }
            let spacing = style.spacing.saturating_mul(state.children.len().saturating_sub(1) as i32);
            let width = preferred_widths.into_iter().sum::<i32>().saturating_add(spacing).max(0);
            let height = super::super::resolve_size(
                state.item_height,
                preferred_height.max(super::super::default_cell_height(style, atlas)),
                available.height,
                available.height,
                None,
            );
            Dimensioni::new(width, height)
        })
    }

    fn update(&mut self, _ctx: &mut WidgetUpdateCtx<'_>, _input: Vec<UiInputEvent>) {}

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

fn layout_row(ctx: &mut ContainerLayoutCtx<'_>, state: &mut RowState, rect: Recti) {
    let count = state.children.len();
    let spacing = ctx.style().spacing.saturating_mul(count.saturating_sub(1) as i32);
    let available_width = rect.width.saturating_sub(spacing).max(0);
    let mut preferred = Vec::with_capacity(count);
    for index in 0..count {
        let size = state
            .children
            .measure_child(index, ctx.style(), ctx.atlas(), Dimensioni::new(available_width, rect.height))
            .unwrap_or_default();
        preferred.push(size.width);
    }
    let policies = (0..count)
        .map(|index| state.widths.get(index).copied().unwrap_or(SizePolicy::Auto))
        .collect::<Vec<_>>();
    let tracks = super::super::resolve_axis_tracks(&policies, &preferred, available_width);
    let height = super::super::resolve_size(state.item_height, rect.height, rect.height, rect.height, None);
    let mut x = rect.x;
    for (index, width) in tracks.into_iter().enumerate() {
        let _ = ctx.layout_child(&mut state.children, index, Recti::new(x, rect.y, width, height));
        x = x.saturating_add(width).saturating_add(ctx.style().spacing);
    }
}

#[cfg(test)]
mod tests {
    use super::*;
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
}
