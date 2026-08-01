use std::{cell::RefCell, rc::Rc};

use crate::sizing::SizePolicy;
use crate::widget::{runtime_read_state, runtime_update_state};
use crate::{
    AtlasHandle, Dimensioni, Recti, StackDirection, Style, UiInputEvent, Widget, WidgetOption, WidgetPaintCtx, WidgetParameters, WidgetState,
    WidgetStateHandle, WidgetStateOwner, WidgetUpdateCtx,
};

use super::{Children, ChildrenVisitor, ChildrenVisitorMut, Container, ContainerBuilder, ContainerLayoutCtx, ContainerState, Node};

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
        runtime_read_state(&self.state, "Stack::measure", |state| {
            let mut width = 0;
            let mut height: i32 = 0;
            for index in 0..state.children.len() {
                let size = state.children.measure_child(index, style, atlas, available).unwrap_or_default();
                width = width.max(super::super::resolve_size(state.item_width, size.width, available.width, available.width, None));
                height = height.saturating_add(super::super::resolve_size(
                    state.item_height,
                    size.height,
                    available.height,
                    available.height,
                    None,
                ));
                if index + 1 < state.children.len() {
                    height = height.saturating_add(style.spacing);
                }
            }
            Dimensioni::new(width.max(0), height.max(0))
        })
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

fn layout_stack(ctx: &mut ContainerLayoutCtx<'_>, state: &mut StackState, rect: Recti) {
    let count = state.children.len();
    let mut heights = Vec::with_capacity(count);
    for index in 0..count {
        let size = state
            .children
            .measure_child(index, ctx.style(), ctx.atlas(), Dimensioni::new(rect.width, rect.height))
            .unwrap_or_default();
        heights.push(super::super::resolve_size(state.item_height, size.height, rect.height, rect.height, None));
    }
    let width = super::super::resolve_size(state.item_width, rect.width, rect.width, rect.width, None);
    match state.direction {
        StackDirection::TopToBottom => {
            let mut y = rect.y;
            for (index, height) in heights.into_iter().enumerate() {
                let _ = ctx.layout_child(&mut state.children, index, Recti::new(rect.x, y, width, height));
                y = y.saturating_add(height).saturating_add(ctx.style().spacing);
            }
        }
        StackDirection::BottomToTop => {
            let mut y = rect.y.saturating_add(rect.height);
            for index in (0..count).rev() {
                let height = heights.get(index).copied().unwrap_or_default();
                y = y.saturating_sub(height);
                let _ = ctx.layout_child(&mut state.children, index, Recti::new(rect.x, y, width, height));
                y = y.saturating_sub(ctx.style().spacing);
            }
        }
    }
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
