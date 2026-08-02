//! Internal retained node model and traversal runtime.
//!
//! One node representation owns either a widget or a container; no parallel legacy tree or
//! generated public identity path remains.
//!
//! Topology is assembled from unique owning nodes and concrete state-owned containers. Update and
//! paint visit a node before its eligible children in forward sibling order; pointer input visits
//! eligible children first in reverse sibling order so the deepest, topmost node wins. Measurement
//! and layout recurse only through a container's scoped child APIs.
//!
//! Each node retains an allocation in its parent's child coordinates plus a node-local child
//! offset and clip. Recursive passes carry one stack-only [`Transform`]. Resolved outer rectangles
//! and outer clips remain runtime stack locals; phase contexts expose node-local content geometry.
//! Common widget phases dispatch once through the [`crate::Widget`] owned by each private
//! `NodeKind` variant. Traversal branches to [`Container`] only for layout, routed input,
//! descendant visibility, and scoped child visitation. Container update and paint run before the
//! visibility gate is checked. Each concrete container borrows its directly owned state for the
//! current runtime method, and each child borrow remains scoped to one opaque visitor call before
//! recursion continues.
use crate::render::DisplayList;
use crate::{Dimensioni, Recti, UNCLIPPED_RECT};
use crate::WidgetOption;
use crate::widget::FocusPolicy;

mod node;
pub use node::{Children, Node};
pub(crate) use node::{NodeKind, NodeLayout, NodeRuntime, RuntimeNodeId, Transform};
mod runtime;
pub(crate) use runtime::UiRuntime;
#[cfg(test)]
pub(crate) use runtime::RuntimeMetrics;
mod containers;
pub(crate) use containers::WidgetNode;
pub use containers::{
    ChildrenVisitor, ChildrenVisitorMut, Column, ColumnBuilder, ColumnContainer, ColumnParameters, ColumnState, Container, ContainerBuilder, ContainerInputCtx,
    ContainerInputResult, ContainerLayoutCtx, ContainerState, Disclosure, DisclosureBuilder, DisclosureContainer, DisclosureParameters, DisclosureState, Grid,
    GridBuilder, GridContainer, GridItem, GridParameters, GridSpan, GridState, Row, RowBuilder, RowContainer, RowParameters, RowState, Stack, StackBuilder,
    ScrollArea, ScrollAreaBuilder, ScrollAreaContainer, ScrollAreaParameters, ScrollAreaState, StackContainer, StackParameters, StackState,
};
pub use containers::UiInputEvent;
pub use containers::ScrollAreaOption;

/// Returns the union of two rectangles.
fn union_rect(a: Recti, b: Recti) -> Recti {
    let min_x = a.x.min(b.x);
    let min_y = a.y.min(b.y);
    let max_x = (a.x + a.width).max(b.x + b.width);
    let max_y = (a.y + a.height).max(b.y + b.height);
    Recti::new(min_x, min_y, max_x - min_x, max_y - min_y)
}

/// Returns the screen-space rectangle occupied by a child and any overflow content it measured.
fn child_content_rect(node: &Node) -> Recti {
    let allocation = node.state.layout.allocation;
    let content_size = node.state.layout.content_size;
    Recti::new(
        allocation.x,
        allocation.y,
        allocation.width.max(content_size.width),
        allocation.height.max(content_size.height),
    )
}
