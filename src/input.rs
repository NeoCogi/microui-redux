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
//! Raw input state, routed pointer events, and option bitfields shared across widgets.

use bitflags::bitflags;
use rs_math3d::Vec2i;

#[derive(PartialEq, Copy, Clone)]
#[repr(u32)]
/// Identifiers for each of the built-in style colors.
pub enum ControlColor {
    /// Number of color entries in [`crate::Style::colors`].
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
    #[derive(Copy, Clone, Debug)]
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
    /// Returns `true` if the widget's data changed.
    pub fn is_changed(&self) -> bool {
        self.intersects(Self::CHANGE)
    }

    /// Returns `true` if the widget signaled submission.
    pub fn is_submitted(&self) -> bool {
        self.intersects(Self::SUBMIT)
    }

    /// Returns `true` if the widget is active.
    pub fn is_active(&self) -> bool {
        self.intersects(Self::ACTIVE)
    }

    /// Returns `true` if the state contains no flags.
    pub fn is_none(&self) -> bool {
        self.bits() == 0
    }
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
/// Scroll behavior requested by a widget or container.
pub enum ScrollBehavior {
    /// Use the default container scroll handling.
    None,
    /// Consume pending scroll when the widget is hovered.
    GrabScroll,
    /// Disable container scroll handling.
    NoScroll,
}

impl ScrollBehavior {
    /// Use the default container scroll handling.
    pub const NONE: Self = Self::None;
    /// Consume pending scroll when the widget is hovered.
    pub const GRAB_SCROLL: Self = Self::GrabScroll;
    /// Disable container scroll handling.
    pub const NO_SCROLL: Self = Self::NoScroll;

    /// Returns `true` if the option enables scroll grabbing for a widget.
    pub fn is_grab_scroll(self) -> bool {
        matches!(self, Self::GrabScroll)
    }

    /// Returns `true` if the option disables container scroll handling.
    pub fn is_no_scroll(self) -> bool {
        matches!(self, Self::NoScroll)
    }
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

#[derive(Clone, Debug)]
/// Aggregates raw input collected during the current frame.
pub struct Input {
    /// Current mouse position in screen coordinates.
    pub(crate) mouse_pos: Vec2i,
    /// Mouse position recorded at the end of the previous frame.
    pub(crate) last_mouse_pos: Vec2i,
    /// Mouse movement delta computed at frame start.
    pub(crate) mouse_delta: Vec2i,
    /// Accumulated scroll wheel/trackpad delta for the frame.
    pub(crate) scroll_delta: Vec2i,
    /// Mouse buttons currently held.
    pub(crate) mouse_down: MouseButton,
    /// Mouse buttons pressed during the current frame.
    pub(crate) mouse_pressed: MouseButton,
    /// Mouse buttons released during the current frame.
    pub(crate) mouse_released: MouseButton,
    /// Modifier keys currently held.
    pub(crate) key_down: KeyMode,
    /// Modifier keys pressed during the current frame.
    pub(crate) key_pressed: KeyMode,
    /// Modifier keys released during the current frame.
    pub(crate) key_released: KeyMode,
    /// Navigation keys currently held.
    pub(crate) key_code_down: KeyCode,
    /// Navigation keys pressed during the current frame.
    pub(crate) key_code_pressed: KeyCode,
    /// Navigation keys released during the current frame.
    pub(crate) key_code_released: KeyCode,
    /// UTF-8 text accumulated during the current frame.
    pub(crate) input_text: String,
}

impl Default for Input {
    fn default() -> Self {
        Self {
            mouse_pos: Vec2i::default(),
            last_mouse_pos: Vec2i::default(),
            mouse_delta: Vec2i::default(),
            scroll_delta: Vec2i::default(),
            mouse_down: MouseButton::NONE,
            mouse_pressed: MouseButton::NONE,
            mouse_released: MouseButton::NONE,
            key_down: KeyMode::NONE,
            key_pressed: KeyMode::NONE,
            key_released: KeyMode::NONE,
            key_code_down: KeyCode::NONE,
            key_code_pressed: KeyCode::NONE,
            key_code_released: KeyCode::NONE,
            input_text: String::default(),
        }
    }
}

impl Input {
    /// Returns the state of all modifier keys.
    pub fn key_state(&self) -> KeyMode {
        self.key_down
    }

    /// Returns the state of all navigation keys.
    pub fn key_codes(&self) -> KeyCode {
        self.key_code_down
    }

    /// Returns the accumulated UTF-8 text entered this frame.
    pub fn text_input(&self) -> &str {
        &self.input_text
    }

    /// Updates the current mouse pointer position.
    pub fn mousemove(&mut self, x: i32, y: i32) {
        self.mouse_pos = Vec2i::new(x, y);
    }

    /// Returns the currently held mouse buttons.
    pub fn get_mouse_buttons(&self) -> MouseButton {
        self.mouse_down
    }

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
        self.mouse_released |= btn;
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
    pub fn keyup(&mut self, key: KeyMode) {
        self.key_down &= !key;
        self.key_released |= key;
    }

    /// Records that a navigation key was pressed.
    pub fn keydown_code(&mut self, code: KeyCode) {
        self.key_code_pressed |= code;
        self.key_code_down |= code;
    }

    /// Records that a navigation key was released.
    pub fn keyup_code(&mut self, code: KeyCode) {
        self.key_code_down &= !code;
        self.key_code_released |= code;
    }

    /// Appends UTF-8 text to the input buffer.
    pub fn text(&mut self, text: &str) {
        self.input_text.push_str(text);
    }

    /// Computes per-frame derived input before UI traversal starts.
    pub(crate) fn prelude(&mut self) {
        self.mouse_delta.x = self.mouse_pos.x - self.last_mouse_pos.x;
        self.mouse_delta.y = self.mouse_pos.y - self.last_mouse_pos.y;
    }

    /// Clears one-frame input fields after UI traversal finishes.
    pub(crate) fn epilogue(&mut self) {
        self.key_pressed = KeyMode::NONE;
        self.key_released = KeyMode::NONE;
        self.key_code_pressed = KeyCode::NONE;
        self.key_code_released = KeyCode::NONE;
        self.input_text.clear();
        self.mouse_pressed = MouseButton::NONE;
        self.mouse_released = MouseButton::NONE;
        self.scroll_delta = Vec2i::new(0, 0);
        self.last_mouse_pos = self.mouse_pos;
    }
}
