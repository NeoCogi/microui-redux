use std::{cell::RefCell, rc::Rc};

use crate::ui_node::sizing::SizePolicy;
use crate::ui_node::{runtime_read_state, runtime_update_state};
use crate::{
    AtlasHandle, Dimensioni, Recti, Style, UiInputEvent, Widget, WidgetOption, WidgetPaintCtx, WidgetParameters, WidgetState, WidgetStateHandle,
    WidgetStateOwner, WidgetUpdateCtx,
};

use super::{Axis, Children, ChildrenVisitor, ChildrenVisitorMut, Container, ContainerBuilder, ContainerLayoutCtx, ContainerState, Node};

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
pub struct StackState {
    children: Children,
    item_width: SizePolicy,
    item_height: SizePolicy,
    direction: StackDirection,
}

impl WidgetState for StackState {}
impl ContainerState for StackState {}

impl StackState {
    /// Returns the number of owned children.
    pub fn len(&self) -> usize {
        self.children.len()
    }
    /// Returns whether the stack owns no children.
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
}

/// Concrete state-owning stack runtime.
pub struct StackContainer {
    state: Rc<RefCell<StackState>>,
    opt: WidgetOption,
}

impl Widget for StackContainer {
    fn widget_opt(&self) -> &WidgetOption {
        &self.opt
    }

    fn measure(&self, style: &Style, atlas: &AtlasHandle, available: Dimensioni) -> Dimensioni {
        runtime_read_state(&self.state, "Stack::measure", |state| stack_size(state, style, atlas, available))
    }

    fn update(&mut self, _ctx: &mut WidgetUpdateCtx<'_>, _input: Option<&UiInputEvent>) {}
    fn paint(&mut self, _ctx: &mut WidgetPaintCtx<'_>) {}
}

impl WidgetStateOwner for StackContainer {
    type State = StackState;
    fn state_handle(&self) -> WidgetStateHandle<Self::State> {
        WidgetStateHandle::new(&self.state)
    }
}

impl Container for StackContainer {
    fn visit_children(&self, visitor: &mut ChildrenVisitor<'_>) {
        runtime_read_state(&self.state, "Stack::visit_children", |state| visitor.visit(&state.children));
    }
    fn visit_children_mut(&mut self, visitor: &mut ChildrenVisitorMut<'_>) {
        runtime_update_state(&self.state, "Stack::visit_children_mut", |state| visitor.visit(&mut state.children));
    }
    fn layout(&mut self, ctx: &mut ContainerLayoutCtx<'_>, rect: Recti) {
        runtime_update_state(&self.state, "Stack::layout", |state| layout_stack(ctx, state, rect));
    }
}

/// Builder associating [`StackParameters`] with [`StackContainer`].
pub struct StackBuilder;

impl ContainerBuilder for StackBuilder {
    type Parameters = StackParameters;
    type W = StackContainer;
    fn create_container(parameters: Self::Parameters) -> Self::W {
        StackContainer {
            state: Rc::new(RefCell::new(StackState {
                children: parameters.children,
                item_width: parameters.item_width,
                item_height: parameters.item_height,
                direction: parameters.direction,
            })),
            opt: WidgetOption::NONE,
        }
    }
}

/// Convenience constructor namespace for directional stacks.
pub struct Stack;

impl Stack {
    /// Creates a state-owned stack and its weak application capability.
    pub fn create(parameters: StackParameters) -> (WidgetStateHandle<StackState>, Node) {
        let container = StackBuilder::create_container(parameters);
        let state = container.state_handle();
        (state, Node::container(container))
    }
}

/// Commits a vertical Stack in its configured traversal direction.
///
/// Stack differs from Column by applying one shared width policy and one shared height policy to
/// every child. Direction changes placement order only; sizing remains index-stable.
fn layout_stack(ctx: &mut ContainerLayoutCtx<'_>, state: &mut StackState, rect: Recti) {
    let count = state.children.len();
    let spacing = ctx.style().spacing.max(0);
    // Establish one item width from the widest intrinsic child before measuring wrapped heights.
    let preferred_width = (0..count)
        .map(|index| {
            state
                .children
                .measure_child(index, ctx.style(), ctx.atlas(), Dimensioni::default())
                .unwrap_or_default()
                .width
        })
        .max()
        .unwrap_or_default();
    let width = state.item_width.preferred_extent(preferred_width, rect.width);
    let spacing_total = spacing.saturating_mul(count.saturating_sub(1) as i32);
    let available_height = rect.height.saturating_sub(spacing_total).max(1);
    // The scalar axis holds only shared allocation totals; individual heights are replayed below.
    let mut axis = stack_axis(state, ctx.style(), ctx.atlas(), width, available_height);
    match state.direction {
        StackDirection::TopToBottom => {
            let mut y = rect.y;
            for index in 0..count {
                let preferred = stack_child_height(state, index, ctx.style(), ctx.atlas(), width);
                let height = axis.next(state.item_height, preferred).advance;
                let _ = ctx.layout_child(&mut state.children, index, Recti::new(rect.x, y, width, height));
                y = y.saturating_add(height).saturating_add(spacing);
            }
        }
        StackDirection::BottomToTop => {
            // Child zero is anchored at the bottom, followed by later children above it.
            let mut y = rect.y.saturating_add(rect.height);
            for index in 0..count {
                let preferred = stack_child_height(state, index, ctx.style(), ctx.atlas(), width);
                let height = axis.next(state.item_height, preferred).advance;
                y = y.saturating_sub(height);
                let _ = ctx.layout_child(&mut state.children, index, Recti::new(rect.x, y, width, height));
                y = y.saturating_sub(spacing);
            }
        }
    }
}

/// Measures one Stack child height at the shared item width.
///
/// The child's own width policy determines the content-measurement bound but is applied to final
/// geometry later by the generic node layout path.
fn stack_child_height(state: &StackState, index: usize, style: &Style, atlas: &AtlasHandle, width: i32) -> i32 {
    let child_width = state
        .children
        .child_policy(index)
        .unwrap_or_else(crate::Policy::auto)
        .width
        .measurement_bound(width);
    state
        .children
        .measure_child(index, style, atlas, Dimensioni::new(child_width, 0))
        .unwrap_or_default()
        .height
}

/// Builds the scalar vertical cursor for all Stack children at one resolved item width.
fn stack_axis(state: &StackState, style: &Style, atlas: &AtlasHandle, width: i32, available_height: i32) -> Axis {
    Axis::new(
        available_height,
        (0..state.children.len()).map(|index| (state.item_height, stack_child_height(state, index, style, atlas, width))),
    )
}

/// Measures the preferred Stack extent without mutating or retaining sizing results.
fn stack_size(state: &StackState, style: &Style, atlas: &AtlasHandle, available: Dimensioni) -> Dimensioni {
    let count = state.children.len();
    if count == 0 {
        return Dimensioni::default();
    }
    let spacing = style.spacing.max(0);
    // Width must be resolved before height because child text may wrap at the shared width.
    let preferred_width = (0..count)
        .map(|index| {
            state
                .children
                .measure_child(index, style, atlas, Dimensioni::default())
                .unwrap_or_default()
                .width
        })
        .max()
        .unwrap_or_default();
    let width = state.item_width.preferred_extent(preferred_width, available.width);
    let spacing_total = spacing.saturating_mul(count.saturating_sub(1) as i32);
    let available_height = if available.height > 0 {
        available.height.saturating_sub(spacing_total).max(1)
    } else {
        0
    };
    let mut axis = stack_axis(state, style, atlas, width, available_height);
    if available_height == 0 {
        // The construction pass already contains every intrinsic child height.
        return Dimensioni::new(width, axis.intrinsic_extent(count, spacing));
    }
    // Bounded policies require ordered replay so Remainder observes earlier siblings.
    for index in 0..count {
        axis.next(state.item_height, stack_child_height(state, index, style, atlas, width));
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
        assert_eq!(stack.try_read(StackState::direction), Some(StackDirection::BottomToTop));
        assert_eq!(stack.try_read(StackState::item_width), Some(SizePolicy::Remainder(0)));
        assert_eq!(stack.try_read(StackState::item_height), Some(SizePolicy::Fixed(28)));
        drop(node);
        assert!(!stack.is_alive());
    }
}
