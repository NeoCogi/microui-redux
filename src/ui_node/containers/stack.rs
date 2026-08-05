use std::{cell::RefCell, rc::Rc};

use crate::ui_node::sizing::SizePolicy;
use crate::ui_node::children::ChildrenHandle;
use crate::{AtlasHandle, Container, Dimensioni, Layout, Recti, Style, WidgetOption, WidgetParameters, WidgetState, WidgetStateHandle};

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
pub struct StackState {
    /// Weak access to topology owned by the enclosing retained container.
    children: ChildrenHandle,
    item_width: SizePolicy,
    item_height: SizePolicy,
    direction: StackDirection,
}

impl WidgetState for StackState {}

impl StackState {
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
}

/// Geometry-only policy for a directional stack.
pub struct StackLayout {
    state: Rc<RefCell<StackState>>,
}

impl Layout for StackLayout {
    fn measure(&self, children: &Children, style: &Style, atlas: &AtlasHandle, available: Dimensioni) -> Dimensioni {
        // Shared item policies and direction live in typed state; borrow them for this measurement.
        crate::ui_node::runtime_read_state(&self.state, "Stack::measure", |state| stack_size(state, children, style, atlas, available))
    }

    fn place(&mut self, ctx: &mut ContainerLayoutCtx<'_>, children: &mut Children, rect: Recti) {
        // One checked state borrow covers policy resolution and every indexed child placement.
        crate::ui_node::runtime_update_state(&self.state, "Stack::place", |state| layout_stack(ctx, state, children, rect));
    }
}

/// Convenience constructor namespace for directional stacks.
pub struct Stack;

impl Stack {
    /// Creates a state-owned stack and its weak application capability.
    ///
    /// The container owns the nodes; typed state owns configuration plus a weak topology route.
    /// Dropping the returned node therefore expires every cloned state handle without requiring a
    /// separate owner wrapper.
    pub fn create(parameters: StackParameters) -> (WidgetStateHandle<StackState>, Node) {
        // Establish the final child allocation before creating its weak mutation capability.
        let children = Rc::new(RefCell::new(parameters.children));
        let state = Rc::new(RefCell::new(StackState {
            children: ChildrenHandle::new(&children),
            item_width: parameters.item_width,
            item_height: parameters.item_height,
            direction: parameters.direction,
        }));
        // Capture a weak application handle, then retain the strong state reference in StackLayout.
        let handle = WidgetStateHandle::new(&state);
        let container = Container::from_shared(children, StackLayout { state }, WidgetOption::NONE);
        (handle, Node::container(container))
    }
}

/// Commits a vertical Stack in its configured traversal direction.
///
/// Stack differs from Column by applying one shared width policy and one shared height policy to
/// every child. Direction changes placement order only; sizing remains index-stable.
fn layout_stack(ctx: &mut ContainerLayoutCtx<'_>, state: &mut StackState, children: &mut Children, rect: Recti) {
    // Direction changes traversal order only. Width and height remain index-stable, so changing
    // direction never remaps policies to different children.
    let count = children.len();
    let spacing = ctx.style().spacing.max(0);
    // Establish one item width from the widest intrinsic child before measuring wrapped heights.
    let preferred_width = (0..count)
        .map(|index| {
            children
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
    let mut axis = stack_axis(state, children, ctx.style(), ctx.atlas(), width, available_height);
    match state.direction {
        StackDirection::TopToBottom => {
            let mut y = rect.y;
            for index in 0..count {
                let preferred = stack_child_height(children, index, ctx.style(), ctx.atlas(), width);
                let height = axis.next(state.item_height, preferred).advance;
                let _ = ctx.layout_child(children, index, Recti::new(rect.x, y, width, height));
                y = y.saturating_add(height).saturating_add(spacing);
            }
        }
        StackDirection::BottomToTop => {
            // Child zero is anchored at the bottom, followed by later children above it.
            let mut y = rect.y.saturating_add(rect.height);
            for index in 0..count {
                let preferred = stack_child_height(children, index, ctx.style(), ctx.atlas(), width);
                let height = axis.next(state.item_height, preferred).advance;
                y = y.saturating_sub(height);
                let _ = ctx.layout_child(children, index, Recti::new(rect.x, y, width, height));
                y = y.saturating_sub(spacing);
            }
        }
    }
}

/// Measures one Stack child height at the shared item width.
///
/// The child's own width policy determines the content-measurement bound but is applied to final
/// geometry later by the generic node layout path.
fn stack_child_height(children: &Children, index: usize, style: &Style, atlas: &AtlasHandle, width: i32) -> i32 {
    // Measure with the resolved shared width so wrapping contributes the height placement will use.
    let child_width = children.child_policy(index).unwrap_or_else(crate::Policy::auto).width.measurement_bound(width);
    children
        .measure_child(index, style, atlas, Dimensioni::new(child_width, 0))
        .unwrap_or_default()
        .height
}

/// Builds the scalar vertical cursor for all Stack children at one resolved item width.
fn stack_axis(state: &StackState, children: &Children, style: &Style, atlas: &AtlasHandle, width: i32, available_height: i32) -> Axis {
    // Every child uses the same policy but contributes its own width-constrained preference.
    Axis::new(
        available_height,
        (0..children.len()).map(|index| (state.item_height, stack_child_height(children, index, style, atlas, width))),
    )
}

/// Measures the preferred Stack extent without mutating or retaining sizing results.
fn stack_size(state: &StackState, children: &Children, style: &Style, atlas: &AtlasHandle, available: Dimensioni) -> Dimensioni {
    // Aggregate the same shared policies used by placement without retaining per-child geometry.
    let count = children.len();
    if count == 0 {
        return Dimensioni::default();
    }
    let spacing = style.spacing.max(0);
    // Width must be resolved before height because child text may wrap at the shared width.
    let preferred_width = (0..count)
        .map(|index| children.measure_child(index, style, atlas, Dimensioni::default()).unwrap_or_default().width)
        .max()
        .unwrap_or_default();
    let width = state.item_width.preferred_extent(preferred_width, available.width);
    let spacing_total = spacing.saturating_mul(count.saturating_sub(1) as i32);
    let available_height = if available.height > 0 {
        available.height.saturating_sub(spacing_total).max(1)
    } else {
        0
    };
    let mut axis = stack_axis(state, children, style, atlas, width, available_height);
    if available_height == 0 {
        // The construction pass already contains every intrinsic child height.
        return Dimensioni::new(width, axis.intrinsic_extent(count, spacing));
    }
    // Bounded policies require ordered replay so Remainder observes earlier siblings.
    for index in 0..count {
        axis.next(state.item_height, stack_child_height(children, index, style, atlas, width));
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
