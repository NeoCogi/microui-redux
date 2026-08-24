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
//! destruction. Leaf nodes retain erased widget cells; containers separately retain an erased
//! concrete [`ContainerWidget`] and their authoritative child collection.
//! Applications and coordinating composites may keep typed weak [`TypedWidgetHandle`] views of
//! either concrete allocation. A handle never keeps a removed widget alive.
//!
//! A concrete `*Parameters` value is one-shot initialization. Semantic values, interaction state,
//! event ports, and [`Widget`] phases live in one concrete widget object rather than a parallel
//! `*State` allocation. Handle-bearing built-in leaf and container constructors return
//! `(TypedWidgetHandle<ConcreteWidget>, Node)`. The stateless [`Custom`] exception returns its
//! concrete runtime from [`Custom::create`]; mount it with [`Node::widget`], [`Node::custom_render`],
//! or [`Node::typed_custom_render`]. A concrete container holds only weak topology capabilities;
//! the generic [`Container`] remains the sole strong owner of its heterogeneous children.
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
//! A `ContextFrame` serializes Context operations but does not lock independent typed handles, and
//! there is no Context token. Typed access closures must finish before retained traversal reaches
//! the same widget. Framework-controlled recursion through a container's opaque
//! child visitor is the intentional exception. If a layout-affecting handle mutation occurs after
//! the last commit, cancel any unsubmitted frame and call `update_ui` again before paint.
//! Handles and Context are independent Rust values, so explicitly capturing Context inside
//! `try_update` compiles; it is nevertheless unsupported because the closure retains the mutable
//! widget borrow. If nested traversal reaches that allocation, runtime borrowing reports an
//! invariant panic. End the closure before calling `update_ui` or `render_ui`.
//!
//! Event-driven applications construct `Context::<B, State>::new(backend)`, register each
//! native widget endpoint through [`Context::subscribe`] or [`Context::subscribe_with`], and call
//! [`Context::update_ui_state`]. The context owns the only application widget-event dispatcher for
//! its complete root forest. This semantic dispatcher is distinct from retained raw-input routing:
//! each retained runtime owns an input router that targets pointer, keyboard, or text input to one
//! node, and that widget may then emit a typed event for the application dispatcher.
//! Application-owned library components such as [`FileDialog`] bind their controls to that same
//! dispatcher through an accessor into application state. Each event port owns its pending
//! payloads and accepts one state method.
//!
//! # Text encoding and glyph coverage
//!
//! Public text uses Rust [`str`] and [`String`] values and is therefore valid UTF-8. Textbox and
//! text-area cursors are byte indices kept on Unicode scalar-value boundaries; movement and
//! deletion operate on scalar values rather than grapheme clusters.
//!
//! Rendering coverage belongs to the selected atlas font. The built-in atlas builder bakes
//! printable ASCII (`U+0020` through `U+007E`). A missing character uses the font's underscore
//! glyph; custom [`AtlasSource`] tables may provide arbitrary Unicode scalar values and should
//! include `_`. The renderer does not perform script shaping, bidirectional reordering, grapheme
//! segmentation, kerning, or fallback-font selection.
//!
//! Update and paint traverse parent before children and siblings in forward order. Later work sees
//! successful earlier cross-cell mutations, work already completed does not rerun, and each input
//! transaction's final layout observes the resulting state and topology. Paint is observational and
//! may update private rendering caches only. Custom-render callbacks may update callback-private
//! rendering caches only.
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
//!     let (_button, button_node) = Button::create(ButtonParameters::new("Save"));
//!     let root = context.create_window(
//!         "main",
//!         rect(20, 20, 180, 80),
//!         button_node,
//!     );
//!
//!     context.update_ui(dimensions);
//!     context.frame(info).render_ui()?;
//!     Ok(root)
//! }
//! ```
//!
//! The crate exposes the context, child-owning containers, widgets, rendering types, styles, and
//! image APIs needed to embed a UI inside custom render backends while remaining allocator- and
//! platform-agnostic.
//! Built-in widget placement is driven by each widget's `measure` result, so auto-sized rows can use
//! per-widget intrinsic text/icon metrics instead of a single shared control size.
//! Retained layout is resolved from context-owned UI nodes, container sizing policies, and widget
//! measurement results.
//! Retained application logic uses typed weak widget handles returned beside mounted nodes.
//! The [`retained`] module and repository examples document the 0.8 alpha retained-authoring API.
//! This release is versioned `0.8.0-alpha.5` and adds sixteen fixed application layers,
//! source-bound popup stacking, independent root activation, and fullscreen application surfaces
//! to that redesign.
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
mod context;
mod event;
mod file_dialog;
pub mod image;
mod input;
mod math;
mod menu;
pub mod render;
#[cfg(test)]
mod test_support;
pub mod theme;
mod ui_node;
pub use ui_node::layout;
pub use ui_node::widgets;
mod window_manager;

/// Retained UI authoring types.
///
/// This module groups the stable retained concepts used by application code without exposing
/// low-level renderer details or manual container drawing helpers through default imports.
pub mod retained {
    pub use crate::event::{SubscribeError, TypedWidget, WidgetEvent, WidgetEventPortHandle};
    pub use crate::file_dialog::{FileDialog, FileDialogCompleted, FileDialogRequest, FileDialogResult, FileDialogStatus};
    pub use crate::menu::{Menu, MenuGroup, MenuItem, MenuItemMark, MenuItemParameters, MenuItemSubmitted, MenuPanel, Submenu, WindowMenu};
    pub use crate::render::{CustomRenderArgs, CustomRenderHandle};
    pub use crate::ui_node::{
        AvailableSpace, ChildParticipation, Children, Constraints, Container, ContainerLayoutCtx, ContainerWidget, Disclosure, DisclosureParameters,
        FocusPolicy, Grid, GridItem, GridParameters, GridSpan, LeafWidget, Linear, LinearCrossSize, LinearDirection, LinearItem, LinearParameters, MeasureCtx,
        Node, ScrollArea, ScrollAreaOption, ScrollAreaParameters, Scrollbar, ScrollbarAxis, ScrollbarChanged, ScrollbarParameters, TrackSize, UiInputEvent,
        Widget, TextWrap, TypedWidgetHandle, WidgetBuilder, WidgetFillOption, WidgetOption, WidgetPaintCtx, WidgetParameters, WidgetUpdateCtx,
    };
    pub use crate::context::{Context, ContextFrame, EventContext};
    pub use crate::window_manager::{
        DEFAULT_LAYER, LayerBinding, MAX_LAYER, MIN_LAYER, PopupHandle, RootChanged, RootHandle, RootId, RootMutationError, RootChrome, RootSubmitted,
        WindowOption,
    };
}

/// Common imports for retained UI applications.
///
/// The prelude intentionally favors retained authoring, widget state, style/input/image types, and
/// renderer integration. Low-level backend and Renderer types live under [`render`].
pub mod prelude {
    pub use crate::event::{SubscribeError, TypedWidget, WidgetEvent, WidgetEventPortHandle};
    pub use crate::atlas::{
        AtlasHandle, CHECK_ICON, CLOSE_ICON, CLOSED_FOLDER_16_ICON, CharEntry, COLLAPSE_ICON, EXPAND_DOWN_ICON, EXPAND_ICON, FILE_16_ICON, FontEntry, FontId,
        IconId, OPEN_FOLDER_16_ICON, SourceFormat, WHITE_ICON,
    };
    pub use crate::file_dialog::{FileDialog, FileDialogCompleted, FileDialogRequest, FileDialogResult, FileDialogStatus};
    pub use crate::menu::{Menu, MenuGroup, MenuItem, MenuItemMark, MenuItemParameters, MenuItemSubmitted, MenuPanel, Submenu, WindowMenu};
    pub use crate::image::{ImageSource, load_image_bytes};
    pub use crate::input::{KeyCode, KeyMode, MouseButton};
    pub use crate::render::{FrameError, FrameInfo, FrameInfoError, RendererBackend, RendererFrame, TextureId};
    pub use crate::retained::{
        ChildParticipation, Children, Container, ContainerLayoutCtx, ContainerWidget, Context, ContextFrame, EventContext, CustomRenderArgs, AvailableSpace,
        Constraints, CustomRenderHandle, Disclosure, DisclosureParameters, FocusPolicy, Grid, GridItem, GridParameters, GridSpan, Linear, LinearCrossSize,
        LinearDirection, LinearItem, LinearParameters, Node, PopupHandle, RootChanged, RootHandle, MeasureCtx, RootId, RootMutationError, RootChrome,
        RootSubmitted, LayerBinding, DEFAULT_LAYER, MAX_LAYER, MIN_LAYER, ScrollArea, ScrollAreaOption, ScrollAreaParameters, Scrollbar, ScrollbarAxis,
        ScrollbarChanged, ScrollbarParameters, TrackSize, LeafWidget, TextWrap, UiInputEvent, TypedWidgetHandle, Widget, WidgetBuilder, WidgetFillOption,
        WidgetOption, WidgetPaintCtx, WidgetParameters, WidgetUpdateCtx, WindowOption,
    };
    pub use crate::math::{expand_rect, rect, vec2};
    pub use crate::theme::{Color, ControlColor, FontChoice, FontRole, Style, ThemeIcons, color};
    pub use crate::widgets::{
        Button, ButtonBuilder, ButtonContent, ButtonParameters, ButtonSubmitted, Checkbox, CheckboxBuilder, CheckboxChanged, CheckboxParameters, ColorSwatch,
        ColorSwatchBuilder, ColorSwatchParameters, Combo, ComboBuilder, ComboChanged, ComboParameters, ComboSubmitted, Custom, CustomBuilder, CustomParameters,
        ListBox, ListBoxBuilder, ListBoxParameters, ListBoxSubmitted, ListItem, ListItemBuilder, ListItemParameters, ListItemSubmitted, Number, NumberBuilder,
        NumberChanged, NumberParameters, Slider, SliderBuilder, SliderChanged, SliderParameters, TextArea, TextAreaChanged, TextAreaParameters,
        TextAreaSubmitted, TextBlock, TextBlockBuilder, TextBlockParameters, Textbox, TextboxBuilder, TextboxChanged, TextboxParameters, TextboxSubmitted,
        Real,
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
pub use context::{Context, ContextFrame, EventContext};
pub use window_manager::{
    DEFAULT_LAYER, LayerBinding, MAX_LAYER, MIN_LAYER, PopupHandle, RootChanged, RootHandle, RootId, RootMutationError, RootChrome, RootSubmitted, WindowOption,
};
pub use event::{SubscribeError, TypedWidget, WidgetEvent, WidgetEventPortHandle};
pub use file_dialog::{FileDialog, FileDialogCompleted, FileDialogRequest, FileDialogResult, FileDialogStatus};
pub use menu::{Menu, MenuGroup, MenuItem, MenuItemMark, MenuItemParameters, MenuItemSubmitted, MenuPanel, Submenu, WindowMenu};
pub use image::{ImageSource, load_image_bytes};
pub use input::{KeyCode, KeyMode, MouseButton};
pub use math::{expand_rect, rect, vec2};
pub use render::TextureId;
pub use theme::{Color, ControlColor, FontChoice, FontRole, Style, ThemeIcons, color};
pub use ui_node::{
    AvailableSpace, ChildParticipation, Children, Constraints, Container, ContainerLayoutCtx, ContainerWidget, Disclosure, DisclosureParameters, FocusPolicy,
    Grid, GridItem, GridParameters, GridSpan, LeafWidget, Linear, LinearCrossSize, LinearDirection, LinearItem, LinearParameters, MeasureCtx, Node, ScrollArea,
    ScrollAreaOption, ScrollAreaParameters, Scrollbar, ScrollbarAxis, ScrollbarChanged, ScrollbarParameters, TextWrap, TrackSize, UiInputEvent, Widget,
    TypedWidgetHandle, WidgetBuilder, WidgetFillOption, WidgetOption, WidgetPaintCtx, WidgetParameters, WidgetUpdateCtx,
};
pub use widgets::{
    Button, ButtonBuilder, ButtonContent, ButtonParameters, ButtonSubmitted, Checkbox, CheckboxBuilder, CheckboxChanged, CheckboxParameters, ColorSwatch,
    ColorSwatchBuilder, ColorSwatchParameters, Combo, ComboBuilder, ComboChanged, ComboParameters, ComboSubmitted, Custom, CustomBuilder, CustomParameters,
    ListBox, ListBoxBuilder, ListBoxParameters, ListBoxSubmitted, ListItem, ListItemBuilder, ListItemParameters, ListItemSubmitted, Number, NumberBuilder,
    NumberChanged, NumberParameters, Slider, SliderBuilder, SliderChanged, SliderParameters, TextArea, TextAreaChanged, TextAreaParameters, TextAreaSubmitted,
    Real, TextBlock, TextBlockBuilder, TextBlockParameters, Textbox, TextboxBuilder, TextboxChanged, TextboxParameters, TextboxSubmitted,
};

#[allow(unused_imports)]
pub(crate) use rs_math3d::{
    Box3f, Color4b, CrossProduct, Dimension, Dimensioni, FloatVector, Mat4f, Quat, Quatf, Rect, Recti, Vec2f, Vec2i, Vec3f, Vec4f, Vector, Vector3, color4b,
    ortho4,
};
pub(crate) use std::{cmp::max, hash::Hash, rc::Rc};
pub(crate) use render::geometry::UNCLIPPED_RECT;
pub(crate) use ui_node::UiRuntime;
