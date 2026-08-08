//
// Copyright 2022-Present (c) Raja Lehtihet & Wael El Oraiby
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
// -----------------------------------------------------------------------------
// Ported to rust from https://github.com/rxi/microui/ and the original license
//
// Copyright (c) 2020 rxi
//
// Permission is hereby granted, free of charge, to any person obtaining a copy
// of this software and associated documentation files (the "Software"), to
// deal in the Software without restriction, including without limitation the
// rights to use, copy, modify, merge, publish, distribute, sublicense, and/or
// sell copies of the Software, and to permit persons to whom the Software is
// furnished to do so, subject to the following conditions:
//
// The above copyright notice and this permission notice shall be included in
// all copies or substantial portions of the Software.
//
// THE SOFTWARE IS PROVIDED "AS IS", WITHOUT WARRANTY OF ANY KIND, EXPRESS OR
// IMPLIED, INCLUDING BUT NOT LIMITED TO THE WARRANTIES OF MERCHANTABILITY,
// FITNESS FOR A PARTICULAR PURPOSE AND NONINFRINGEMENT. IN NO EVENT SHALL THE
// AUTHORS OR COPYRIGHT HOLDERS BE LIABLE FOR ANY CLAIM, DAMAGES OR OTHER
// LIABILITY, WHETHER IN AN ACTION OF CONTRACT, TORT OR OTHERWISE, ARISING
// FROM, OUT OF OR IN CONNECTION WITH THE SOFTWARE OR THE USE OR OTHER DEALINGS
// IN THE SOFTWARE.
//
#![deny(missing_docs)]
// `clippy::pedantic` is useful as an occasional review tool for this crate, but these categories
// are intentionally outside the local lint profile. The UI/rendering path performs many bounded
// pixel/UV casts, internal modules use crate preludes heavily, and the retained public API
// should not grow `#[must_use]` or pedantic doc-section noise mechanically.
#![allow(
    clippy::cast_lossless,
    clippy::cast_possible_truncation,
    clippy::cast_possible_wrap,
    clippy::cast_precision_loss,
    clippy::cast_sign_loss,
    clippy::missing_errors_doc,
    clippy::missing_panics_doc,
    clippy::module_name_repetitions,
    clippy::must_use_candidate,
    clippy::similar_names,
    clippy::struct_field_names,
    clippy::too_many_arguments,
    clippy::wildcard_imports
)]
//! `microui-redux` provides a GUI toolkit inspired by [rxi/microui](https://github.com/rxi/microui).
//! The crate uses unique owning [`Node`] values as its public UI authoring input. Each
//! [`Context`] root consumes one persistent node and remains its sole owner until explicit
//! destruction. Applications retain typed weak [`WidgetStateHandle`] and [`RootHandle`]
//! capabilities, not node identities or strong mounted-state owners.
//!
//! Construction is split deliberately: a concrete `*Parameters` value is one-shot initialization,
//! a concrete `*State` value contains mounted mutable values and native event endpoints, and the concrete
//! runtime implements [`Widget`] plus [`WidgetStateOwner`]. Ordinary leaf constructors return
//! `(WidgetStateHandle<State>, Runtime)`; ordinary container constructors return
//! `(WidgetStateHandle<State>, Node)`. [`widgets::Custom::create`] is the fixed exception because
//! its state is `()` and it exposes no application handle. Discarding a returned weak handle never
//! changes runtime ownership.
//!
//! # Update and paint boundary
//!
//! Input is appended through [`Context`] forwarding methods. [`Context::update_ui`] first
//! synchronizes layout, then drains that one ordered queue. Each raw event is normalized, routed,
//! applied by one full eligible-tree update traversal, and followed by layout before the next
//! event. Calling it with an empty queue is the synchronization path after programmatic state or
//! topology mutation. [`ContextFrame::render_ui`] is paint-only and returns
//! [`render::RenderError::UiUpdateRequired`] before backend acquisition when the commit is missing,
//! stale by Context-owned input/mutation, or for different dimensions. The runtime synthesizes no
//! timer events and produces no generic frame-result or resource-state object.
//!
//! A `ContextFrame` serializes Context operations but does not lock independent typed state
//! handles, and there is no Context token. State-access closures must finish before retained
//! traversal reaches the same state. Framework-controlled recursion through a container's opaque
//! child visitor is the intentional exception. If a layout-affecting handle mutation occurs after
//! the last commit, cancel any unsubmitted frame and call `update_ui` again before paint.
//! Handles and Context are independent Rust values, so explicitly capturing Context inside
//! `try_update` compiles; it is nevertheless unsupported because the closure retains the mutable
//! state borrow. If nested traversal reaches that cell, built-in runtime borrowing reports the
//! phase-specific invariant panic. End the closure before calling `update_ui` or `render_ui`.
//!
//! Update and paint traverse parent before children and siblings in forward order. Later work sees
//! successful earlier cross-cell mutations, work already completed does not rerun, and each input
//! transaction's final layout observes the resulting state and topology. Paint is observational
//! with respect to application-authored semantic state and the committed layout. Built-in widgets
//! may publish framework-owned, paint-derived read-only geometry for later use or update private
//! rendering caches; custom-render callbacks may update callback-private rendering caches only.
//!
//! ```
//! use microui_redux::prelude::*;
//! use microui_redux::render::{RenderError, RendererBackend};
//!
//! fn install_and_draw<B: RendererBackend>(
//!     context: &mut Context<B>,
//!     dimensions: Dimensioni,
//!     info: FrameInfo,
//! ) -> Result<RootHandle, RenderError> {
//!     let (_button_state, button) = Button::create(ButtonParameters::new("Save"));
//!     let root = context.create_window(
//!         "main",
//!         rect(20, 20, 180, 80),
//!         Node::widget(button),
//!     );
//!
//!     context.update_ui(dimensions);
//!     context.frame(info).render_ui()?;
//!     Ok(root)
//! }
//! ```
//!
//! The crate exposes the context, state-owning containers, widgets, rendering types, styles, and
//! image APIs needed to embed a UI inside custom render backends while remaining allocator- and
//! platform-agnostic.
//! Built-in widget placement is driven by each widget's `measure` result, so auto-sized rows can use
//! per-widget intrinsic text/icon metrics instead of a single shared control size.
//! Retained layout is resolved from context-owned UI nodes, container sizing policies, and widget
//! measurement results.
//! Retained application logic observes typed widget state handles returned by constructors.
//! The [`retained`] module and repository examples document the `0.8.0-pre-alpha` retained-authoring
//! API.
//!
//! # Rendering pipeline
//!
//! Drawing is recorded before it reaches the backend:
//!
//! ```text
//! Widget::paint
//!      |
//!      v
//!   Painter  --->  DisplayList  --->  Renderer  --->  RendererBackend::Frame
//! (records)         (owns ops)        (executes)       (submits/presents)
//! ```
//!
//! Widgets obtain a [`render::Painter`] from [`WidgetPaintCtx::painter`] and record
//! backend-neutral operations. [`render::Renderer`] executes the resulting crate-owned display
//! list, performs final clipping and tessellation, and submits final
//! [`render::Vertex`] values through [`render::RendererBackend`]. Applications normally import
//! retained UI types from [`prelude`], while backend integrations import frame contracts from
//! [`render`].

pub mod atlas;
mod event;
mod file_dialog;
pub mod image;
mod input;
mod math;
pub mod render;
#[cfg(test)]
mod test_support;
pub mod theme;
mod ui_node;
pub use ui_node::widgets;
mod window_manager;

/// Retained UI authoring types.
///
/// This module groups the stable retained concepts used by application code without exposing
/// low-level renderer details or manual container drawing helpers through default imports.
pub mod retained {
    pub use crate::event::{ConnectError, Emit, Session, Subscribers, SubscriptionId, WidgetEvent};
    pub use crate::file_dialog::{FileDialogRequest, FileDialogResult, FileDialogSession, FileDialogStatus};
    pub use crate::render::{CustomRenderArgs, CustomRenderHandle};
    pub use crate::ui_node::{
        ChildParticipation, Children, Column, ColumnParameters, ColumnState, Container, ContainerLayoutCtx, ContainerSurface, Disclosure, DisclosureParameters,
        DisclosureState, FocusPolicy, Grid, GridItem, GridParameters, GridSpan, GridState, Layout, Node, Policy, Row, RowParameters, RowState, ScrollArea,
        ScrollAreaOption, ScrollAreaParameters, ScrollAreaState, SizePolicy, Stack, StackDirection, StackParameters, StackState, UiInputEvent, Widget,
        TextWrap, WidgetBuilder, WidgetFillOption, WidgetOption, WidgetPaintCtx, WidgetParameters, WidgetState, WidgetStateHandle, WidgetStateOwner,
        WidgetUpdateCtx,
    };
    pub use crate::window_manager::{Context, ContextFrame, RootChanged, RootHandle, RootId, RootMutationError, RootState, RootSubmitted, WindowOption};
}

/// Common imports for retained UI applications.
///
/// The prelude intentionally favors retained authoring, widget state, style/input/image types, and
/// renderer integration. Low-level backend and Renderer types live under [`render`].
pub mod prelude {
    pub use crate::event::{ConnectError, Emit, Session, Subscribers, SubscriptionId, WidgetEvent};
    pub use crate::atlas::{
        AtlasHandle, CHECK_ICON, CLOSE_ICON, CLOSED_FOLDER_16_ICON, CharEntry, COLLAPSE_ICON, EXPAND_DOWN_ICON, EXPAND_ICON, FILE_16_ICON, FontEntry, FontId,
        IconId, OPEN_FOLDER_16_ICON, SourceFormat, WHITE_ICON,
    };
    pub use crate::file_dialog::{FileDialogRequest, FileDialogResult, FileDialogSession, FileDialogStatus};
    pub use crate::image::{ImageSource, load_image_bytes};
    pub use crate::input::{KeyCode, KeyMode, MouseButton};
    pub use crate::render::{FrameError, FrameInfo, FrameInfoError, RendererBackend, RendererFrame, TextureId};
    pub use crate::retained::{
        ChildParticipation, Children, Column, ColumnParameters, ColumnState, Container, ContainerLayoutCtx, ContainerSurface, Context, ContextFrame,
        CustomRenderArgs, CustomRenderHandle, Disclosure, DisclosureParameters, DisclosureState, FocusPolicy, Grid, GridItem, GridParameters, GridSpan,
        GridState, Layout, Node, Policy, RootChanged, RootHandle, RootId, RootMutationError, RootState, RootSubmitted, Row, RowParameters, RowState,
        ScrollArea, ScrollAreaOption, ScrollAreaParameters, ScrollAreaState, SizePolicy, Stack, StackDirection, StackParameters, StackState, TextWrap,
        UiInputEvent, Widget, WidgetBuilder, WidgetFillOption, WidgetOption, WidgetPaintCtx, WidgetParameters, WidgetState, WidgetStateHandle,
        WidgetStateOwner, WidgetUpdateCtx, WindowOption,
    };
    pub use crate::math::{expand_rect, rect, vec2};
    pub use crate::theme::{Color, ControlColor, FontChoice, FontRole, Style, ThemeIcons, color};
    pub use crate::widgets::{
        Button, ButtonBuilder, ButtonContent, ButtonParameters, ButtonState, ButtonSubmitted, Checkbox, CheckboxBuilder, CheckboxChanged, CheckboxParameters,
        CheckboxState, ColorSwatch, ColorSwatchBuilder, ColorSwatchParameters, ColorSwatchState, Combo, ComboBuilder, ComboChanged, ComboParameters,
        ComboState, ComboSubmitted, Custom, CustomBuilder, CustomParameters, ListBox, ListBoxBuilder, ListBoxParameters, ListBoxState, ListBoxSubmitted,
        ListItem, ListItemBuilder, ListItemParameters, ListItemState, ListItemSubmitted, Number, NumberBuilder, NumberChanged, NumberParameters, NumberState,
        Slider, SliderBuilder, SliderChanged, SliderParameters, SliderState, TextArea, TextAreaBuilder, TextAreaChanged, TextAreaParameters, TextAreaState,
        TextAreaSubmitted, TextBlock, TextBlockBuilder, TextBlockParameters, TextBlockState, Textbox, TextboxBuilder, TextboxChanged, TextboxParameters,
        TextboxState, TextboxSubmitted, Real,
    };
    pub use rs_math3d::{
        Box3f, Color4b, CrossProduct, Dimension, Dimensioni, FloatVector, Mat4f, Quat, Quatf, Rect, Recti, Vec2f, Vec2i, Vec3f, Vec4f, Vector, Vector3,
        color4b, ortho4,
    };
}

pub use atlas::{
    AtlasHandle, AtlasSource, CHECK_ICON, CLOSE_ICON, CLOSED_FOLDER_16_ICON, CharEntry, COLLAPSE_ICON, EXPAND_DOWN_ICON, EXPAND_ICON, FILE_16_ICON, FontEntry,
    FontId, IconId, OPEN_FOLDER_16_ICON, SourceFormat, WHITE_ICON,
};
pub use window_manager::{Context, ContextFrame, RootChanged, RootHandle, RootId, RootMutationError, RootState, RootSubmitted, WindowOption};
pub use event::{ConnectError, Emit, Session, Subscribers, SubscriptionId, WidgetEvent};
pub use file_dialog::{FileDialogRequest, FileDialogResult, FileDialogSession, FileDialogStatus};
pub use image::{ImageSource, load_image_bytes};
pub use input::{KeyCode, KeyMode, MouseButton};
pub use math::{expand_rect, rect, vec2};
pub use render::TextureId;
pub use theme::{Color, ControlColor, FontChoice, FontRole, Style, ThemeIcons, color};
pub use ui_node::{
    ChildParticipation, Children, Column, ColumnParameters, ColumnState, Container, ContainerLayoutCtx, ContainerSurface, Disclosure, DisclosureParameters,
    DisclosureState, FocusPolicy, Grid, GridItem, GridParameters, GridSpan, GridState, Layout, Node, Policy, Row, RowParameters, RowState, ScrollArea,
    ScrollAreaOption, ScrollAreaParameters, ScrollAreaState, SizePolicy, Stack, StackDirection, StackParameters, StackState, TextWrap, UiInputEvent, Widget,
    WidgetBuilder, WidgetFillOption, WidgetOption, WidgetPaintCtx, WidgetParameters, WidgetState, WidgetStateHandle, WidgetStateOwner, WidgetUpdateCtx,
};
pub use widgets::{
    Button, ButtonBuilder, ButtonContent, ButtonParameters, ButtonState, ButtonSubmitted, Checkbox, CheckboxBuilder, CheckboxChanged, CheckboxParameters,
    CheckboxState, ColorSwatch, ColorSwatchBuilder, ColorSwatchParameters, ColorSwatchState, Combo, ComboBuilder, ComboChanged, ComboParameters, ComboState,
    ComboSubmitted, Custom, CustomBuilder, CustomParameters, ListBox, ListBoxBuilder, ListBoxParameters, ListBoxState, ListBoxSubmitted, ListItem,
    ListItemBuilder, ListItemParameters, ListItemState, ListItemSubmitted, Number, NumberBuilder, NumberChanged, NumberParameters, NumberState, Slider,
    SliderBuilder, SliderChanged, SliderParameters, SliderState, TextArea, TextAreaBuilder, TextAreaChanged, TextAreaParameters, TextAreaState,
    TextAreaSubmitted, Real, TextBlock, TextBlockBuilder, TextBlockParameters, TextBlockState, Textbox, TextboxBuilder, TextboxChanged, TextboxParameters,
    TextboxState, TextboxSubmitted,
};

#[allow(unused_imports)]
pub(crate) use rs_math3d::{
    Box3f, Color4b, CrossProduct, Dimension, Dimensioni, FloatVector, Mat4f, Quat, Quatf, Rect, Recti, Vec2f, Vec2i, Vec3f, Vec4f, Vector, Vector3, color4b,
    ortho4,
};
pub(crate) use std::{cmp::max, hash::Hash, rc::Rc};
pub(crate) use render::geometry::UNCLIPPED_RECT;
pub(crate) use ui_node::UiRuntime;
