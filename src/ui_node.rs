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

//! Retained UI contracts, built-in components, owning node model, and traversal runtime.
//!
//! One node representation owns either a widget or a container; no parallel legacy tree or
//! generated public identity path remains.
//!
//! Topology is assembled from unique owning nodes and concrete child-owning containers. Update and
//! paint visit a node before its eligible children in forward sibling order; pointer input visits
//! eligible children first in reverse sibling order so the deepest, topmost node wins. Measurement
//! and layout recurse only through a container's scoped child APIs.
//!
//! Each node retains an allocation in its parent's child coordinates plus a node-local child
//! offset and clip. Recursive passes carry one stack-only [`Transform`]. Resolved outer rectangles
//! and outer clips remain runtime stack locals; phase contexts expose node-local content geometry.
//! Common phases dispatch through the concrete [`crate::Widget`] owned by each private `NodeKind`
//! variant. A branch stores that object in an erased widget cell beside—not around—its child cell.
//! Container update, layout, and paint borrow the concrete object only for the current
//! method and release it before recursive child traversal; each child borrow likewise remains
//! scoped to one opaque visitor call.
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
pub use sizing::{AvailableSpace, Constraints, TrackSize};
pub(crate) mod text_layout;
pub use text_layout::TextWrap;
mod widget;
mod widget_context;
pub use widget::{
    FocusPolicy, LeafWidget, TypedWidgetHandle, Widget, WidgetBuilder, WidgetFillOption, WidgetOption, WidgetPaintCtx, WidgetParameters, WidgetUpdateCtx,
};
pub(crate) use widget::WidgetStorage;
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
pub use container::{Container, ContainerLayoutCtx, ContainerWidget, MeasureCtx};
mod containers;
pub use containers::{
    Column, ColumnParameters, Disclosure, DisclosureParameters, Grid, GridItem, GridParameters, GridSpan, LinearItem, Row, RowHeight, RowParameters,
    ScrollArea, ScrollAreaParameters,
};
pub use containers::ScrollAreaOption;
