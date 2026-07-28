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
//! The crate uses retained [`UiNodeSet`] values as the public UI authoring input while keeping Microui's
//! compact frame-driven execution and renderer integration.
//! It exposes the core context, retained node builders, widget state types, rendering types,
//! styles, and image APIs needed to embed a UI inside custom render backends while remaining
//! allocator- and platform-agnostic.
//! Built-in widget placement is driven by each widget's `measure` result, so auto-sized rows can use
//! per-widget intrinsic text/icon metrics instead of a single shared control size.
//! Retained layout is resolved from context-owned UI nodes, container sizing policies, and widget
//! measurement results.
//! Per-frame interaction results are collected internally and published as a committed
//! generation through [`Context::committed_results`].
//! Retained application/business logic reacts through
//! [`Context::committed_results`], which exposes the previous frame's published
//! interaction generation as the crate's public retained contract.
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
mod file_dialog;
mod frame;
mod id;
mod input;
mod rect_packer;
pub mod render;
mod scrollbar;
mod sizing;
mod style;
#[cfg(test)]
mod test_support;
#[cfg(test)]
mod tests;
mod text_layout;
mod ui_node;
mod widget;
mod widget_ctx;
pub mod widgets;
mod window_manager;

/// Retained UI authoring types.
///
/// This module groups the stable retained concepts used by application code without exposing
/// low-level renderer details or manual container drawing helpers through default imports.
pub mod retained {
    pub use crate::render::{CustomRenderArgs, CustomRenderHandle};
    pub use crate::text_layout::TextWrap;
    pub use crate::window_manager::{Context, ContextFrame, RootId, WindowOption};
    pub use crate::ui_node::{ScrollAreaOption, UiInputEvent};
    pub use crate::widget::{FocusPolicy, FrameResultGeneration, RetainedId, Widget, WidgetInputEvents, WidgetOption, WidgetPaintCtx, WidgetUpdateCtx};
    pub use crate::window_manager::{widget_handle, GridSpan, NodeBuilder, NodeId, NodeOptions, Policy, WidgetHandle, UiNodeSet, UiNodeBuilder};
}

/// Common imports for retained UI applications.
///
/// The prelude intentionally favors retained authoring, widget state, style/input/image types, and
/// renderer integration. Low-level backend and Renderer types live under [`render`].
pub mod prelude {
    pub use crate::atlas::{
        AtlasHandle, CHECK_ICON, CLOSE_ICON, CLOSED_FOLDER_16_ICON, CharEntry, COLLAPSE_ICON, EXPAND_DOWN_ICON, EXPAND_ICON, FILE_16_ICON, FontEntry, FontId,
        IconId, OPEN_FOLDER_16_ICON, SourceFormat, WHITE_ICON, load_image_bytes,
    };
    pub use crate::file_dialog::FileDialogState;
    pub use crate::input::{ControlColor, Input, KeyCode, KeyMode, MouseButton, ResourceState, WidgetFillOption};
    pub use crate::sizing::{SizePolicy, StackDirection};
    pub use crate::render::{FrameError, FrameInfo, FrameInfoError, RendererBackend, RendererFrame};
    pub use crate::retained::{
        Context, ContextFrame, CustomRenderArgs, CustomRenderHandle, FocusPolicy, FrameResultGeneration, NodeBuilder, NodeId, NodeOptions, Policy, RetainedId,
        RootId, ScrollAreaOption, TextWrap, UiInputEvent, Widget, WidgetHandle, WidgetInputEvents, WidgetOption, WidgetPaintCtx, WidgetUpdateCtx, WindowOption,
        UiNodeSet, UiNodeBuilder, widget_handle,
    };
    pub use crate::style::{Color, Font, FontChoice, FontRole, ImageSource, Real, Style, TextureId, color, expand_rect, rect, vec2};
    pub use crate::widgets::{
        Button, ButtonContent, Checkbox, ColorSwatch, Combo, Custom, ListBox, ListItem, Node, NodeStateValue, Number, Slider, TextArea, TextBlock, Textbox,
        WidgetConfig,
    };
    pub use rs_math3d::{
        Box3f, Color4b, CrossProduct, Dimension, Dimensioni, FloatVector, Mat4f, Quat, Quatf, Rect, Recti, Vec2f, Vec2i, Vec3f, Vec4f, Vector, Vector3,
        color4b, ortho4,
    };
}

pub use atlas::{
    AtlasHandle, AtlasSource, CHECK_ICON, CLOSE_ICON, CLOSED_FOLDER_16_ICON, CharEntry, COLLAPSE_ICON, EXPAND_DOWN_ICON, EXPAND_ICON, FILE_16_ICON, FontEntry,
    FontId, IconId, OPEN_FOLDER_16_ICON, SourceFormat, WHITE_ICON, load_image_bytes,
};
pub use window_manager::{
    widget_handle, Context, ContextFrame, GridSpan, NodeBuilder, NodeId, NodeOptions, Policy, RootId, WidgetHandle, WindowOption, UiNodeSet, UiNodeBuilder,
};
pub use file_dialog::FileDialogState;
pub use id::Id;
pub use input::{ControlColor, Input, KeyCode, KeyMode, MouseButton, ResourceState, WidgetFillOption};
pub use text_layout::TextWrap;
pub use sizing::{SizePolicy, StackDirection};
pub use style::{Color, Font, FontChoice, FontRole, ImageSource, Real, Style, TextureId, color, expand_rect, rect, vec2};
pub use widget::{FocusPolicy, FrameResultGeneration, RetainedId, Widget, WidgetInputEvents, WidgetOption, WidgetPaintCtx, WidgetUpdateCtx};
pub use ui_node::{ScrollAreaOption, UiInputEvent};
pub use widgets::{
    Button, ButtonContent, Checkbox, ColorSwatch, Combo, Custom, ListBox, ListItem, Node, NodeStateValue, Number, Slider, TextArea, TextBlock, Textbox,
    WidgetConfig,
};

#[allow(unused_imports)]
pub(crate) use rs_math3d::{
    Box3f, Color4b, CrossProduct, Dimension, Dimensioni, FloatVector, Mat4f, Quat, Quatf, Rect, Recti, Vec2f, Vec2i, Vec3f, Vec4f, Vector, Vector3, color4b,
    ortho4,
};
pub(crate) use std::{cmp::max, hash::Hash, rc::Rc};
pub(crate) use style::UNCLIPPED_RECT;
pub(crate) use ui_node::UiRuntime;
pub(crate) use widget::FrameResults;
