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
//! Declarative menu items use the same non-owning capability rule without becoming widget nodes.
//! [`MenuItemHandle`] carries private stable identity and separately projects its weak
//! [`MenuItemSubmitted`] endpoint through [`MenuItemHandle::submitted`]; [`Ui::menu_item`] and
//! [`Ui::menu_item_mut`] lend the authoritative mounted
//! [`MenuItemParameters`]. Both return [`MenuItemAccessError::UnknownItem`] for an unmounted,
//! destroyed, or foreign item capability. The same recursive [`Menu`] declaration installs below
//! a [`MenuBar`] heading or becomes an application-addressable compact popup through
//! [`Ui::create_menu_popup`]; both paths use the same manager-owned menu surface.
//!
//! # Update and paint boundary
//!
//! Input is appended through [`Context`] forwarding methods. [`Context::update_ui`] first
//! synchronizes layout, then drains that one ordered queue. Each raw event is normalized, routed,
//! applied by one full eligible-tree update traversal, and followed by layout before the next
//! event. Calling it with an empty queue is the synchronization path after programmatic state or
//! topology mutation. [`ContextFrame::render_ui`] is paint-only and returns
//! [`render::RenderError::UiUpdateRequired`] before backend acquisition when the commit is missing,
//! raw input is pending, an invalidating typed mutation succeeded in a visible tree, or the frame
//! dimensions differ. Specialized measurement-preserving setters may leave the current commit
//! renderable while derived state waits for the next update. Applications call `update_ui` after
//! every programmatic mutation before relying on its visual or layout result. The runtime
//! synthesizes no timer events and produces no generic frame-result/resource-state object.
//!
//! A `ContextFrame` serializes Context operations but does not lock independent typed handles, and
//! there is no Context token. Typed access closures must finish before retained traversal reaches
//! the same widget. Framework-controlled recursion through a container's opaque
//! child visitor is the intentional exception. An invalidating handle mutation after the last
//! commit makes `render_ui` reject that commit until `update_ui` consumes the marker and
//! synchronizes layout again.
//! Handles and Context are independent Rust values, so explicitly capturing Context inside
//! `try_update` compiles; it is nevertheless unsupported because the closure retains the mutable
//! widget borrow. If nested traversal reaches that allocation, runtime borrowing reports an
//! invariant panic. End the closure before calling `update_ui` or `render_ui`.
//!
//! Event-driven applications construct `Context::<B, State>::new(backend)`, register each
//! native widget endpoint through [`Context::subscribe`] or [`Context::subscribe_with`], and call
//! [`Context::update_ui_state`]. The context owns the only application widget-event dispatcher for
//! all retained windows. This semantic dispatcher is distinct from retained raw-input routing:
//! each retained runtime owns an input router that targets pointer, keyboard, or text input to one
//! node, and that widget may then emit a typed event for the application dispatcher.
//! Application-owned library components such as [`FileDialog`] bind their controls to that same
//! dispatcher through an accessor into application state. Each event port owns its pending
//! payloads and accepts one state method.
//!
//! Backends forward one logical [`KeyEvent`] per press or release and send composed UTF-8 through
//! [`Context::text`] separately. Focus persists independently from pointer capture. In the active
//! eligible window, Tab and Shift+Tab wrap through retained [`KeyboardBehavior::TAB_STOP`]
//! surfaces; built-in controls derive Windows-style activation, adjustment, hierarchy, and popup
//! actions from the same [`KeyboardAction`] mapping. Ctrl+F6 and Ctrl+Shift+F6 cycle visible
//! ordinary windows while preserving each window's focused widget. F10 or an unchorded Alt tap
//! transfers routing temporarily to the owning window's intrinsic menu without discarding
//! application widget focus.
//! Only the current keyboard scope paints that remembered focus. Each semantic role and
//! interaction state resolves one complete [`Visual`] containing both background art and its
//! matching semantic-content color. Focus and window activation therefore select ordinary role/state visuals
//! instead of adding a second widget-independent paint effect.
//!
//! # Text encoding and glyph coverage
//!
//! Public text uses Rust [`str`] and [`String`] values and is therefore valid UTF-8. Textbox removes
//! CR and LF at every ingress; text-area and text-block storage converts CRLF and lone CR to LF.
//! Textbox and text-area cursors are byte indices kept on Unicode scalar-value boundaries;
//! movement and deletion operate on scalar values rather than grapheme clusters.
//!
//! Rendering coverage belongs to the selected atlas font. The built-in atlas builder bakes
//! printable ASCII (`U+0020` through `U+007E`). A missing character uses the font's required
//! underscore glyph; validated custom [`AtlasSource`] tables may provide arbitrary Unicode scalar
//! values but must include exactly one `_` per font. The renderer does not perform script shaping,
//! bidirectional reordering, grapheme
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
//! ) -> Result<WindowHandle, RenderError> {
//!     let (_button, button_node) = Button::create(ButtonParameters::new("Save"));
//!     let window = context.ui().create_window(Window::new(
//!         "main",
//!         rect(20, 20, 180, 80),
//!         button_node,
//!     ));
//!
//!     context.update_ui(dimensions);
//!     context.frame(info).render_ui()?;
//!     Ok(window)
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
//! The [`retained`] module and repository examples document the 0.8 retained-authoring API.
//! This release is versioned `0.8.0` and uses one concrete surface forest. Sole parent
//! edges encode structural child-window, dialog, and popup ownership; forest storage records
//! chronological window order; and one deepest-popup identity derives the visible transient branch.
//! Within a child family, parent content records below descendants while the parent's intrinsic menu
//! and chrome record and handle above them. Window-intrinsic declarative menu surfaces share that
//! ownership model without erased payloads.
//!
//! # Rendering pipeline
//!
//! Drawing is recorded before it reaches the backend:
//!
//! ```text
//! Widget::paint
//!      |
//!      v
//!   Painter  --->  DisplayList  --->  Context executor  --->  RendererBackend::Frame
//! (records)         (owns ops)        (executes)            (submits/presents)
//! ```
//!
//! Widgets obtain a [`render::Painter`] from [`WidgetPaintCtx::painter`] and record
//! backend-neutral operations. A crate-private Context executor consumes the resulting display
//! list, performs final clipping and tessellation, and submits final
//! [`render::Vertex`] values through [`render::RendererBackend`]. Applications normally import
//! retained UI types from [`prelude`], while backend integrations import frame contracts from
//! [`render`].

// Compile the Rust examples in standalone guides as part of `cargo test --doc` without publishing
// duplicate guide modules in the normal API reference.
#[cfg(doctest)]
mod guide_doctests {
    #[doc = include_str!("../docs/ARCHITECTURE.md")]
    mod architecture {}
    #[doc = include_str!("../docs/BACKENDS.md")]
    mod backends {}
    #[doc = include_str!("../docs/LAYOUT.md")]
    mod layout {}
    #[doc = include_str!("../docs/MENUS.md")]
    mod menus {}
    #[doc = include_str!("../docs/SKINNING.md")]
    mod skinning {}
    #[doc = include_str!("../docs/THEMES.md")]
    mod themes {}
    #[doc = include_str!("../docs/TYPOGRAPHY.md")]
    mod typography {}
    #[doc = include_str!("../docs/WIDGETS.md")]
    mod widgets {}
}

pub mod atlas;
mod context;
mod event;
mod file_dialog;
mod identity;
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
    pub use crate::menu::{Menu, MenuBar, MenuItem, MenuItemAccessError, MenuItemHandle, MenuItemMark, MenuItemParameters, MenuItemSubmitted};
    pub use crate::render::{CustomRenderArgs, CustomRenderHandle};
    pub use crate::ui_node::{
        AvailableSpace, ChildParticipation, Children, Constraints, Container, ContainerLayoutCtx, ContainerWidget, Disclosure, DisclosureParameters, Grid,
        GridItem, GridParameters, GridSpan, KeyboardAction, KeyboardBehavior, LeafWidget, Linear, LinearCrossSize, LinearDirection, LinearItem,
        LinearParameters, MeasureCtx, Node, ScrollArea, ScrollAreaOption, ScrollAreaParameters, Scrollbar, ScrollbarAxis, ScrollbarChanged,
        ScrollbarParameters, TrackSize, UiInputEvent, Widget, TextWrap, TypedWidgetHandle, WidgetBuilder, WidgetFillOption, WidgetOption, WidgetPaintCtx,
        WidgetParameters, WidgetUpdateCtx,
    };
    pub use crate::context::{Context, ContextFrame, Ui};
    pub use crate::window_manager::{
        ChildWindowClip, DEFAULT_LAYER, LayerBinding, MAX_LAYER, MIN_LAYER, PopupEvent, PopupHandle, SurfaceCreationError, SurfaceMutationError, Window,
        WindowEvent, WindowHandle, WindowOption,
    };
}

/// Common imports for retained UI applications.
///
/// The prelude intentionally favors retained authoring, widget state, skin/input/image types, and
/// renderer integration. Low-level backend contracts live under [`render`].
pub mod prelude {
    pub use crate::event::{SubscribeError, TypedWidget, WidgetEvent, WidgetEventPortHandle};
    pub use crate::atlas::{AtlasError, AtlasHandle, AtlasSource, CharEntry, FontEntry, FontId, IconId, SourceFormat};
    pub use crate::file_dialog::{FileDialog, FileDialogCompleted, FileDialogRequest, FileDialogResult, FileDialogStatus};
    pub use crate::menu::{Menu, MenuBar, MenuItem, MenuItemAccessError, MenuItemHandle, MenuItemMark, MenuItemParameters, MenuItemSubmitted};
    pub use crate::image::{ImageError, ImageSource, ImageStorageError, MAX_DECODED_RGBA_BYTES, load_image_bytes};
    pub use crate::input::{Key, KeyEvent, KeyState, Modifiers, MouseButton};
    pub use crate::render::{
        AtlasUploadError, FrameError, FrameInfo, FrameInfoError, NinePatch, NinePatchCell, NinePatchCells, NinePatchContent, NinePatchImage, RendererBackend,
        RendererFrame, SliceInsets, TextureError, TextureId,
    };
    pub use crate::retained::{
        ChildParticipation, ChildWindowClip, Children, Container, ContainerLayoutCtx, ContainerWidget, Context, ContextFrame, Ui, CustomRenderArgs,
        AvailableSpace, Constraints, CustomRenderHandle, Disclosure, DisclosureParameters, Grid, GridItem, GridParameters, GridSpan, KeyboardAction,
        KeyboardBehavior, Linear, LinearCrossSize, LinearDirection, LinearItem, LinearParameters, Node, PopupEvent, PopupHandle, MeasureCtx,
        SurfaceCreationError, SurfaceMutationError, WindowEvent, WindowHandle, LayerBinding, DEFAULT_LAYER, MAX_LAYER, MIN_LAYER, ScrollArea, ScrollAreaOption,
        ScrollAreaParameters, Scrollbar, ScrollbarAxis, ScrollbarChanged, ScrollbarParameters, TrackSize, LeafWidget, TextWrap, UiInputEvent,
        TypedWidgetHandle, Widget, WidgetBuilder, WidgetFillOption, WidgetOption, WidgetPaintCtx, WidgetParameters, WidgetUpdateCtx, Window, WindowOption,
    };
    pub use crate::math::{expand_rect, rect, vec2};
    pub use crate::theme::{
        CaptionButtonSide, CaptionButtonsSkin, ChromeRole, ChromeState, Color, ControlRole, ControlState, FlatPalette, FontRef, FontRole, FrameRole, IconRef,
        IconRole, MenuRole, MenuState, PointerState, ResourceCatalog, Skin, SkinBundle, SkinMetrics, SurfaceRole, SurfaceState, TitleBackdropSkin, Visual,
        WindowChromeSkin, WindowTitleAlignment, color,
    };
    #[cfg(feature = "theme-json")]
    pub use crate::theme::{LoadedTheme, THEME_SCHEMA_VERSION, ThemeLoadError};
    pub use crate::widgets::{
        Button, ButtonBuilder, ButtonContent, ButtonParameters, ButtonSubmitted, Checkbox, CheckboxBuilder, CheckboxChanged, CheckboxParameters, ColorSwatch,
        ColorSwatchBuilder, ColorSwatchParameters, Combo, ComboBuilder, ComboChanged, ComboParameters, ComboSubmitted, Custom, CustomBuilder, CustomParameters,
        DecimalPrecision, DecimalPrecisionError, ListBox, ListBoxBuilder, ListBoxParameters, ListBoxSubmitted, ListItem, ListItemBuilder, ListItemParameters,
        ListItemSubmitted, Number, NumberBuilder, NumberChanged, NumberParameters, NumericParameterError, Slider, SliderBuilder, SliderChanged,
        SliderParameters, TextArea, TextAreaChanged, TextAreaParameters, TextAreaSubmitted, TextBlock, TextBlockBuilder, TextBlockParameters, Textbox,
        TextboxBuilder, TextboxChanged, TextboxParameters, TextboxSubmitted, Real,
    };
    pub use rs_math3d::{
        Box3f, Color4b, CrossProduct, Dimension, Dimensioni, FloatVector, Mat4f, Quat, Quatf, Rect, Recti, Vec2f, Vec2i, Vec3f, Vec4f, Vector, Vector3,
        color4b, ortho4,
    };
}

pub use atlas::{AtlasError, AtlasHandle, AtlasSource, CharEntry, FontEntry, FontId, IconId, SourceFormat};
pub use context::{Context, ContextFrame, Ui};
pub use window_manager::{
    ChildWindowClip, DEFAULT_LAYER, LayerBinding, MAX_LAYER, MIN_LAYER, PopupEvent, PopupHandle, SurfaceCreationError, SurfaceMutationError, Window,
    WindowEvent, WindowHandle, WindowOption,
};
pub use event::{SubscribeError, TypedWidget, WidgetEvent, WidgetEventPortHandle};
pub use file_dialog::{FileDialog, FileDialogCompleted, FileDialogRequest, FileDialogResult, FileDialogStatus};
pub use menu::{Menu, MenuBar, MenuItem, MenuItemAccessError, MenuItemHandle, MenuItemMark, MenuItemParameters, MenuItemSubmitted};
pub use image::{ImageError, ImageSource, ImageStorageError, MAX_DECODED_RGBA_BYTES, load_image_bytes};
pub use input::{Key, KeyEvent, KeyState, Modifiers, MouseButton};
pub use math::{expand_rect, rect, vec2};
pub use render::{AtlasUploadError, NinePatch, NinePatchCell, NinePatchCells, NinePatchContent, NinePatchImage, SliceInsets, TextureError, TextureId};
pub use theme::{
    CaptionButtonSide, CaptionButtonsSkin, ChromeRole, ChromeState, Color, ControlRole, ControlState, FlatPalette, FontRef, FontRole, FrameRole, IconRef,
    IconRole, MenuRole, MenuState, PointerState, ResourceCatalog, Skin, SkinBundle, SkinMetrics, SurfaceRole, SurfaceState, TitleBackdropSkin, Visual,
    WindowChromeSkin, WindowTitleAlignment, color,
};
#[cfg(feature = "theme-json")]
pub use theme::{LoadedTheme, THEME_SCHEMA_VERSION, ThemeLoadError};
pub use ui_node::{
    AvailableSpace, ChildParticipation, Children, Constraints, Container, ContainerLayoutCtx, ContainerWidget, Disclosure, DisclosureParameters, Grid,
    GridItem, GridParameters, GridSpan, KeyboardAction, KeyboardBehavior, LeafWidget, Linear, LinearCrossSize, LinearDirection, LinearItem, LinearParameters,
    MeasureCtx, Node, ScrollArea, ScrollAreaOption, ScrollAreaParameters, Scrollbar, ScrollbarAxis, ScrollbarChanged, ScrollbarParameters, TextWrap, TrackSize,
    UiInputEvent, Widget, TypedWidgetHandle, WidgetBuilder, WidgetFillOption, WidgetOption, WidgetPaintCtx, WidgetParameters, WidgetUpdateCtx,
};
pub use widgets::{
    Button, ButtonBuilder, ButtonContent, ButtonParameters, ButtonSubmitted, Checkbox, CheckboxBuilder, CheckboxChanged, CheckboxParameters, ColorSwatch,
    ColorSwatchBuilder, ColorSwatchParameters, Combo, ComboBuilder, ComboChanged, ComboParameters, ComboSubmitted, Custom, CustomBuilder, CustomParameters,
    DecimalPrecision, DecimalPrecisionError, ListBox, ListBoxBuilder, ListBoxParameters, ListBoxSubmitted, ListItem, ListItemBuilder, ListItemParameters,
    ListItemSubmitted, Number, NumberBuilder, NumberChanged, NumberParameters, NumericParameterError, Slider, SliderBuilder, SliderChanged, SliderParameters,
    TextArea, TextAreaChanged, TextAreaParameters, TextAreaSubmitted, Real, TextBlock, TextBlockBuilder, TextBlockParameters, Textbox, TextboxBuilder,
    TextboxChanged, TextboxParameters, TextboxSubmitted,
};

#[allow(unused_imports)]
pub(crate) use rs_math3d::{
    Box3f, Color4b, CrossProduct, Dimension, Dimensioni, FloatVector, Mat4f, Quat, Quatf, Rect, Recti, Vec2f, Vec2i, Vec3f, Vec4f, Vector, Vector3, color4b,
    ortho4,
};
pub(crate) use std::{cmp::max, hash::Hash, rc::Rc};
pub(crate) use render::geometry::UNCLIPPED_RECT;
pub(crate) use ui_node::UiRuntime;
