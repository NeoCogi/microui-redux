//
// Copyright 2023-Present (c) Raja Lehtihet & Wael El Oraiby
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

//! Normalized input events routed through the retained UI tree.

use crate::{KeyEvent, MouseButton, Vec2i};

/// Input event routed to one retained widget or container.
#[derive(Clone, Debug)]
pub enum UiInputEvent {
    /// Pointer moved without any mouse button held.
    MouseMove {
        /// Current pointer position in the receiver's routed local coordinate space.
        pos: Vec2i,
        /// Pointer movement since the previous queued pointer-position event.
        delta: Vec2i,
    },
    /// Pointer moved while one or more mouse buttons are held.
    MouseDrag {
        /// Current pointer position in the receiver's routed local coordinate space.
        pos: Vec2i,
        /// Pointer movement since the previous queued pointer-position event.
        delta: Vec2i,
        /// Mouse buttons held during the drag.
        buttons: MouseButton,
    },
    /// One or more mouse buttons were pressed.
    MouseDown {
        /// Current pointer position in the receiver's routed local coordinate space.
        pos: Vec2i,
        /// Buttons carried by this queued press transition.
        button: MouseButton,
    },
    /// One or more mouse buttons were released.
    MouseUp {
        /// Current pointer position in the receiver's routed local coordinate space.
        pos: Vec2i,
        /// Buttons carried by this queued release transition.
        button: MouseButton,
    },
    /// Scroll wheel or equivalent high-level scroll input.
    Scroll {
        /// Pointer position in the receiver's routed local coordinate space.
        pos: Vec2i,
        /// Requested scroll delta.
        delta: Vec2i,
    },
    /// One logical keyboard press or release with its complete modifier snapshot.
    Key {
        /// Backend-normalized logical transition.
        event: KeyEvent,
    },
    /// One queued UTF-8 text input transition.
    Text {
        /// Entered text.
        text: String,
    },
}

impl UiInputEvent {
    /// Returns the pointer position carried by this event, when it belongs to pointer routing.
    pub(crate) fn position(&self) -> Option<Vec2i> {
        match self {
            Self::MouseMove { pos, .. } | Self::MouseDrag { pos, .. } | Self::MouseDown { pos, .. } | Self::MouseUp { pos, .. } | Self::Scroll { pos, .. } => {
                Some(*pos)
            }
            Self::Key { .. } | Self::Text { .. } => None,
        }
    }

    /// Returns whether this event belongs to pointer routing.
    pub(crate) fn is_pointer(&self) -> bool {
        matches!(
            self,
            Self::MouseMove { .. } | Self::MouseDrag { .. } | Self::MouseDown { .. } | Self::MouseUp { .. } | Self::Scroll { .. }
        )
    }

    /// Returns whether this event should be delivered to the focused node.
    pub(crate) fn is_focus_input(&self) -> bool {
        matches!(self, Self::Key { .. } | Self::Text { .. })
    }

    /// Returns whether this event ends an active pointer capture when no buttons remain held.
    pub(crate) fn is_pointer_release(&self) -> bool {
        matches!(self, Self::MouseUp { .. })
    }
}
