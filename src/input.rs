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
    #[derive(Copy, Clone, Debug, Default, Eq, Hash, PartialEq)]
    /// Modifier state accompanying one logical keyboard transition.
    pub struct Modifiers: u8 {
        /// Either Alt/Option key is held.
        const ALT = 1;
        /// Either Control key is held.
        const CTRL = 2;
        /// Either Shift key is held.
        const SHIFT = 4;
        /// Either Windows/Command/Super key is held.
        const SUPER = 8;
        /// No modifiers are held.
        const NONE = 0;
    }
}

/// Backend-independent logical key identity.
///
/// Character keys describe the key's logical printable value and are deliberately separate from
/// [`crate::UiInputEvent::Text`]. A `Character('a')` transition can therefore participate in a
/// shortcut while composed UTF-8 text continues through the text-input channel exactly once.
#[derive(Copy, Clone, Debug, Eq, Hash, PartialEq)]
pub enum Key {
    /// Logical printable key used by accelerators and custom controls.
    Character(char),
    /// Backspace editing key.
    Backspace,
    /// Forward-delete editing key.
    Delete,
    /// Return or keypad-enter key.
    Enter,
    /// Escape or cancellation key.
    Escape,
    /// Space-bar key.
    Space,
    /// Tab traversal key.
    Tab,
    /// Insert editing key.
    Insert,
    /// Home navigation key.
    Home,
    /// End navigation key.
    End,
    /// Page-up navigation key.
    PageUp,
    /// Page-down navigation key.
    PageDown,
    /// Up-arrow navigation key.
    ArrowUp,
    /// Down-arrow navigation key.
    ArrowDown,
    /// Left-arrow navigation key.
    ArrowLeft,
    /// Right-arrow navigation key.
    ArrowRight,
    /// One numbered function key, conventionally in the inclusive range `1..=24`.
    Function(u8),
    /// Alt/Option modifier key itself, needed to distinguish an Alt tap from an Alt chord.
    Alt,
    /// Control modifier key itself.
    Control,
    /// Shift modifier key itself.
    Shift,
    /// Windows/Command/Super modifier key itself.
    Super,
}

/// Direction of one queued logical key transition.
#[derive(Copy, Clone, Debug, Eq, Hash, PartialEq)]
pub enum KeyState {
    /// Key became held.
    Pressed,
    /// Key ceased being held.
    Released,
}

/// Complete logical keyboard transition accepted from a platform backend.
#[derive(Copy, Clone, Debug, Eq, Hash, PartialEq)]
pub struct KeyEvent {
    /// Logical key whose state changed.
    pub key: Key,
    /// Whether this transition pressed or released the key.
    pub state: KeyState,
    /// Complete modifier snapshot after applying this transition.
    pub modifiers: Modifiers,
    /// Whether the platform generated this press through key-repeat.
    ///
    /// Release transitions should set this to `false`.
    pub repeat: bool,
}

impl KeyEvent {
    /// Constructs one non-repeated press with its resulting modifier snapshot.
    pub const fn pressed(key: Key, modifiers: Modifiers) -> Self {
        Self {
            key,
            state: KeyState::Pressed,
            modifiers,
            repeat: false,
        }
    }

    /// Constructs one release with its resulting modifier snapshot.
    pub const fn released(key: Key, modifiers: Modifiers) -> Self {
        Self {
            key,
            state: KeyState::Released,
            modifiers,
            repeat: false,
        }
    }

    /// Marks a press as platform-generated key repeat.
    pub const fn repeated(mut self) -> Self {
        self.repeat = true;
        self
    }

    /// Returns whether this transition presses its key.
    pub const fn is_pressed(self) -> bool {
        matches!(self.state, KeyState::Pressed)
    }
}

enum RawInputEvent {
    MouseMove { pos: Vec2i },
    MouseDown { pos: Vec2i, button: MouseButton },
    MouseUp { pos: Vec2i, button: MouseButton },
    Scroll { delta: Vec2i },
    Key { event: KeyEvent },
    Text { text: String },
}

/// Held and pointer state after one queued raw event has been applied.
#[derive(Copy, Clone, Debug)]
pub(crate) struct InputSnapshot {
    pub(crate) mouse_pos: Vec2i,
    pub(crate) mouse_buttons: MouseButton,
    /// Modifier state committed by the latest consumed key transition.
    pub(crate) modifiers: Modifiers,
}

/// Ordered raw input queue plus the state committed by events already consumed by the UI.
pub(crate) struct Input {
    /// Pointer position after the most recently consumed input event.
    mouse_pos: Vec2i,
    /// Mouse buttons held after the most recently consumed input event.
    mouse_down: MouseButton,
    /// Modifier snapshot committed by the most recently consumed keyboard transition.
    modifiers: Modifiers,
    /// Raw events waiting to be applied, in API call order.
    pending: VecDeque<RawInputEvent>,
}

impl Default for Input {
    fn default() -> Self {
        Self {
            mouse_pos: Vec2i::default(),
            mouse_down: MouseButton::NONE,
            modifiers: Modifiers::NONE,
            pending: VecDeque::new(),
        }
    }
}

impl Input {
    /// Queues a mouse-pointer position update.
    pub(crate) fn mousemove(&mut self, x: i32, y: i32) {
        self.pending.push_back(RawInputEvent::MouseMove { pos: Vec2i::new(x, y) });
    }

    /// Queues a mouse-button press at the supplied pointer position.
    pub(crate) fn mousedown(&mut self, x: i32, y: i32, btn: MouseButton) {
        self.pending.push_back(RawInputEvent::MouseDown { pos: Vec2i::new(x, y), button: btn });
    }

    /// Queues a mouse-button release at the supplied pointer position.
    pub(crate) fn mouseup(&mut self, x: i32, y: i32, btn: MouseButton) {
        self.pending.push_back(RawInputEvent::MouseUp { pos: Vec2i::new(x, y), button: btn });
    }

    /// Queues one scroll-wheel or trackpad transition.
    pub(crate) fn scroll(&mut self, x: i32, y: i32) {
        self.pending.push_back(RawInputEvent::Scroll { delta: Vec2i::new(x, y) });
    }

    /// Queues one complete logical keyboard transition.
    pub(crate) fn key(&mut self, event: KeyEvent) {
        // The event already carries the backend's authoritative modifier snapshot. Retaining one
        // transition rather than parallel modifier/navigation streams preserves exact ordering.
        self.pending.push_back(RawInputEvent::Key { event });
    }

    /// Queues one UTF-8 text input transition.
    pub(crate) fn text(&mut self, text: &str) {
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
            modifiers: self.modifiers,
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
            RawInputEvent::Key { event } => {
                self.modifiers = event.modifiers;
                UiInputEvent::Key { event }
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
        input.key(KeyEvent::pressed(Key::Shift, Modifiers::SHIFT));
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
            Some(UiInputEvent::Key { event })
                if event == KeyEvent::pressed(Key::Shift, Modifiers::SHIFT)
        ));
        assert_eq!(input.snapshot().modifiers, Modifiers::SHIFT);
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
