//! Retained UI contracts, built-in components, owning node model, and traversal runtime.
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
//!
//! # Source layout
//!
//! Singular modules such as `widget` and `container` own extension contracts; plural modules such
//! as `widgets` and `containers` are built-in catalogs. Named module files are used at every depth,
//! each public built-in component family has one correspondingly named file, and shared helpers are
//! named for their behavior. Tests follow their owning subject without forcing production code into
//! a `mod.rs` layout.
pub(crate) mod frame;
mod input;
pub use input::UiInputEvent;
mod scrollbar;
mod sizing;
pub use sizing::{Policy, SizePolicy};
mod widget;
mod widget_context;
pub use widget::{
    FocusPolicy, Widget, WidgetBuilder, WidgetOption, WidgetPaintCtx, WidgetParameters, WidgetState, WidgetStateHandle, WidgetStateOwner, WidgetUpdateCtx,
};
pub(crate) use widget::{runtime_read_state, runtime_update_state};
pub mod widgets;

mod children;
pub use children::Children;
mod node_layout;
pub use node_layout::ChildParticipation;
pub(crate) use node_layout::{NodeLayout, RuntimeNodeId, Transform};
mod node;
pub use node::Node;
pub(crate) use node::{NodeKind, NodeRuntime};
mod runtime;
pub(crate) use runtime::UiRuntime;
#[cfg(test)]
pub(crate) use runtime::RuntimeMetrics;
mod container;
pub use container::{Container, ContainerLayoutCtx, Layout};
pub(crate) use container::DispatchResult;
mod containers;
pub use containers::{
    Column, ColumnParameters, ColumnState, Disclosure, DisclosureParameters, DisclosureState, Grid, GridItem, GridParameters, GridSpan, GridState, Row,
    RowParameters, RowState, ScrollArea, ScrollAreaParameters, ScrollAreaState, Stack, StackDirection, StackParameters, StackState,
};
pub use containers::ScrollAreaOption;
