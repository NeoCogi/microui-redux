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
//! The crate uses unique owning [`Node`] values as its public UI authoring input; each root consumes
//! one persistent node while keeping Microui's compact frame-driven execution and renderer
//! integration. It exposes the core context, state-owned containers, widget state types, rendering types,
//! styles, and image APIs needed to embed a UI inside custom render backends while remaining
//! allocator- and platform-agnostic.
//! Built-in widget placement is driven by each widget's `measure` result, so auto-sized rows can use
//! per-widget intrinsic text/icon metrics instead of a single shared control size.
//! Retained layout is resolved from context-owned UI nodes, container sizing policies, and widget
//! measurement results.
//! Retained application logic observes typed widget state handles returned by constructors.
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
mod input;
mod rect_packer;
pub mod render;
mod scrollbar;
mod sizing;
mod style;
#[cfg(test)]
mod test_support;
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
    pub use crate::file_dialog::{FileDialogRequest, FileDialogResult, FileDialogSession, FileDialogStatus};
    pub use crate::render::{CustomRenderArgs, CustomRenderHandle};
    pub use crate::sizing::{Policy, SizePolicy, StackDirection};
    pub use crate::text_layout::TextWrap;
    pub use crate::ui_node::{
        Children, ChildrenVisitor, ChildrenVisitorMut, Column, ColumnBuilder, ColumnContainer, ColumnParameters, ColumnState, Container, ContainerBuilder,
        ContainerInputCtx, ContainerInputResult, ContainerLayoutCtx, ContainerState, Disclosure, DisclosureBuilder, DisclosureContainer, DisclosureParameters,
        DisclosureState, Grid, GridBuilder, GridContainer, GridItem, GridParameters, GridSpan, GridState, Node, Row, RowBuilder, RowContainer, RowParameters,
        RowState, ScrollArea, ScrollAreaBuilder, ScrollAreaContainer, ScrollAreaOption, ScrollAreaParameters, ScrollAreaState, Stack, StackBuilder,
        StackContainer, StackParameters, StackState, UiInputEvent,
    };
    pub use crate::window_manager::{Context, ContextFrame, RootHandle, RootId, RootMutationError, RootState, WindowOption};
    pub use crate::widget::{
        FocusPolicy, Widget, WidgetBuilder, WidgetOption, WidgetPaintCtx, WidgetParameters, WidgetState, WidgetStateHandle, WidgetStateOwner, WidgetUpdateCtx,
    };
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
    pub use crate::file_dialog::{FileDialogRequest, FileDialogResult, FileDialogSession, FileDialogStatus};
    pub use crate::input::{ControlColor, Input, KeyCode, KeyMode, MouseButton, WidgetFillOption};
    pub use crate::sizing::{Policy, SizePolicy, StackDirection};
    pub use crate::render::{FrameError, FrameInfo, FrameInfoError, RendererBackend, RendererFrame};
    pub use crate::retained::{
        Children, ChildrenVisitor, ChildrenVisitorMut, Column, ColumnBuilder, ColumnContainer, ColumnParameters, ColumnState, Container, ContainerBuilder,
        ContainerInputCtx, ContainerInputResult, ContainerLayoutCtx, ContainerState, Context, ContextFrame, CustomRenderArgs, CustomRenderHandle, Disclosure,
        DisclosureBuilder, DisclosureContainer, DisclosureParameters, DisclosureState, FocusPolicy, Grid, GridBuilder, GridContainer, GridItem, GridParameters,
        GridSpan, GridState, Node, RootHandle, RootId, RootMutationError, RootState, Row, RowBuilder, RowContainer, RowParameters, RowState, ScrollArea,
        ScrollAreaBuilder, ScrollAreaContainer, ScrollAreaOption, ScrollAreaParameters, ScrollAreaState, Stack, StackBuilder, StackContainer, StackParameters,
        StackState, TextWrap, UiInputEvent, Widget, WidgetBuilder, WidgetOption, WidgetPaintCtx, WidgetParameters, WidgetState, WidgetStateHandle,
        WidgetStateOwner, WidgetUpdateCtx, WindowOption,
    };
    pub use crate::style::{Color, Font, FontChoice, FontRole, ImageSource, Real, Style, TextureId, color, expand_rect, rect, vec2};
    pub use crate::widgets::{
        Button, ButtonBuilder, ButtonContent, ButtonParameters, ButtonState, Checkbox, CheckboxBuilder, CheckboxParameters, CheckboxState, ColorSwatch,
        ColorSwatchBuilder, ColorSwatchParameters, ColorSwatchState, Combo, ComboBuilder, ComboParameters, ComboState, Custom, CustomBuilder, CustomParameters,
        ListBox, ListBoxBuilder, ListBoxParameters, ListBoxState, ListItem, ListItemBuilder, ListItemParameters, ListItemState, Number, NumberBuilder,
        NumberParameters, NumberState, Slider, SliderBuilder, SliderParameters, SliderState, TextArea, TextAreaBuilder, TextAreaParameters, TextAreaState,
        TextBlock, TextBlockBuilder, TextBlockParameters, TextBlockState, Textbox, TextboxBuilder, TextboxParameters, TextboxState, WidgetConfig,
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
pub use window_manager::{Context, ContextFrame, RootHandle, RootId, RootMutationError, RootState, WindowOption};
pub use file_dialog::{FileDialogRequest, FileDialogResult, FileDialogSession, FileDialogStatus};
pub use input::{ControlColor, Input, KeyCode, KeyMode, MouseButton, WidgetFillOption};
pub use text_layout::TextWrap;
pub use sizing::{Policy, SizePolicy, StackDirection};
pub use style::{Color, Font, FontChoice, FontRole, ImageSource, Real, Style, TextureId, color, expand_rect, rect, vec2};
pub use widget::{
    FocusPolicy, Widget, WidgetBuilder, WidgetOption, WidgetPaintCtx, WidgetParameters, WidgetState, WidgetStateHandle, WidgetStateOwner, WidgetUpdateCtx,
};
pub use ui_node::{
    Children, ChildrenVisitor, ChildrenVisitorMut, Column, ColumnBuilder, ColumnContainer, ColumnParameters, ColumnState, Container, ContainerBuilder,
    ContainerInputCtx, ContainerInputResult, ContainerLayoutCtx, ContainerState, Disclosure, DisclosureBuilder, DisclosureContainer, DisclosureParameters,
    DisclosureState, Grid, GridBuilder, GridContainer, GridItem, GridParameters, GridSpan, GridState, Node, Row, RowBuilder, RowContainer, RowParameters,
    RowState, ScrollArea, ScrollAreaBuilder, ScrollAreaContainer, ScrollAreaOption, ScrollAreaParameters, ScrollAreaState, Stack, StackBuilder, StackContainer,
    StackParameters, StackState, UiInputEvent,
};
pub use widgets::{
    Button, ButtonBuilder, ButtonContent, ButtonParameters, ButtonState, Checkbox, CheckboxBuilder, CheckboxParameters, CheckboxState, ColorSwatch,
    ColorSwatchBuilder, ColorSwatchParameters, ColorSwatchState, Combo, ComboBuilder, ComboParameters, ComboState, Custom, CustomBuilder, CustomParameters,
    ListBox, ListBoxBuilder, ListBoxParameters, ListBoxState, ListItem, ListItemBuilder, ListItemParameters, ListItemState, Number, NumberBuilder,
    NumberParameters, NumberState, Slider, SliderBuilder, SliderParameters, SliderState, TextArea, TextAreaBuilder, TextAreaParameters, TextAreaState,
    TextBlock, TextBlockBuilder, TextBlockParameters, TextBlockState, Textbox, TextboxBuilder, TextboxParameters, TextboxState, WidgetConfig,
};

#[allow(unused_imports)]
pub(crate) use rs_math3d::{
    Box3f, Color4b, CrossProduct, Dimension, Dimensioni, FloatVector, Mat4f, Quat, Quatf, Rect, Recti, Vec2f, Vec2i, Vec3f, Vec4f, Vector, Vector3, color4b,
    ortho4,
};
pub(crate) use std::{cmp::max, hash::Hash, rc::Rc};
pub(crate) use style::UNCLIPPED_RECT;
pub(crate) use ui_node::UiRuntime;
