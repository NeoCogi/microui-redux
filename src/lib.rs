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
//! `microui-redux` provides an immediate-mode GUI toolkit inspired by [rxi/microui](https://github.com/rxi/microui).
//! The crate exposes the core context, container, layout, and renderer hooks necessary to embed a UI inside
//! custom render backends while remaining allocator- and platform-agnostic.

use std::{
    cell::{Ref, RefCell, RefMut},
    f32,
    hash::Hash,
    rc::Rc,
    sync::Arc,
};

#[cfg(any(feature = "builder", feature = "png_source"))]
use std::io::Cursor;

#[cfg(any(feature = "builder", feature = "png_source"))]
use png::{ColorType, Decoder};

mod atlas;
mod canvas;
mod container;
mod file_dialog;
mod idmngr;
mod layout;
mod rect_packer;
mod window;

pub use atlas::*;
pub use canvas::*;
pub use container::*;
pub use idmngr::*;
pub use layout::SizePolicy;
pub use rect_packer::*;
pub use rs_math3d::*;
pub use window::*;
pub use file_dialog::*;

use layout::LayoutManager;

use bitflags::*;
use std::cmp::{max, min};
use std::sync::RwLock;

#[derive(Debug, Copy, Clone)]
/// Tracks input button transitions seen since the previous frame.
pub enum InputButtonState {
    /// No interaction was registered.
    None,
    /// The button was pressed this frame, storing the press timestamp.
    Pressed(f32),
    /// The button was released this frame.
    Released,
    /// The scroll wheel moved by the given amount.
    Scroll(f32),
}

#[derive(Debug, Copy, Clone)]
/// Records the latest pointer interaction that occurred over a widget.
pub enum MouseEvent {
    /// No pointer activity occurred.
    None,
    /// The pointer clicked at the given pixel position.
    Click(Vec2i),
    /// The pointer is being dragged between two positions.
    Drag {
        /// Position where the drag originated.
        prev_pos: Vec2i,
        /// Current drag position.
        curr_pos: Vec2i,
    },
    /// The pointer moved to a new coordinate without interacting.
    Move(Vec2i),
}

/// Trait implemented by render backends used by the UI context.
pub trait Renderer {
    /// Returns the atlas backing the renderer.
    fn get_atlas(&self) -> AtlasHandle;
    /// Begins a new frame with the viewport size and clear color.
    fn begin(&mut self, width: i32, height: i32, clr: Color);
    /// Pushes four vertices representing a quad to the backend.
    fn push_quad_vertices(&mut self, v0: &Vertex, v1: &Vertex, v2: &Vertex, v3: &Vertex);
    /// Flushes any buffered geometry to the GPU.
    fn flush(&mut self);
    /// Ends the frame, finalizing any outstanding GPU work.
    fn end(&mut self);
    /// Creates a texture owned by the renderer.
    fn create_texture(&mut self, id: TextureId, width: i32, height: i32, pixels: &[u8]);
    /// Destroys a previously created texture.
    fn destroy_texture(&mut self, id: TextureId);
    /// Draws the provided textured quad.
    fn draw_texture(&mut self, id: TextureId, vertices: [Vertex; 4]);
}

/// Thread-safe handle that shares ownership of a [`Renderer`].
pub struct RendererHandle<R: Renderer> {
    handle: Arc<RwLock<R>>,
}

// seems there's a bug in #[derive(Clone)] as it's unable to induce that Arc is sufficient
impl<R: Renderer> Clone for RendererHandle<R> {
    fn clone(&self) -> Self { Self { handle: self.handle.clone() } }
}

impl<R: Renderer> RendererHandle<R> {
    /// Wraps a renderer inside an [`Arc<RwLock<...>>`] so it can be shared.
    pub fn new(renderer: R) -> Self { Self { handle: Arc::new(RwLock::new(renderer)) } }

    /// Executes the provided closure with a shared reference to the renderer.
    pub fn scope<Res, F: Fn(&R) -> Res>(&self, f: F) -> Res {
        match self.handle.read() {
            Ok(guard) => f(&*guard),
            Err(poisoned) => {
                // Handle poisoned lock by using the data anyway
                // This is safe because we're just reading
                f(&*poisoned.into_inner())
            }
        }
    }

    /// Executes the provided closure with a mutable reference to the renderer.
    pub fn scope_mut<Res, F: FnMut(&mut R) -> Res>(&mut self, mut f: F) -> Res {
        match self.handle.write() {
            Ok(mut guard) => f(&mut *guard),
            Err(poisoned) => {
                // Handle poisoned lock by using the data anyway
                // Clear the poison and continue
                f(&mut *poisoned.into_inner())
            }
        }
    }
}

#[derive(PartialEq, Copy, Clone)]
#[repr(u32)]
/// Describes whether a rectangle is clipped by the current scissor.
pub enum Clip {
    /// Rectangle is fully visible.
    None = 0,
    /// Rectangle is partially visible.
    Part = 1,
    /// Rectangle is fully clipped away.
    All = 2,
}

#[derive(PartialEq, Copy, Clone)]
#[repr(u32)]
/// Identifiers for each of the built-in style colors.
pub enum ControlColor {
    /// Number of color entries in [`Style::colors`].
    Max = 14,
    /// Thumb of scrollbars.
    ScrollThumb = 13,
    /// Base frame of scrollbars.
    ScrollBase = 12,
    /// Base color for focused widgets.
    BaseFocus = 11,
    /// Base color while the pointer hovers the widget.
    BaseHover = 10,
    /// Default base color.
    Base = 9,
    /// Button color while the widget is focused.
    ButtonFocus = 8,
    /// Button color while the pointer hovers the widget.
    ButtonHover = 7,
    /// Default button color.
    Button = 6,
    /// Panel background color.
    PanelBG = 5,
    /// Window title text color.
    TitleText = 4,
    /// Window title background color.
    TitleBG = 3,
    /// Window background color.
    WindowBG = 2,
    /// Outline/border color.
    Border = 1,
    /// Default text color.
    Text = 0,
}

impl ControlColor {
    /// Promotes the enum to the hover variant when relevant.
    pub fn hover(&mut self) {
        *self = match self {
            Self::Base => Self::BaseHover,
            Self::Button => Self::ButtonHover,
            _ => *self,
        }
    }

    /// Promotes the enum to the focused variant when relevant.
    pub fn focus(&mut self) {
        *self = match self {
            Self::Base => Self::BaseFocus,
            Self::Button => Self::ButtonFocus,
            Self::BaseHover => Self::BaseFocus,
            Self::ButtonHover => Self::ButtonFocus,
            _ => *self,
        }
    }
}

bitflags! {
    /// State bits returned by widgets to describe their interaction outcome.
    pub struct ResourceState : u32 {
        /// Indicates that the widget's data changed.
        const CHANGE = 4;
        /// Indicates that the widget was submitted (e.g. button clicked).
        const SUBMIT = 2;
        /// Indicates that the widget is currently active.
        const ACTIVE = 1;
        /// Indicates no interaction.
        const NONE = 0;
    }
}

impl ResourceState {
    /// Returns `true` if the widget changed its bound value.
    pub fn is_changed(&self) -> bool { self.intersects(Self::CHANGE) }
    /// Returns `true` if the widget signaled submission.
    pub fn is_submitted(&self) -> bool { self.intersects(Self::SUBMIT) }
    /// Returns `true` if the widget is active.
    pub fn is_active(&self) -> bool { self.intersects(Self::ACTIVE) }
    /// Returns `true` if the state contains no flags.
    pub fn is_none(&self) -> bool { self.bits() == 0 }
}

bitflags! {
        #[derive(Copy, Clone)]
    /// Options that control how a container behaves.
    pub struct ContainerOption : u32 {
        /// Automatically adapts the container size to its content.
        const AUTO_SIZE = 512;
        /// Hides the title bar.
        const NO_TITLE = 128;
        /// Hides the close button.
        const NO_CLOSE = 64;
        /// Prevents the user from resizing the window.
        const NO_RESIZE = 16;
        /// Hides the outer frame.
        const NO_FRAME = 8;
        /// Disables hit testing for the container.
        const NO_INTERACT = 4;
        /// No special options.
        const NONE = 0;
    }

    #[derive(Copy, Clone)]
    /// Widget specific options that influence layout and interactivity.
    pub struct WidgetOption : u32 {
        /// Keeps keyboard focus while the widget is held.
        const HOLD_FOCUS = 256;
        /// Draws the widget without its frame/background.
        const NO_FRAME = 128;
        /// Disables interaction for the widget.
        const NO_INTERACT = 4;
        /// Aligns the widget to the right side of the cell.
        const ALIGN_RIGHT = 2;
        /// Centers the widget inside the cell.
        const ALIGN_CENTER = 1;
        /// No special options.
        const NONE = 0;
    }

    #[derive(Copy, Clone)]
    /// Controls which widget states should draw a filled background.
    pub struct WidgetFillOption : u32 {
        /// Fill the background for the idle/normal state.
        const NORMAL = 1;
        /// Fill the background while hovered.
        const HOVER = 2;
        /// Fill the background while actively clicked.
        const CLICK = 4;
        /// Fill the background for every interaction state.
        const ALL = Self::NORMAL.bits() | Self::HOVER.bits() | Self::CLICK.bits();
    }
}

#[derive(Copy, Clone, Debug, PartialEq, Eq)]
/// Behaviour options that control how widgets and containers handle input side effects.
pub enum WidgetBehaviourOption {
    /// No special behaviour.
    None,
    /// Consume pending scroll when the widget is hovered.
    GrabScroll,
    /// Disable container scroll handling.
    NoScroll,
}

impl WidgetBehaviourOption {
    /// No special behaviour.
    pub const NONE: Self = Self::None;
    /// Consume pending scroll when the widget is hovered.
    pub const GRAB_SCROLL: Self = Self::GrabScroll;
    /// Disable container scroll handling.
    pub const NO_SCROLL: Self = Self::NoScroll;

    /// Returns `true` if the option enables scroll grabbing for a widget.
    pub fn is_grab_scroll(self) -> bool { matches!(self, Self::GrabScroll) }
    /// Returns `true` if the option disables container scroll handling.
    pub fn is_no_scroll(self) -> bool { matches!(self, Self::NoScroll) }
}

/// Context passed to widget handlers.
pub struct WidgetCtx<'a> {
    _marker: std::marker::PhantomData<&'a mut ()>,
}

#[derive(Copy, Clone, Default, Debug)]
/// Captures the interaction state for a widget during the current frame.
pub struct ControlState {
    /// Cursor is hovering the widget.
    pub hovered: bool,
    /// Widget currently owns focus.
    pub focused: bool,
    /// Mouse was pressed on the widget this frame.
    pub clicked: bool,
    /// Mouse is held down while the widget is focused.
    pub active: bool,
    /// Scroll delta consumed by this widget, if any.
    pub scroll_delta: Option<Vec2i>,
}

/// Trait implemented by persistent widget state structures.
pub trait WidgetState {
    /// Returns the widget options for this state.
    fn widget_opt(&self) -> &WidgetOption;
    /// Returns the behaviour options for this state.
    fn behaviour_opt(&self) -> &WidgetBehaviourOption;
    /// Returns the widget identifier for this state.
    fn get_id(&self) -> Id { Id::from_ptr(self) }
    /// Handles widget interaction and rendering for the current frame.
    fn handle(&mut self, ctx: &mut WidgetCtx<'_>, control: &ControlState) -> ResourceState;
}

impl WidgetState for (WidgetOption, WidgetBehaviourOption) {
    fn widget_opt(&self) -> &WidgetOption { &self.0 }
    fn behaviour_opt(&self) -> &WidgetBehaviourOption { &self.1 }
    fn handle(&mut self, _ctx: &mut WidgetCtx<'_>, _control: &ControlState) -> ResourceState { ResourceState::NONE }
}

#[derive(Clone, Copy)]
/// Expansion state used by tree nodes, headers, and similar widgets.
pub enum NodeStateValue {
    /// Child content is visible.
    Expanded,
    /// Child content is hidden.
    Closed,
}

impl NodeStateValue {
    /// Returns `true` when the node is expanded.
    pub fn is_expanded(&self) -> bool {
        match self {
            Self::Expanded => true,
            _ => false,
        }
    }

    /// Returns `true` when the node is closed.
    pub fn is_closed(&self) -> bool {
        match self {
            Self::Closed => true,
            _ => false,
        }
    }
}

#[derive(Clone)]
/// Persistent state for headers and tree nodes.
pub struct NodeState {
    /// Label displayed for the node.
    pub label: String,
    /// Current expansion state.
    pub state: NodeStateValue,
    /// Widget options applied to the node.
    pub opt: WidgetOption,
    /// Behaviour options applied to the node.
    pub bopt: WidgetBehaviourOption,
}

impl NodeState {
    /// Creates a node state with the default widget options.
    pub fn new(label: impl Into<String>, state: NodeStateValue) -> Self {
        Self { label: label.into(), state, opt: WidgetOption::NONE, bopt: WidgetBehaviourOption::NONE }
    }

    /// Creates a node state with explicit widget options.
    pub fn with_opt(label: impl Into<String>, state: NodeStateValue, opt: WidgetOption) -> Self {
        Self { label: label.into(), state, opt, bopt: WidgetBehaviourOption::NONE }
    }

    /// Returns `true` when the node is expanded.
    pub fn is_expanded(&self) -> bool { self.state.is_expanded() }

    /// Returns `true` when the node is closed.
    pub fn is_closed(&self) -> bool { self.state.is_closed() }
}

impl WidgetState for NodeState {
    fn widget_opt(&self) -> &WidgetOption { &self.opt }
    fn behaviour_opt(&self) -> &WidgetBehaviourOption { &self.bopt }
    fn handle(&mut self, _ctx: &mut WidgetCtx<'_>, _control: &ControlState) -> ResourceState { ResourceState::NONE }
}

#[derive(Clone)]
/// Describes the content rendered inside a button widget.
pub enum ButtonContent {
    /// A text label and optional icon from the atlas.
    Text {
        /// Text displayed on the button.
        label: String,
        /// Optional icon rendered on the button.
        icon: Option<IconId>,
    },
    /// A text label and optional image.
    Image {
        /// Text displayed on the button.
        label: String,
        /// Optional image rendered on the button.
        image: Option<Image>,
    },
    /// A text label and a slot refreshed via a paint callback.
    Slot {
        /// Text displayed on the button.
        label: String,
        /// Slot rendered on the button.
        slot: SlotId,
        /// Callback used to fill the slot pixels.
        paint: Rc<dyn Fn(usize, usize) -> Color4b>,
    },
}

#[derive(Clone)]
/// Persistent state for button widgets.
pub struct ButtonState {
    /// Content rendered inside the button.
    pub content: ButtonContent,
    /// Widget options applied to the button.
    pub opt: WidgetOption,
    /// Behaviour options applied to the button.
    pub bopt: WidgetBehaviourOption,
    /// Fill behavior for the button background.
    pub fill: WidgetFillOption,
}

impl ButtonState {
    /// Creates a text button with default options.
    pub fn new(label: impl Into<String>) -> Self {
        Self {
            content: ButtonContent::Text { label: label.into(), icon: None },
            opt: WidgetOption::NONE,
            bopt: WidgetBehaviourOption::NONE,
            fill: WidgetFillOption::ALL,
        }
    }

    /// Creates a text button with explicit widget options.
    pub fn with_opt(label: impl Into<String>, opt: WidgetOption) -> Self {
        Self {
            content: ButtonContent::Text { label: label.into(), icon: None },
            opt,
            bopt: WidgetBehaviourOption::NONE,
            fill: WidgetFillOption::ALL,
        }
    }

    /// Creates an image button with explicit widget options and fill behavior.
    pub fn with_image(label: impl Into<String>, image: Option<Image>, opt: WidgetOption, fill: WidgetFillOption) -> Self {
        Self {
            content: ButtonContent::Image { label: label.into(), image },
            opt,
            bopt: WidgetBehaviourOption::NONE,
            fill,
        }
    }

    /// Creates a slot button that repaints via the provided callback.
    pub fn with_slot(
        label: impl Into<String>,
        slot: SlotId,
        paint: Rc<dyn Fn(usize, usize) -> Color4b>,
        opt: WidgetOption,
        fill: WidgetFillOption,
    ) -> Self {
        Self {
            content: ButtonContent::Slot { label: label.into(), slot, paint },
            opt,
            bopt: WidgetBehaviourOption::NONE,
            fill,
        }
    }
}

impl WidgetState for ButtonState {
    fn widget_opt(&self) -> &WidgetOption { &self.opt }
    fn behaviour_opt(&self) -> &WidgetBehaviourOption { &self.bopt }
    fn handle(&mut self, _ctx: &mut WidgetCtx<'_>, _control: &ControlState) -> ResourceState { ResourceState::NONE }
}

#[derive(Clone)]
/// Persistent state for list items.
pub struct ListItemState {
    /// Label displayed for the list item.
    pub label: String,
    /// Optional atlas icon rendered alongside the label.
    pub icon: Option<IconId>,
    /// Widget options applied to the list item.
    pub opt: WidgetOption,
    /// Behaviour options applied to the list item.
    pub bopt: WidgetBehaviourOption,
}

impl ListItemState {
    /// Creates a list item with default widget options.
    pub fn new(label: impl Into<String>) -> Self {
        Self { label: label.into(), icon: None, opt: WidgetOption::NONE, bopt: WidgetBehaviourOption::NONE }
    }

    /// Creates a list item with explicit widget options.
    pub fn with_opt(label: impl Into<String>, opt: WidgetOption) -> Self {
        Self { label: label.into(), icon: None, opt, bopt: WidgetBehaviourOption::NONE }
    }

    /// Creates a list item with an icon and default widget options.
    pub fn with_icon(label: impl Into<String>, icon: IconId) -> Self {
        Self { label: label.into(), icon: Some(icon), opt: WidgetOption::NONE, bopt: WidgetBehaviourOption::NONE }
    }

    /// Creates a list item with an icon and explicit widget options.
    pub fn with_icon_opt(label: impl Into<String>, icon: IconId, opt: WidgetOption) -> Self {
        Self { label: label.into(), icon: Some(icon), opt, bopt: WidgetBehaviourOption::NONE }
    }
}

impl WidgetState for ListItemState {
    fn widget_opt(&self) -> &WidgetOption { &self.opt }
    fn behaviour_opt(&self) -> &WidgetBehaviourOption { &self.bopt }
    fn handle(&mut self, _ctx: &mut WidgetCtx<'_>, _control: &ControlState) -> ResourceState { ResourceState::NONE }
}

#[derive(Clone)]
/// Persistent state for list boxes.
pub struct ListBoxState {
    /// Label displayed for the list box.
    pub label: String,
    /// Optional image rendered alongside the label.
    pub image: Option<Image>,
    /// Widget options applied to the list box.
    pub opt: WidgetOption,
    /// Behaviour options applied to the list box.
    pub bopt: WidgetBehaviourOption,
}

impl ListBoxState {
    /// Creates a list box with default widget options.
    pub fn new(label: impl Into<String>, image: Option<Image>) -> Self {
        Self { label: label.into(), image, opt: WidgetOption::NONE, bopt: WidgetBehaviourOption::NONE }
    }

    /// Creates a list box with explicit widget options.
    pub fn with_opt(label: impl Into<String>, image: Option<Image>, opt: WidgetOption) -> Self {
        Self { label: label.into(), image, opt, bopt: WidgetBehaviourOption::NONE }
    }
}

impl WidgetState for ListBoxState {
    fn widget_opt(&self) -> &WidgetOption { &self.opt }
    fn behaviour_opt(&self) -> &WidgetBehaviourOption { &self.bopt }
    fn handle(&mut self, _ctx: &mut WidgetCtx<'_>, _control: &ControlState) -> ResourceState { ResourceState::NONE }
}

#[derive(Clone)]
/// Persistent state for checkbox widgets.
pub struct CheckboxState {
    /// Label displayed for the checkbox.
    pub label: String,
    /// Current value of the checkbox.
    pub value: bool,
    /// Widget options applied to the checkbox.
    pub opt: WidgetOption,
    /// Behaviour options applied to the checkbox.
    pub bopt: WidgetBehaviourOption,
}

impl CheckboxState {
    /// Creates a checkbox with default widget options.
    pub fn new(label: impl Into<String>, value: bool) -> Self {
        Self { label: label.into(), value, opt: WidgetOption::NONE, bopt: WidgetBehaviourOption::NONE }
    }

    /// Creates a checkbox with explicit widget options.
    pub fn with_opt(label: impl Into<String>, value: bool, opt: WidgetOption) -> Self {
        Self { label: label.into(), value, opt, bopt: WidgetBehaviourOption::NONE }
    }
}

impl WidgetState for CheckboxState {
    fn widget_opt(&self) -> &WidgetOption { &self.opt }
    fn behaviour_opt(&self) -> &WidgetBehaviourOption { &self.bopt }
    fn handle(&mut self, _ctx: &mut WidgetCtx<'_>, _control: &ControlState) -> ResourceState { ResourceState::NONE }
}

#[derive(Clone)]
/// Persistent state for textbox widgets.
pub struct TextboxState {
    /// Buffer edited by the textbox.
    pub buf: String,
    /// Widget options applied to the textbox.
    pub opt: WidgetOption,
    /// Behaviour options applied to the textbox.
    pub bopt: WidgetBehaviourOption,
}

impl TextboxState {
    /// Creates a textbox with default widget options.
    pub fn new(buf: impl Into<String>) -> Self {
        Self { buf: buf.into(), opt: WidgetOption::NONE, bopt: WidgetBehaviourOption::NONE }
    }

    /// Creates a textbox with explicit widget options.
    pub fn with_opt(buf: impl Into<String>, opt: WidgetOption) -> Self {
        Self { buf: buf.into(), opt, bopt: WidgetBehaviourOption::NONE }
    }
}

impl WidgetState for TextboxState {
    fn widget_opt(&self) -> &WidgetOption { &self.opt }
    fn behaviour_opt(&self) -> &WidgetBehaviourOption { &self.bopt }
    fn handle(&mut self, _ctx: &mut WidgetCtx<'_>, _control: &ControlState) -> ResourceState { ResourceState::NONE }
}

#[derive(Clone)]
/// Persistent state for slider widgets.
pub struct SliderState {
    /// Current slider value.
    pub value: Real,
    /// Lower bound of the slider range.
    pub low: Real,
    /// Upper bound of the slider range.
    pub high: Real,
    /// Step size used for snapping (0 for continuous).
    pub step: Real,
    /// Number of digits after the decimal point when rendering.
    pub precision: usize,
    /// Widget options applied to the slider.
    pub opt: WidgetOption,
    /// Behaviour options applied to the slider.
    pub bopt: WidgetBehaviourOption,
}

impl SliderState {
    /// Creates a slider with default widget options.
    pub fn new(value: Real, low: Real, high: Real) -> Self {
        Self {
            value,
            low,
            high,
            step: 0.0,
            precision: 0,
            opt: WidgetOption::NONE,
            bopt: WidgetBehaviourOption::GRAB_SCROLL,
        }
    }

    /// Creates a slider with explicit widget options.
    pub fn with_opt(value: Real, low: Real, high: Real, step: Real, precision: usize, opt: WidgetOption) -> Self {
        Self {
            value,
            low,
            high,
            step,
            precision,
            opt,
            bopt: WidgetBehaviourOption::GRAB_SCROLL,
        }
    }
}

impl WidgetState for SliderState {
    fn widget_opt(&self) -> &WidgetOption { &self.opt }
    fn behaviour_opt(&self) -> &WidgetBehaviourOption { &self.bopt }
    fn handle(&mut self, _ctx: &mut WidgetCtx<'_>, _control: &ControlState) -> ResourceState { ResourceState::NONE }
}

#[derive(Clone)]
/// Persistent state for number input widgets.
pub struct NumberState {
    /// Current number value.
    pub value: Real,
    /// Step applied when dragging.
    pub step: Real,
    /// Number of digits after the decimal point when rendering.
    pub precision: usize,
    /// Widget options applied to the number input.
    pub opt: WidgetOption,
    /// Behaviour options applied to the number input.
    pub bopt: WidgetBehaviourOption,
}

impl NumberState {
    /// Creates a number input with default widget options.
    pub fn new(value: Real, step: Real, precision: usize) -> Self {
        Self { value, step, precision, opt: WidgetOption::NONE, bopt: WidgetBehaviourOption::NONE }
    }

    /// Creates a number input with explicit widget options.
    pub fn with_opt(value: Real, step: Real, precision: usize, opt: WidgetOption) -> Self {
        Self { value, step, precision, opt, bopt: WidgetBehaviourOption::NONE }
    }
}

impl WidgetState for NumberState {
    fn widget_opt(&self) -> &WidgetOption { &self.opt }
    fn behaviour_opt(&self) -> &WidgetBehaviourOption { &self.bopt }
    fn handle(&mut self, _ctx: &mut WidgetCtx<'_>, _control: &ControlState) -> ResourceState { ResourceState::NONE }
}

#[derive(Clone)]
/// Persistent state for custom render widgets.
pub struct CustomState {
    /// Label used for debugging or inspection.
    pub name: String,
    /// Widget options applied to the custom widget.
    pub opt: WidgetOption,
    /// Behaviour options applied to the custom widget.
    pub bopt: WidgetBehaviourOption,
}

impl CustomState {
    /// Creates a custom widget state with default options.
    pub fn new(name: impl Into<String>) -> Self {
        Self {
            name: name.into(),
            opt: WidgetOption::NONE,
            bopt: WidgetBehaviourOption::NONE,
        }
    }

    /// Creates a custom widget state with explicit options.
    pub fn with_opt(name: impl Into<String>, opt: WidgetOption, bopt: WidgetBehaviourOption) -> Self {
        Self { name: name.into(), opt, bopt }
    }
}

impl WidgetState for CustomState {
    fn widget_opt(&self) -> &WidgetOption { &self.opt }
    fn behaviour_opt(&self) -> &WidgetBehaviourOption { &self.bopt }
    fn handle(&mut self, _ctx: &mut WidgetCtx<'_>, _control: &ControlState) -> ResourceState { ResourceState::NONE }
}

#[derive(Clone)]
/// Persistent state for internal window/container controls.
pub struct InternalState {
    /// Stable tag describing the internal control.
    pub tag: &'static str,
    /// Widget options applied to the internal control.
    pub opt: WidgetOption,
    /// Behaviour options applied to the internal control.
    pub bopt: WidgetBehaviourOption,
}

impl InternalState {
    /// Creates an internal control state with a stable tag.
    pub fn new(tag: &'static str) -> Self {
        Self {
            tag,
            opt: WidgetOption::NONE,
            bopt: WidgetBehaviourOption::NONE,
        }
    }
}

impl WidgetState for InternalState {
    fn widget_opt(&self) -> &WidgetOption { &self.opt }
    fn behaviour_opt(&self) -> &WidgetBehaviourOption { &self.bopt }
    fn handle(&mut self, _ctx: &mut WidgetCtx<'_>, _control: &ControlState) -> ResourceState { ResourceState::NONE }
}

impl ContainerOption {
    /// Returns `true` if the option requests automatic sizing.
    pub fn is_auto_sizing(&self) -> bool { self.intersects(Self::AUTO_SIZE) }

    /// Returns `true` if the title bar should be hidden.
    pub fn has_no_title(&self) -> bool { self.intersects(Self::NO_TITLE) }

    /// Returns `true` if the close button should be hidden.
    pub fn has_no_close(&self) -> bool { self.intersects(Self::NO_CLOSE) }

    /// Returns `true` if the container is fixed-size.
    pub fn is_fixed(&self) -> bool { self.intersects(Self::NO_RESIZE) }
    /// Returns `true` if the outer frame is hidden.
    pub fn has_no_frame(&self) -> bool { self.intersects(Self::NO_FRAME) }
}

impl WidgetOption {
    /// Returns `true` if the widget should keep focus while held.
    pub fn is_holding_focus(&self) -> bool { self.intersects(WidgetOption::HOLD_FOCUS) }

    /// Returns `true` if the widget shouldn't draw its frame.
    pub fn has_no_frame(&self) -> bool { self.intersects(WidgetOption::NO_FRAME) }

    /// Returns `true` if the widget is non-interactive.
    pub fn is_not_interactive(&self) -> bool { self.intersects(WidgetOption::NO_INTERACT) }
    /// Returns `true` if the widget prefers right alignment.
    pub fn is_aligned_right(&self) -> bool { self.intersects(WidgetOption::ALIGN_RIGHT) }
    /// Returns `true` if the widget prefers centered alignment.
    pub fn is_aligned_center(&self) -> bool { self.intersects(WidgetOption::ALIGN_CENTER) }
    /// Returns `true` if the option set is empty.
    pub fn is_none(&self) -> bool { self.bits() == 0 }
}

impl WidgetFillOption {
    /// Returns `true` when the normal state should be filled.
    pub fn fill_normal(&self) -> bool { self.intersects(Self::NORMAL) }

    /// Returns `true` when the hover state should be filled.
    pub fn fill_hover(&self) -> bool { self.intersects(Self::HOVER) }

    /// Returns `true` when the clicked/active state should be filled.
    pub fn fill_click(&self) -> bool { self.intersects(Self::CLICK) }
}

bitflags! {
    #[derive(Copy, Clone, Debug)]
    /// Mouse button state as reported by the input system.
    pub struct MouseButton : u32 {
        /// Middle mouse button.
        const MIDDLE = 4;
        /// Right mouse button.
        const RIGHT = 2;
        /// Left mouse button.
        const LEFT = 1;
        /// No buttons pressed.
        const NONE = 0;
    }
}

impl MouseButton {
    /// Returns `true` if the middle mouse button is pressed.
    pub fn is_middle(&self) -> bool { self.intersects(Self::MIDDLE) }
    /// Returns `true` if the right mouse button is pressed.
    pub fn is_right(&self) -> bool { self.intersects(Self::RIGHT) }
    /// Returns `true` if the left mouse button is pressed.
    pub fn is_left(&self) -> bool { self.intersects(Self::LEFT) }
    /// Returns `true` if no mouse buttons are pressed.
    pub fn is_none(&self) -> bool { self.bits() == 0 }
}

bitflags! {
    #[derive(Copy, Clone, Debug)]
    /// Modifier key state tracked by the input system.
    pub struct KeyMode : u32 {
        /// Return/Enter key held.
        const RETURN = 16;
        /// Backspace key held.
        const BACKSPACE = 8;
        /// Alt key held.
        const ALT = 4;
        /// Control key held.
        const CTRL = 2;
        /// Shift key held.
        const SHIFT = 1;
        /// No modifiers active.
        const NONE = 0;
    }
}

impl KeyMode {
    /// Returns `true` if no modifiers are active.
    pub fn is_none(&self) -> bool { self.bits() == 0 }
    /// Returns `true` if Return/Enter is held.
    pub fn is_return(&self) -> bool { self.intersects(Self::RETURN) }
    /// Returns `true` if Backspace is held.
    pub fn is_backspace(&self) -> bool { self.intersects(Self::BACKSPACE) }
    /// Returns `true` if Alt is held.
    pub fn is_alt(&self) -> bool { self.intersects(Self::ALT) }
    /// Returns `true` if Control is held.
    pub fn is_ctrl(&self) -> bool { self.intersects(Self::CTRL) }
    /// Returns `true` if Shift is held.
    pub fn is_shift(&self) -> bool { self.intersects(Self::SHIFT) }
}

bitflags! {
    #[derive(Copy, Clone, Debug)]
    /// Logical navigation keys handled by the UI.
    pub struct KeyCode : u32 {
        /// Right arrow key.
        const RIGHT = 8;
        /// Left arrow key.
        const LEFT = 4;
        /// Down arrow key.
        const DOWN = 2;
        /// Up arrow key.
        const UP = 1;
        /// No navigation keys pressed.
        const NONE = 0;
    }
}

impl KeyCode {
    /// Returns `true` if no navigation key is pressed.
    pub fn is_none(&self) -> bool { self.bits() == 0 }
    /// Returns `true` if up is pressed.
    pub fn is_up(&self) -> bool { self.intersects(Self::UP) }
    /// Returns `true` if down is pressed.
    pub fn is_down(&self) -> bool { self.intersects(Self::DOWN) }
    /// Returns `true` if left is pressed.
    pub fn is_left(&self) -> bool { self.intersects(Self::LEFT) }
    /// Returns `true` if right is pressed.
    pub fn is_right(&self) -> bool { self.intersects(Self::RIGHT) }
}

#[derive(Clone, Debug)]
/// Aggregates raw input collected during the current frame.
pub struct Input {
    mouse_pos: Vec2i,
    last_mouse_pos: Vec2i,
    mouse_delta: Vec2i,
    scroll_delta: Vec2i,
    rel_mouse_pos: Vec2i,
    mouse_down: MouseButton,
    mouse_pressed: MouseButton,
    key_down: KeyMode,
    key_pressed: KeyMode,
    key_code_down: KeyCode,
    key_code_pressed: KeyCode,
    input_text: String,
}

impl Default for Input {
    fn default() -> Self {
        Self {
            mouse_pos: Vec2i::default(),
            last_mouse_pos: Vec2i::default(),
            mouse_delta: Vec2i::default(),
            rel_mouse_pos: Vec2i::default(),
            scroll_delta: Vec2i::default(),
            mouse_down: MouseButton::NONE,
            mouse_pressed: MouseButton::NONE,
            key_down: KeyMode::NONE,
            key_pressed: KeyMode::NONE,
            key_code_down: KeyCode::NONE,
            key_code_pressed: KeyCode::NONE,
            input_text: String::default(),
        }
    }
}

impl Input {
    /// Returns the mouse position relative to the container that currently owns focus.
    pub fn rel_mouse_pos(&self) -> Vec2i { self.rel_mouse_pos }

    /// Returns the state of all modifier keys.
    pub fn key_state(&self) -> KeyMode { self.key_down }

    /// Returns the state of all navigation keys.
    pub fn key_codes(&self) -> KeyCode { self.key_code_down }

    /// Returns the accumulated UTF-8 text entered this frame.
    pub fn text_input(&self) -> &str { &self.input_text }

    /// Updates the current mouse pointer position.
    pub fn mousemove(&mut self, x: i32, y: i32) { self.mouse_pos = vec2(x, y); }

    /// Returns the currently held mouse buttons.
    pub fn get_mouse_buttons(&self) -> MouseButton { self.mouse_down }

    /// Records that the specified mouse button was pressed.
    pub fn mousedown(&mut self, x: i32, y: i32, btn: MouseButton) {
        self.mousemove(x, y);
        self.mouse_down |= btn;
        self.mouse_pressed |= btn;
    }

    /// Records that the specified mouse button was released.
    pub fn mouseup(&mut self, x: i32, y: i32, btn: MouseButton) {
        self.mousemove(x, y);
        self.mouse_down &= !btn;
    }

    /// Accumulates scroll wheel movement.
    pub fn scroll(&mut self, x: i32, y: i32) {
        self.scroll_delta.x += x;
        self.scroll_delta.y += y;
    }

    /// Records that a modifier key was pressed.
    pub fn keydown(&mut self, key: KeyMode) {
        self.key_pressed |= key;
        self.key_down |= key;
    }

    /// Records that a modifier key was released.
    pub fn keyup(&mut self, key: KeyMode) { self.key_down &= !key; }

    /// Records that a navigation key was pressed.
    pub fn keydown_code(&mut self, code: KeyCode) {
        self.key_code_pressed |= code;
        self.key_code_down |= code;
    }

    /// Records that a navigation key was released.
    pub fn keyup_code(&mut self, code: KeyCode) { self.key_code_down &= !code; }

    /// Appends UTF-8 text to the input buffer.
    pub fn text(&mut self, text: &str) {
        for c in text.chars() {
            self.input_text.push(c);
        }
    }

    fn prelude(&mut self) {
        self.mouse_delta.x = self.mouse_pos.x - self.last_mouse_pos.x;
        self.mouse_delta.y = self.mouse_pos.y - self.last_mouse_pos.y;
    }

    fn epilogue(&mut self) {
        self.key_pressed = KeyMode::NONE;
        self.key_code_pressed = KeyCode::NONE;
        self.input_text.clear();
        self.mouse_pressed = MouseButton::NONE;
        self.scroll_delta = vec2(0, 0);
        self.last_mouse_pos = self.mouse_pos;
    }
}

#[derive(Default, Copy, Clone)]
#[repr(C)]
/// Simple RGBA color stored with 8-bit components.
pub struct Color {
    /// Red channel.
    pub r: u8,
    /// Green channel.
    pub g: u8,
    /// Blue channel.
    pub b: u8,
    /// Alpha channel.
    pub a: u8,
}

/// Describes the interface the atlas uses to query font metadata.
pub trait Font {
    /// Returns the font's display name.
    fn name(&self) -> &str;
    /// Returns the base pixel size of the font.
    fn get_size(&self) -> usize;
    /// Returns the pixel width and height for a specific character.
    fn get_char_size(&self, c: char) -> (usize, usize);
}

#[derive(Copy, Clone)]
/// Collection of visual constants that drive widget appearance.
pub struct Style {
    /// Font used for all text rendering.
    pub font: FontId,
    /// Default width used by layouts when no size policy overrides it.
    pub default_cell_width: i32,
    /// Inner padding applied to most widgets.
    pub padding: i32,
    /// Spacing between cells in a layout.
    pub spacing: i32,
    /// Indentation applied to nested content.
    pub indent: i32,
    /// Height of window title bars.
    pub title_height: i32,
    /// Width of scrollbars.
    pub scrollbar_size: i32,
    /// Size of slider thumbs.
    pub thumb_size: i32,
    /// Palette of [`ControlColor`] entries.
    pub colors: [Color; 14],
}

/// Floating-point type used by widgets and layout calculations.
pub type Real = f32;

#[derive(Copy, Clone, Debug, PartialEq, Eq, Hash)]
/// Handle referencing a renderer-owned texture.
pub struct TextureId(u32);

impl TextureId {
    /// Returns the raw numeric identifier stored inside the handle.
    pub fn raw(self) -> u32 { self.0 }
}

#[derive(Copy, Clone, Debug, PartialEq, Eq)]
/// Either a slot stored inside the atlas or a standalone texture.
pub enum Image {
    /// Reference to an atlas slot.
    Slot(SlotId),
    /// Reference to an external texture ID.
    Texture(TextureId),
}

/// Describes image bytes that can be uploaded to a texture.
#[derive(Copy, Clone)]
pub enum ImageSource<'a> {
    /// Raw RGBA pixels laid out as width × height × 4 bytes.
    Raw {
        /// Width in pixels.
        width: i32,
        /// Height in pixels.
        height: i32,
        /// Pixel buffer in RGBA8888 format.
        pixels: &'a [u8],
    },
    #[cfg(any(feature = "builder", feature = "png_source"))]
    /// PNG-compressed byte slice (requires the `png` feature).
    Png {
        /// Compressed PNG payload.
        bytes: &'a [u8],
    },
}

static UNCLIPPED_RECT: Recti = Recti {
    x: 0,
    y: 0,
    width: i32::MAX,
    height: i32::MAX,
};

impl Default for Style {
    fn default() -> Self {
        Self {
            font: FontId::default(),
            default_cell_width: 68,
            padding: 5,
            spacing: 4,
            indent: 24,
            title_height: 24,
            scrollbar_size: 12,
            thumb_size: 8,
            colors: [
                Color { r: 230, g: 230, b: 230, a: 255 },
                Color { r: 25, g: 25, b: 25, a: 255 },
                Color { r: 50, g: 50, b: 50, a: 255 },
                Color { r: 25, g: 25, b: 25, a: 255 },
                Color { r: 240, g: 240, b: 240, a: 255 },
                Color { r: 0, g: 0, b: 0, a: 0 },
                Color { r: 75, g: 75, b: 75, a: 255 },
                Color { r: 95, g: 95, b: 95, a: 255 },
                Color { r: 115, g: 115, b: 115, a: 255 },
                Color { r: 30, g: 30, b: 30, a: 255 },
                Color { r: 35, g: 35, b: 35, a: 255 },
                Color { r: 40, g: 40, b: 40, a: 255 },
                Color { r: 43, g: 43, b: 43, a: 255 },
                Color { r: 30, g: 30, b: 30, a: 255 },
            ],
        }
    }
}

/// Convenience constructor for [`Vec2i`].
pub fn vec2(x: i32, y: i32) -> Vec2i { Vec2i { x, y } }

/// Convenience constructor for [`Recti`].
pub fn rect(x: i32, y: i32, w: i32, h: i32) -> Recti { Recti { x, y, width: w, height: h } }

/// Convenience constructor for [`Color`].
pub fn color(r: u8, g: u8, b: u8, a: u8) -> Color { Color { r, g, b, a } }

/// Expands (or shrinks) a rectangle uniformly on all sides.
pub fn expand_rect(r: Recti, n: i32) -> Recti { rect(r.x - n, r.y - n, r.width + n * 2, r.height + n * 2) }

#[derive(Clone)]
/// Shared handle to a container that can be embedded inside windows or panels.
pub struct ContainerHandle(Rc<RefCell<Container>>);

impl ContainerHandle {
    pub(crate) fn new(container: Container) -> Self { Self(Rc::new(RefCell::new(container))) }

    pub(crate) fn render<R: Renderer>(&mut self, canvas: &mut Canvas<R>) { self.0.borrow_mut().render(canvas) }

    /// Returns an immutable borrow of the underlying container.
    pub fn inner<'a>(&'a self) -> Ref<'a, Container> { self.0.borrow() }

    /// Returns a mutable borrow of the underlying container.
    pub fn inner_mut<'a>(&'a mut self) -> RefMut<'a, Container> { self.0.borrow_mut() }
}

/// Primary entry point used to drive the UI over a renderer implementation.
pub struct Context<R: Renderer> {
    canvas: Canvas<R>,
    style: Style,

    last_zindex: i32,
    frame: usize,
    hover_root: Option<WindowHandle>,
    next_hover_root: Option<WindowHandle>,
    scroll_target: Option<WindowHandle>,

    root_list: Vec<WindowHandle>,

    /// Shared pointer to the input state driving this context.
    pub input: Rc<RefCell<Input>>,
}

impl<R: Renderer> Context<R> {
    /// Creates a new UI context around the provided renderer and dimensions.
    pub fn new(renderer: RendererHandle<R>, dim: Dimensioni) -> Self {
        Self {
            canvas: Canvas::from(renderer, dim),
            style: Style::default(),
            last_zindex: 0,
            frame: 0,
            hover_root: None,
            next_hover_root: None,
            scroll_target: None,

            root_list: Vec::default(),

            input: Rc::new(RefCell::new(Input::default())),
        }
    }
}

impl<R: Renderer> Context<R> {
    /// Begins a new UI frame and prepares the canvas for drawing.
    pub fn begin(&mut self, width: i32, height: i32, clr: Color) { self.canvas.begin(width, height, clr); }

    /// Finishes the UI frame and emits all draw commands to the renderer.
    pub fn end(&mut self) {
        for r in &mut self.root_list {
            r.render(&mut self.canvas);
        }
        self.canvas.end()
    }

    /// Returns a handle to the underlying renderer.
    pub fn renderer_handle(&self) -> RendererHandle<R> { self.canvas.renderer_handle() }

    #[inline(never)]
    fn frame_begin(&mut self) {
        self.scroll_target = None;
        self.input.borrow_mut().prelude();
        for r in &mut self.root_list {
            r.prepare();
        }
        self.frame += 1;
        self.root_list.clear();
    }

    #[inline(never)]
    fn frame_end(&mut self) {
        for r in &mut self.root_list {
            r.finish();
        }

        let mouse_pressed = self.input.borrow().mouse_pressed;
        match (mouse_pressed.is_none(), &self.next_hover_root) {
            (false, Some(next_hover_root)) if next_hover_root.zindex() < self.last_zindex && next_hover_root.zindex() >= 0 => {
                self.bring_to_front(&mut next_hover_root.clone());
            }
            _ => (),
        }

        self.input.borrow_mut().epilogue();

        // prepare the next frame
        self.hover_root = self.next_hover_root.clone();
        self.next_hover_root = None;
        for r in &mut self.root_list {
            r.inner_mut().main.in_hover_root = false;
        }
        match &mut self.hover_root {
            Some(window) => window.inner_mut().main.in_hover_root = true,
            _ => (),
        }

        // sort all windows
        self.root_list.sort_by(|a, b| a.zindex().cmp(&b.zindex()));
    }

    /// Runs the UI for a single frame by wrapping begin/end calls.
    pub fn frame<F: FnOnce(&mut Self)>(&mut self, f: F) {
        self.frame_begin();

        // execute the frame function
        f(self);

        self.frame_end();
    }

    /// Creates a new movable window rooted at the provided rectangle.
    pub fn new_window(&mut self, name: &str, initial_rect: Recti) -> WindowHandle {
        let mut window = WindowHandle::window(name, self.canvas.get_atlas(), &self.style, self.input.clone(), initial_rect);
        self.bring_to_front(&mut window);
        window
    }

    /// Creates a modal dialog window.
    pub fn new_dialog(&mut self, name: &str, initial_rect: Recti) -> WindowHandle {
        WindowHandle::dialog(name, self.canvas.get_atlas(), &self.style, self.input.clone(), initial_rect)
    }

    /// Creates a popup window that appears under the mouse cursor.
    pub fn new_popup(&mut self, name: &str) -> WindowHandle { WindowHandle::popup(name, self.canvas.get_atlas(), &self.style, self.input.clone()) }

    /// Creates a standalone panel that can be embedded inside other windows.
    pub fn new_panel(&mut self, name: &str) -> ContainerHandle {
        ContainerHandle::new(Container::new(name, self.canvas.get_atlas(), &self.style, self.input.clone()))
    }

    /// Bumps the window's Z order so it renders above others.
    pub fn bring_to_front(&mut self, window: &mut WindowHandle) {
        self.last_zindex += 1;
        window.inner_mut().main.zindex = self.last_zindex;
    }

    #[inline(never)]
    fn begin_root_container(&mut self, window: &mut WindowHandle) {
        self.root_list.push(window.clone());

        if window.inner().main.rect.contains(&self.input.borrow().mouse_pos)
            && (self.next_hover_root.is_none() || window.zindex() > self.next_hover_root.as_ref().unwrap().zindex())
        {
            self.next_hover_root = Some(window.clone());
        }
        let container = &mut window.inner_mut().main;
        let scroll_delta = self.input.borrow().scroll_delta;
        let pending_scroll = if container.in_hover_root && (scroll_delta.x != 0 || scroll_delta.y != 0) {
            Some(scroll_delta)
        } else {
            None
        };
        container.seed_pending_scroll(pending_scroll);
        container.clip_stack.push(UNCLIPPED_RECT);
    }

    #[inline(never)]
    fn end_root_container(&mut self, window: &mut WindowHandle) {
        let container = &mut window.inner_mut().main;
        container.pop_clip_rect();

        let layout_body = container.layout.current_body();
        match container.layout.current_max() {
            None => (),
            Some(lm) => container.content_size = Vec2i::new(lm.x - layout_body.x, lm.y - layout_body.y),
        }
        container.consume_pending_scroll();
        container.layout.pop_scope();
    }

    #[inline(never)]
    #[must_use]
    fn begin_window(&mut self, window: &mut WindowHandle, opt: ContainerOption, bopt: WidgetBehaviourOption) -> bool {
        if !window.is_open() {
            return false;
        }

        self.begin_root_container(window);
        window.begin_window(opt, bopt);

        true
    }

    fn end_window(&mut self, window: &mut WindowHandle) {
        window.end_window();
        self.end_root_container(window);
    }

    /// Opens a window, executes the provided UI builder, and closes the window.
    pub fn window<F: FnOnce(&mut Container) -> WindowState>(
        &mut self,
        window: &mut WindowHandle,
        opt: ContainerOption,
        bopt: WidgetBehaviourOption,
        f: F,
    ) {
        // call the window function if the window is open
        if self.begin_window(window, opt, bopt) {
            window.inner_mut().main.style = self.style.clone();
            let state = f(&mut window.inner_mut().main);
            self.end_window(window);
            if window.is_open() {
                window.inner_mut().win_state = state;
            }

            // in case the window needs to be reopened, reset all states
            if !window.is_open() {
                window.inner_mut().main.reset();
            }
        }
    }

    /// Marks a dialog window as open for the next frame.
    pub fn open_dialog(&mut self, window: &mut WindowHandle) { window.inner_mut().win_state = WindowState::Open; }

    /// Renders a dialog window if it is currently open.
    pub fn dialog<F: FnOnce(&mut Container) -> WindowState>(
        &mut self,
        window: &mut WindowHandle,
        opt: ContainerOption,
        bopt: WidgetBehaviourOption,
        f: F,
    ) {
        if window.is_open() {
            self.next_hover_root = Some(window.clone());
            self.hover_root = self.next_hover_root.clone();
            window.inner_mut().main.in_hover_root = true;
            self.bring_to_front(window);

            self.window(window, opt, bopt, f);
        }
    }

    /// Shows a popup at the mouse cursor position.
    pub fn open_popup(&mut self, window: &mut WindowHandle) {
        let was_open = window.is_open();
        let mouse_pos = self.input.borrow().mouse_pos;
        {
            let mut inner = window.inner_mut();
            if was_open {
                let mut rect = inner.main.rect;
                rect.x = mouse_pos.x;
                rect.y = mouse_pos.y;
                inner.main.rect = rect;
            } else {
                inner.main.rect = rect(mouse_pos.x, mouse_pos.y, 1, 1);
                inner.win_state = WindowState::Open;
                inner.main.in_hover_root = true;
                inner.main.popup_just_opened = true;
            }
        }
        if !was_open {
            self.next_hover_root = Some(window.clone());
            self.hover_root = self.next_hover_root.clone();
            self.bring_to_front(window);
        }
    }

    /// Shows a popup anchored at the provided rectangle instead of the mouse cursor.
    pub fn open_popup_at(&mut self, window: &mut WindowHandle, anchor: Recti) {
        let was_open = window.is_open();
        {
            let mut inner = window.inner_mut();
            if was_open {
                let mut rect = inner.main.rect;
                rect.x = anchor.x;
                rect.y = anchor.y;
                rect.width = anchor.width;
                inner.main.rect = rect;
            } else {
                inner.main.rect = anchor;
                inner.win_state = WindowState::Open;
                inner.main.in_hover_root = true;
                inner.main.popup_just_opened = true;
            }
        }
        if !was_open {
            self.next_hover_root = Some(window.clone());
            self.hover_root = self.next_hover_root.clone();
            self.bring_to_front(window);
        }
    }

    /// Opens a popup window with default options.
    pub fn popup<F: FnOnce(&mut Container) -> WindowState>(
        &mut self,
        window: &mut WindowHandle,
        bopt: WidgetBehaviourOption,
        f: F,
    ) {
        let opt = ContainerOption::AUTO_SIZE | ContainerOption::NO_RESIZE | ContainerOption::NO_TITLE;
        self.window(window, opt, bopt, f);
    }

    /// Replaces the current UI style.
    pub fn set_style(&mut self, style: &Style) { self.style = style.clone() }

    /// Returns the underlying canvas used for rendering.
    pub fn canvas(&self) -> &Canvas<R> { &self.canvas }

    /// Uploads an RGBA image to the renderer and returns its [`TextureId`].
    pub fn load_image_rgba(&mut self, width: i32, height: i32, pixels: &[u8]) -> TextureId { self.canvas.load_texture_rgba(width, height, pixels) }

    /// Deletes a previously uploaded texture.
    pub fn free_image(&mut self, id: TextureId) { self.canvas.free_texture(id); }

    /// Uploads texture data described by `source`. PNG decoding is only available when the
    /// `png_source` (or `builder`) feature is enabled.
    pub fn load_image_from(&mut self, source: ImageSource) -> Result<TextureId, String> {
        match source {
            ImageSource::Raw { width, height, pixels } => {
                Self::assert_rgba_len(width, height, pixels.len())?;
                Ok(self.load_image_rgba(width, height, pixels))
            }
            #[cfg(any(feature = "builder", feature = "png_source"))]
            ImageSource::Png { bytes } => {
                let (width, height, rgba) = Self::decode_png(bytes)?;
                Ok(self.load_image_rgba(width, height, rgba.as_slice()))
            }
        }
    }

    fn assert_rgba_len(width: i32, height: i32, len: usize) -> Result<(), String> {
        if width <= 0 || height <= 0 {
            return Err(String::from("Image dimensions must be positive"));
        }
        let expected = width as usize * height as usize * 4;
        if len != expected {
            return Err(format!("Expected {} RGBA bytes, received {}", expected, len));
        }
        Ok(())
    }

    #[cfg(any(feature = "builder", feature = "png_source"))]
    fn decode_png(bytes: &[u8]) -> Result<(i32, i32, Vec<u8>), String> {
        let cursor = Cursor::new(bytes);
        let decoder = Decoder::new(cursor);
        let mut reader = decoder.read_info().map_err(|e| e.to_string())?;
        let buf_size = reader
            .output_buffer_size()
            .ok_or_else(|| "PNG decoder did not report output size".to_string())?;
        let mut buf = vec![0; buf_size];
        let info = reader.next_frame(&mut buf).map_err(|e| e.to_string())?;
        let raw = &buf[..info.buffer_size()];
        let mut rgba = Vec::with_capacity((info.width as usize) * (info.height as usize) * 4);
        match info.color_type {
            ColorType::Rgba => rgba.extend_from_slice(raw),
            ColorType::Rgb => {
                for chunk in raw.chunks(3) {
                    rgba.extend_from_slice(chunk);
                    rgba.push(0xFF);
                }
            }
            ColorType::Grayscale => {
                for &v in raw {
                    rgba.extend_from_slice(&[v, v, v, 0xFF]);
                }
            }
            ColorType::GrayscaleAlpha => {
                for chunk in raw.chunks(2) {
                    let v = chunk[0];
                    let a = chunk[1];
                    rgba.extend_from_slice(&[v, v, v, a]);
                }
            }
            _ => {
                return Err("Unsupported PNG color type".into());
            }
        }
        Ok((info.width as i32, info.height as i32, rgba))
    }
}
