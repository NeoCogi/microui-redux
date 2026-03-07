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
//! Input snapshots, control flags, and option bitfields shared across widgets.

use bitflags::bitflags;
use rs_math3d::Vec2i;

/// Tracks input button transitions seen since the previous frame.
#[derive(Debug, Copy, Clone)]
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
    pub fn is_grab_scroll(self) -> bool {
        matches!(self, Self::GrabScroll)
    }

    /// Returns `true` if the option disables container scroll handling.
    pub fn is_no_scroll(self) -> bool {
        matches!(self, Self::NoScroll)
    }
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

#[derive(Clone, Debug)]
/// Snapshot of the per-frame input state for widgets that need it.
pub struct InputSnapshot {
    /// Mouse position relative to the current widget rectangle.
    pub mouse_pos: Vec2i,
    /// Mouse movement delta since the previous frame, expressed in widget-local space.
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

impl ContainerOption {
    /// Returns `true` if the option requests automatic sizing.
    pub fn is_auto_sizing(&self) -> bool {
        self.intersects(Self::AUTO_SIZE)
    }

    /// Returns `true` if the title bar should be hidden.
    pub fn has_no_title(&self) -> bool {
        self.intersects(Self::NO_TITLE)
    }

    /// Returns `true` if the close button should be hidden.
    pub fn has_no_close(&self) -> bool {
        self.intersects(Self::NO_CLOSE)
    }

    /// Returns `true` if the container is fixed-size.
    pub fn is_fixed(&self) -> bool {
        self.intersects(Self::NO_RESIZE)
    }

    /// Returns `true` if the outer frame is hidden.
    pub fn has_no_frame(&self) -> bool {
        self.intersects(Self::NO_FRAME)
    }
}

impl WidgetOption {
    /// Returns `true` if the widget should keep focus while held.
    pub fn is_holding_focus(&self) -> bool {
        self.intersects(WidgetOption::HOLD_FOCUS)
    }

    /// Returns `true` if the widget should not draw its frame.
    pub fn has_no_frame(&self) -> bool {
        self.intersects(WidgetOption::NO_FRAME)
    }

    /// Returns `true` if the widget is non-interactive.
    pub fn is_not_interactive(&self) -> bool {
        self.intersects(WidgetOption::NO_INTERACT)
    }

    /// Returns `true` if the widget prefers right alignment.
    pub fn is_aligned_right(&self) -> bool {
        self.intersects(WidgetOption::ALIGN_RIGHT)
    }

    /// Returns `true` if the widget prefers centered alignment.
    pub fn is_aligned_center(&self) -> bool {
        self.intersects(WidgetOption::ALIGN_CENTER)
    }

    /// Returns `true` if the option set is empty.
    pub fn is_none(&self) -> bool {
        self.bits() == 0
    }
}

impl WidgetFillOption {
    /// Returns `true` when the normal state should be filled.
    pub fn fill_normal(&self) -> bool {
        self.intersects(Self::NORMAL)
    }

    /// Returns `true` when the hover state should be filled.
    pub fn fill_hover(&self) -> bool {
        self.intersects(Self::HOVER)
    }

    /// Returns `true` when the clicked state should be filled.
    pub fn fill_click(&self) -> bool {
        self.intersects(Self::CLICK)
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

impl MouseButton {
    /// Returns `true` if the middle mouse button is pressed.
    pub fn is_middle(&self) -> bool {
        self.intersects(Self::MIDDLE)
    }

    /// Returns `true` if the right mouse button is pressed.
    pub fn is_right(&self) -> bool {
        self.intersects(Self::RIGHT)
    }

    /// Returns `true` if the left mouse button is pressed.
    pub fn is_left(&self) -> bool {
        self.intersects(Self::LEFT)
    }

    /// Returns `true` if no mouse buttons are pressed.
    pub fn is_none(&self) -> bool {
        self.bits() == 0
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

impl KeyMode {
    /// Returns `true` if no modifiers are active.
    pub fn is_none(&self) -> bool {
        self.bits() == 0
    }

    /// Returns `true` if Delete is held.
    pub fn is_delete(&self) -> bool {
        self.intersects(Self::DELETE)
    }

    /// Returns `true` if Return/Enter is held.
    pub fn is_return(&self) -> bool {
        self.intersects(Self::RETURN)
    }

    /// Returns `true` if Backspace is held.
    pub fn is_backspace(&self) -> bool {
        self.intersects(Self::BACKSPACE)
    }

    /// Returns `true` if Alt is held.
    pub fn is_alt(&self) -> bool {
        self.intersects(Self::ALT)
    }

    /// Returns `true` if Control is held.
    pub fn is_ctrl(&self) -> bool {
        self.intersects(Self::CTRL)
    }

    /// Returns `true` if Shift is held.
    pub fn is_shift(&self) -> bool {
        self.intersects(Self::SHIFT)
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

impl KeyCode {
    /// Returns `true` if no navigation key is pressed.
    pub fn is_none(&self) -> bool {
        self.bits() == 0
    }

    /// Returns `true` if Delete is pressed.
    pub fn is_delete(&self) -> bool {
        self.intersects(Self::DELETE)
    }

    /// Returns `true` if End is pressed.
    pub fn is_end(&self) -> bool {
        self.intersects(Self::END)
    }

    /// Returns `true` if up is pressed.
    pub fn is_up(&self) -> bool {
        self.intersects(Self::UP)
    }

    /// Returns `true` if down is pressed.
    pub fn is_down(&self) -> bool {
        self.intersects(Self::DOWN)
    }

    /// Returns `true` if left is pressed.
    pub fn is_left(&self) -> bool {
        self.intersects(Self::LEFT)
    }

    /// Returns `true` if right is pressed.
    pub fn is_right(&self) -> bool {
        self.intersects(Self::RIGHT)
    }
}

#[derive(Clone, Debug)]
/// Aggregates raw input collected during the current frame.
pub struct Input {
    pub(crate) mouse_pos: Vec2i,
    pub(crate) last_mouse_pos: Vec2i,
    pub(crate) mouse_delta: Vec2i,
    pub(crate) scroll_delta: Vec2i,
    pub(crate) rel_mouse_pos: Vec2i,
    pub(crate) mouse_down: MouseButton,
    pub(crate) mouse_pressed: MouseButton,
    pub(crate) key_down: KeyMode,
    pub(crate) key_pressed: KeyMode,
    pub(crate) key_code_down: KeyCode,
    pub(crate) key_code_pressed: KeyCode,
    pub(crate) input_text: String,
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
    pub fn rel_mouse_pos(&self) -> Vec2i {
        self.rel_mouse_pos
    }

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
    }

    /// Records that a navigation key was pressed.
    pub fn keydown_code(&mut self, code: KeyCode) {
        self.key_code_pressed |= code;
        self.key_code_down |= code;
    }

    /// Records that a navigation key was released.
    pub fn keyup_code(&mut self, code: KeyCode) {
        self.key_code_down &= !code;
    }

    /// Appends UTF-8 text to the input buffer.
    pub fn text(&mut self, text: &str) {
        self.input_text.push_str(text);
    }

    pub(crate) fn prelude(&mut self) {
        self.mouse_delta.x = self.mouse_pos.x - self.last_mouse_pos.x;
        self.mouse_delta.y = self.mouse_pos.y - self.last_mouse_pos.y;
    }

    pub(crate) fn epilogue(&mut self) {
        self.key_pressed = KeyMode::NONE;
        self.key_code_pressed = KeyCode::NONE;
        self.input_text.clear();
        self.mouse_pressed = MouseButton::NONE;
        self.scroll_delta = Vec2i::new(0, 0);
        self.last_mouse_pos = self.mouse_pos;
    }
}
