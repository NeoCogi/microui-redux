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
    ops::{Deref, DerefMut},
    rc::Rc,
    sync::Arc,
};

mod atlas;
mod canvas;
mod container;
mod context;
mod draw_context;
mod file_dialog;
mod layout;
mod rect_packer;
mod scrollbar;
mod text_layout;
mod widgets;
mod window;

pub use atlas::*;
pub use canvas::*;
pub use container::*;
pub use context::Context;
pub use layout::SizePolicy;
pub use rect_packer::*;
pub use rs_math3d::*;
pub use window::*;
pub use file_dialog::*;
pub use widgets::*;

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

#[derive(Default, Copy, Clone, Eq, PartialEq, Hash, Debug)]
/// Numeric identifier value.
pub struct Id(usize);

impl Id {
    /// Creates an ID from the address of a stable object.
    pub fn from_ptr<T: ?Sized>(value: &T) -> Self { Self(value as *const T as *const () as usize) }

    /// Creates an ID from a caller-supplied numeric value.
    /// On 32-bit platforms the value is truncated to fit in a `usize`.
    pub fn new(value: u64) -> Self { Self(value as usize) }

    /// Creates a stable ID from a string label using FNV-1a hashing.
    pub fn from_str(label: &str) -> Self {
        const FNV_OFFSET_BASIS: u64 = 0xcbf29ce484222325;
        const FNV_PRIME: u64 = 0x100000001b3;
        let mut hash = FNV_OFFSET_BASIS;
        for byte in label.as_bytes() {
            hash ^= *byte as u64;
            hash = hash.wrapping_mul(FNV_PRIME);
        }
        Self::new(hash)
    }

    /// Returns the raw numeric value wrapped by this ID.
    pub fn raw(self) -> usize { self.0 }
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
        /// Reserved for future use (currently unused by the container).
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

#[derive(Copy, Clone, Default, Debug)]
/// Captures the interaction state for a widget during the current frame.
/// Produced by `Container::update_control` and passed into `Widget::handle`.
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

#[derive(Clone, Debug)]
/// Snapshot of the per-frame input state for widgets that need it.
pub struct InputSnapshot {
    /// Absolute mouse position in screen coordinates.
    pub mouse_pos: Vec2i,
    /// Mouse movement delta since the previous frame.
    pub mouse_delta: Vec2i,
    /// Currently held mouse buttons.
    pub mouse_down: MouseButton,
    /// Mouse buttons pressed this frame.
    pub mouse_pressed: MouseButton,
    /// Active modifier keys.
    pub key_mods: KeyMode,
    /// Modifier keys pressed this frame.
    pub key_pressed: KeyMode,
    /// Active navigation keys.
    pub key_codes: KeyCode,
    /// Navigation keys pressed this frame.
    pub key_code_pressed: KeyCode,
    /// UTF-8 text input collected this frame.
    pub text_input: String,
}

impl Default for InputSnapshot {
    fn default() -> Self {
        Self {
            mouse_pos: Vec2i::default(),
            mouse_delta: Vec2i::default(),
            mouse_down: MouseButton::NONE,
            mouse_pressed: MouseButton::NONE,
            key_mods: KeyMode::NONE,
            key_pressed: KeyMode::NONE,
            key_codes: KeyCode::NONE,
            key_code_pressed: KeyCode::NONE,
            text_input: String::new(),
        }
    }
}

/// Trait implemented by persistent widget state structures.
/// `handle` is invoked with a `WidgetCtx` and precomputed `ControlState`.
pub trait Widget {
    /// Returns the widget options for this state.
    fn widget_opt(&self) -> &WidgetOption;
    /// Returns the behaviour options for this state.
    fn behaviour_opt(&self) -> &WidgetBehaviourOption;
    /// Handles widget interaction and rendering for the current frame using the provided context.
    fn handle(&mut self, ctx: &mut WidgetCtx<'_>, control: &ControlState) -> ResourceState;
}

/// Raw trait-object pointer identity used for widget hover/focus tracking.
pub type WidgetId = *const dyn Widget;

/// Returns the pointer identity for a widget state object.
/// Use this when calling APIs such as `Container::set_focus`.
pub fn widget_id_of<W: Widget>(widget: &W) -> WidgetId { widget as *const W as *const dyn Widget }

impl Widget for (WidgetOption, WidgetBehaviourOption) {
    fn widget_opt(&self) -> &WidgetOption { &self.0 }
    fn behaviour_opt(&self) -> &WidgetBehaviourOption { &self.1 }
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
        /// Delete key held.
        const DELETE = 32;
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
    /// Returns `true` if Delete is held.
    pub fn is_delete(&self) -> bool { self.intersects(Self::DELETE) }
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
        /// Delete key.
        const DELETE = 32;
        /// End key.
        const END = 16;
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
    /// Returns `true` if Delete is pressed.
    pub fn is_delete(&self) -> bool { self.intersects(Self::DELETE) }
    /// Returns `true` if End is pressed.
    pub fn is_end(&self) -> bool { self.intersects(Self::END) }
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
    pub fn text(&mut self, text: &str) { self.input_text.push_str(text); }

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
    /// PNG-compressed byte slice (requires the `builder` or `png_source` feature).
    /// Grayscale and RGB images are expanded to opaque RGBA (alpha = 255).
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

/// Read-only view into a container borrowed from a handle.
pub struct ContainerView<'a> {
    inner: &'a Container,
}

impl<'a> ContainerView<'a> {
    fn new(inner: &'a Container) -> Self { Self { inner } }
}

impl<'a> Deref for ContainerView<'a> {
    type Target = Container;

    fn deref(&self) -> &Self::Target { self.inner }
}

/// Mutable view into a container borrowed from a handle.
pub struct ContainerViewMut<'a> {
    inner: &'a mut Container,
}

impl<'a> ContainerViewMut<'a> {
    fn new(inner: &'a mut Container) -> Self { Self { inner } }
}

impl<'a> Deref for ContainerViewMut<'a> {
    type Target = Container;

    fn deref(&self) -> &Self::Target { self.inner }
}

impl<'a> DerefMut for ContainerViewMut<'a> {
    fn deref_mut(&mut self) -> &mut Self::Target { self.inner }
}

impl ContainerHandle {
    pub(crate) fn new(container: Container) -> Self { Self(Rc::new(RefCell::new(container))) }

    pub(crate) fn render<R: Renderer>(&mut self, canvas: &mut Canvas<R>) { self.0.borrow_mut().render(canvas) }

    /// Returns an immutable borrow of the underlying container.
    pub(crate) fn inner<'a>(&'a self) -> Ref<'a, Container> { self.0.borrow() }

    /// Returns a mutable borrow of the underlying container.
    pub(crate) fn inner_mut<'a>(&'a mut self) -> RefMut<'a, Container> { self.0.borrow_mut() }

    /// Executes `f` with a read-only view into the container.
    pub fn with<R>(&self, f: impl FnOnce(&ContainerView<'_>) -> R) -> R {
        let container = self.0.borrow();
        let view = ContainerView::new(&container);
        f(&view)
    }

    /// Executes `f` with a mutable view into the container.
    pub fn with_mut<R>(&mut self, f: impl FnOnce(&mut ContainerViewMut<'_>) -> R) -> R {
        let mut container = self.0.borrow_mut();
        let mut view = ContainerViewMut::new(&mut container);
        f(&mut view)
    }
}
