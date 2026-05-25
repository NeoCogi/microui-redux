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
//! The crate uses retained [`WidgetTree`] values as the public UI authoring model while keeping Microui's
//! compact frame-driven execution and renderer integration.
//! It exposes the core context, retained widget tree builders, widget state types, renderer traits,
//! styles, and image APIs needed to embed a UI inside custom render backends while remaining
//! allocator- and platform-agnostic.
//! Built-in widget placement is driven by each widget's `measure` result, so auto-sized rows can use
//! per-widget intrinsic text/icon metrics instead of a single shared control size.
//! Layout internals are flow-based: row tracks and vertical stack flows both run through the same
//! engine so scope/scroll/content bookkeeping stays consistent.
//! Per-frame interaction results are collected internally and published as a committed
//! generation through [`Context::committed_results`].
//! Retained application/business logic reacts through
//! [`Context::committed_results`], which exposes the previous frame's published
//! interaction generation as the crate's public retained contract.

pub mod atlas;
mod canvas;
mod container;
mod context;
mod draw_context;
mod file_dialog;
mod graphics;
mod id;
mod input;
mod layout;
mod rect_packer;
mod render;
mod scrollbar;
mod style;
#[cfg(test)]
mod test_support;
mod text_layout;
mod widget;
mod widget_ctx;
mod widget_tree;
pub mod widgets;
mod window;

/// Low-level renderer integration types.
///
/// Most applications should use [`Context`] plus retained [`WidgetTree`] values. Backend authors
/// and renderer smoke tests can use these types when they need direct access to the command canvas
/// or the exact vertex payload delivered to [`Renderer`].
pub mod backend {
    pub use crate::canvas::{Canvas, Vertex};
    pub use crate::render::{Renderer, RendererHandle};
}

/// Advanced inspection types.
///
/// These are intentionally outside the default retained prelude because they expose low-level state
/// views rather than the primary retained UI authoring model.
pub mod advanced {
    pub use crate::container::{ScrollAreaView, ScrollAreaViewMut};
    pub use crate::graphics::Graphics;
    pub use crate::window::{WindowHandle, WindowState};
}

/// Retained UI authoring types.
///
/// This module groups the stable retained concepts used by application code without exposing
/// low-level renderer/canvas details or manual container drawing helpers through default imports.
pub mod retained {
    pub use crate::container::{CustomRenderArgs, CustomRenderCommand, ScrollAreaHandle, TextWrap};
    pub use crate::context::{Context, RootId};
    pub use crate::widget::{FocusPolicy, FrameResultGeneration, RetainedId, Widget, WidgetCtx};
    pub use crate::widget_tree::{NodeBuilder, NodeId, NodeOptions, Policy, WidgetHandle, WidgetTree, WidgetTreeBuilder, widget_handle};
}

/// Common imports for retained UI applications.
///
/// The prelude intentionally favors retained authoring, widget state, style/input/image types, and
/// renderer integration. Backend-specific canvas access remains under [`backend`].
pub mod prelude {
    pub use crate::atlas::{
        AtlasHandle, CHECK_ICON, CLOSE_ICON, CLOSED_FOLDER_16_ICON, CharEntry, COLLAPSE_ICON, EXPAND_DOWN_ICON, EXPAND_ICON, FILE_16_ICON, FontEntry, FontId,
        IconId, OPEN_FOLDER_16_ICON, SlotId, SourceFormat, WHITE_ICON, load_image_bytes,
    };
    pub use crate::file_dialog::FileDialogState;
    pub use crate::input::{
        Clip, ContainerOption, ControlColor, ControlState, Input, InputButtonState, InputSnapshot, KeyCode, KeyMode, MouseButton, MouseEvent, ResourceState,
        ScrollBehavior, WidgetFillOption, WidgetOption,
    };
    pub use crate::layout::{SizePolicy, StackDirection};
    pub use crate::render::{Renderer, RendererHandle};
    pub use crate::retained::{
        Context, CustomRenderArgs, CustomRenderCommand, FocusPolicy, FrameResultGeneration, NodeBuilder, NodeId, NodeOptions, Policy, RetainedId, RootId,
        ScrollAreaHandle, TextWrap, Widget, WidgetCtx, WidgetHandle, WidgetTree, WidgetTreeBuilder, widget_handle,
    };
    pub use crate::style::{Color, Font, FontChoice, FontRole, Image, ImageSource, Real, Style, TextureId, color, expand_rect, rect, vec2};
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
    FontId, IconId, OPEN_FOLDER_16_ICON, SlotId, SourceFormat, WHITE_ICON, load_image_bytes,
};
pub use container::{CustomRenderArgs, CustomRenderCommand, ScrollAreaHandle, TextWrap};
pub use context::{Context, RootId};
pub use file_dialog::FileDialogState;
pub use id::Id;
pub use input::{
    Clip, ContainerOption, ControlColor, ControlState, Input, InputButtonState, InputSnapshot, KeyCode, KeyMode, MouseButton, MouseEvent, ResourceState,
    ScrollBehavior, WidgetFillOption, WidgetOption,
};
pub use layout::{SizePolicy, StackDirection};
pub use render::{Renderer, RendererHandle};
pub use style::{Color, Font, FontChoice, FontRole, Image, ImageSource, Real, Style, TextureId, color, expand_rect, rect, vec2};
pub use widget::{FocusPolicy, FrameResultGeneration, RetainedId, Widget, WidgetCtx};
pub use widget_tree::{NodeBuilder, NodeId, NodeOptions, Policy, WidgetHandle, WidgetTree, WidgetTreeBuilder, widget_handle};
pub use widgets::{
    Button, ButtonContent, Checkbox, ColorSwatch, Combo, Custom, ListBox, ListItem, Node, NodeStateValue, Number, Slider, TextArea, TextBlock, Textbox,
    WidgetConfig,
};

pub(crate) use canvas::{Canvas, Vertex};
pub(crate) use container::{ScrollArea, TraversalHost};
pub(crate) use layout::LayoutManager;
#[allow(unused_imports)]
pub(crate) use rs_math3d::{
    Box3f, Color4b, CrossProduct, Dimension, Dimensioni, FloatVector, Mat4f, Quat, Quatf, Rect, Recti, Vec2f, Vec2i, Vec3f, Vec4f, Vector, Vector3, color4b,
    ortho4,
};
pub(crate) use std::{
    cell::RefCell,
    cmp::{max, min},
    hash::Hash,
    rc::Rc,
};
pub(crate) use style::UNCLIPPED_RECT;
pub(crate) use widget::FrameResults;
pub(crate) use widget_tree::WidgetTreeCache;
pub(crate) use widgets::{Internal, Scrollbar, ScrollbarLayout};
pub(crate) use window::WindowHandle;
