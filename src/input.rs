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
//! Raw input state, key/button identifiers, and input-related state flags.

use std::collections::VecDeque;

use bitflags::bitflags;
use rs_math3d::Vec2i;

use crate::ui_node::UiInputEvent;

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
enum RawInputEvent {
    MouseMove { pos: Vec2i },
    MouseDown { pos: Vec2i, button: MouseButton },
    MouseUp { pos: Vec2i, button: MouseButton },
    Scroll { delta: Vec2i },
    KeyDown { key: KeyMode },
    KeyUp { key: KeyMode },
    KeyCodeDown { code: KeyCode },
    KeyCodeUp { code: KeyCode },
    Text { text: String },
}

/// Held and pointer state after one queued raw event has been applied.
#[derive(Copy, Clone, Debug)]
pub(crate) struct InputSnapshot {
    pub(crate) mouse_pos: Vec2i,
    pub(crate) mouse_buttons: MouseButton,
    pub(crate) key_modes: KeyMode,
    pub(crate) key_codes: KeyCode,
}

#[derive(Clone, Debug)]
/// Ordered raw input queue plus the state committed by events already consumed by the UI.
pub struct Input {
    /// Pointer position after the most recently consumed input event.
    pub(crate) mouse_pos: Vec2i,
    /// Mouse buttons held after the most recently consumed input event.
    pub(crate) mouse_down: MouseButton,
    /// Modifier keys held after the most recently consumed input event.
    pub(crate) key_down: KeyMode,
    /// Navigation keys held after the most recently consumed input event.
    pub(crate) key_code_down: KeyCode,
    /// Raw events waiting to be applied, in API call order.
    pending: VecDeque<RawInputEvent>,
}

impl Default for Input {
    fn default() -> Self {
        Self {
            mouse_pos: Vec2i::default(),
            mouse_down: MouseButton::NONE,
            key_down: KeyMode::NONE,
            key_code_down: KeyCode::NONE,
            pending: VecDeque::new(),
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

    /// Queues a mouse-pointer position update.
    pub fn mousemove(&mut self, x: i32, y: i32) {
        self.pending.push_back(RawInputEvent::MouseMove { pos: Vec2i::new(x, y) });
    }

    /// Returns the currently held mouse buttons.
    pub fn get_mouse_buttons(&self) -> MouseButton {
        self.mouse_down
    }

    /// Queues a mouse-button press at the supplied pointer position.
    pub fn mousedown(&mut self, x: i32, y: i32, btn: MouseButton) {
        self.pending.push_back(RawInputEvent::MouseDown { pos: Vec2i::new(x, y), button: btn });
    }

    /// Queues a mouse-button release at the supplied pointer position.
    pub fn mouseup(&mut self, x: i32, y: i32, btn: MouseButton) {
        self.pending.push_back(RawInputEvent::MouseUp { pos: Vec2i::new(x, y), button: btn });
    }

    /// Queues one scroll-wheel or trackpad transition.
    pub fn scroll(&mut self, x: i32, y: i32) {
        self.pending.push_back(RawInputEvent::Scroll { delta: Vec2i::new(x, y) });
    }

    /// Queues a modifier/control-key press.
    pub fn keydown(&mut self, key: KeyMode) {
        self.pending.push_back(RawInputEvent::KeyDown { key });
    }

    /// Queues a modifier/control-key release.
    pub fn keyup(&mut self, key: KeyMode) {
        self.pending.push_back(RawInputEvent::KeyUp { key });
    }

    /// Queues a navigation-key press.
    pub fn keydown_code(&mut self, code: KeyCode) {
        self.pending.push_back(RawInputEvent::KeyCodeDown { code });
    }

    /// Queues a navigation-key release.
    pub fn keyup_code(&mut self, code: KeyCode) {
        self.pending.push_back(RawInputEvent::KeyCodeUp { code });
    }

    /// Queues one UTF-8 text input transition.
    pub fn text(&mut self, text: &str) {
        self.pending.push_back(RawInputEvent::Text { text: text.to_owned() });
    }

    /// Returns whether at least one raw event is waiting for UI update.
    pub(crate) fn has_pending(&self) -> bool {
        !self.pending.is_empty()
    }

    /// Returns state committed by events already popped from the queue.
    pub(crate) fn snapshot(&self) -> InputSnapshot {
        InputSnapshot {
            mouse_pos: self.mouse_pos,
            mouse_buttons: self.mouse_down,
            key_modes: self.key_down,
            key_codes: self.key_code_down,
        }
    }

    /// Applies and normalizes exactly one queued raw event.
    pub(crate) fn pop_event(&mut self) -> Option<UiInputEvent> {
        let event = self.pending.pop_front()?;
        Some(match event {
            RawInputEvent::MouseMove { pos } => {
                let delta = pos - self.mouse_pos;
                self.mouse_pos = pos;
                if self.mouse_down.is_empty() {
                    UiInputEvent::MouseMove { pos, delta }
                } else {
                    UiInputEvent::MouseDrag { pos, delta, buttons: self.mouse_down }
                }
            }
            RawInputEvent::MouseDown { pos, button } => {
                self.mouse_pos = pos;
                self.mouse_down |= button;
                UiInputEvent::MouseDown { pos, button }
            }
            RawInputEvent::MouseUp { pos, button } => {
                self.mouse_pos = pos;
                self.mouse_down &= !button;
                UiInputEvent::MouseUp { pos, button }
            }
            RawInputEvent::Scroll { delta } => UiInputEvent::Scroll { pos: self.mouse_pos, delta },
            RawInputEvent::KeyDown { key } => {
                self.key_down |= key;
                UiInputEvent::KeyDown { key }
            }
            RawInputEvent::KeyUp { key } => {
                self.key_down &= !key;
                UiInputEvent::KeyUp { key }
            }
            RawInputEvent::KeyCodeDown { code } => {
                self.key_code_down |= code;
                UiInputEvent::KeyCodeDown { code }
            }
            RawInputEvent::KeyCodeUp { code } => {
                self.key_code_down &= !code;
                UiInputEvent::KeyCodeUp { code }
            }
            RawInputEvent::Text { text } => UiInputEvent::Text { text },
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn raw_calls_are_normalized_in_fifo_order_with_per_event_held_state() {
        let mut input = Input::default();
        input.mousemove(4, 5);
        input.mousedown(4, 5, MouseButton::LEFT);
        input.mousemove(9, 12);
        input.keydown(KeyMode::SHIFT);
        input.text("x");
        input.mouseup(9, 12, MouseButton::LEFT);

        assert!(matches!(
            input.pop_event(),
            Some(UiInputEvent::MouseMove { pos, delta })
                if (pos.x, pos.y, delta.x, delta.y) == (4, 5, 4, 5)
        ));
        assert!(matches!(
            input.pop_event(),
            Some(UiInputEvent::MouseDown { button, .. }) if button.bits() == MouseButton::LEFT.bits()
        ));
        assert_eq!(input.snapshot().mouse_buttons.bits(), MouseButton::LEFT.bits());
        assert!(matches!(
            input.pop_event(),
            Some(UiInputEvent::MouseDrag { pos, delta, buttons })
                if (pos.x, pos.y, delta.x, delta.y) == (9, 12, 5, 7) && buttons.bits() == MouseButton::LEFT.bits()
        ));
        assert!(matches!(
            input.pop_event(),
            Some(UiInputEvent::KeyDown { key }) if key.bits() == KeyMode::SHIFT.bits()
        ));
        assert_eq!(input.snapshot().key_modes.bits(), KeyMode::SHIFT.bits());
        assert!(matches!(input.pop_event(), Some(UiInputEvent::Text { text }) if text == "x"));
        assert!(matches!(
            input.pop_event(),
            Some(UiInputEvent::MouseUp { button, .. }) if button.bits() == MouseButton::LEFT.bits()
        ));
        assert_eq!(input.snapshot().mouse_buttons.bits(), MouseButton::NONE.bits());
        assert!(!input.has_pending());
    }

    #[test]
    fn repeated_and_zero_valued_calls_are_not_coalesced() {
        let mut input = Input::default();
        input.scroll(0, 0);
        input.scroll(0, 0);
        input.text("");

        assert!(matches!(input.pop_event(), Some(UiInputEvent::Scroll { .. })));
        assert!(matches!(input.pop_event(), Some(UiInputEvent::Scroll { .. })));
        assert!(matches!(input.pop_event(), Some(UiInputEvent::Text { text }) if text.is_empty()));
        assert!(input.pop_event().is_none());
    }
}
