//! Normalized input events routed through the retained UI tree.

use crate::{KeyCode, KeyMode, MouseButton, Vec2i};

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
    /// Modifier/control key state was pressed.
    KeyDown {
        /// Modifier/control key bits carried by this queued press transition.
        key: KeyMode,
    },
    /// Modifier/control key state was released.
    KeyUp {
        /// Modifier/control key bits carried by this queued release transition.
        key: KeyMode,
    },
    /// Navigation key state was pressed.
    KeyCodeDown {
        /// Navigation key bits carried by this queued press transition.
        code: KeyCode,
    },
    /// Navigation key state was released.
    KeyCodeUp {
        /// Navigation key bits carried by this queued release transition.
        code: KeyCode,
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
            Self::KeyDown { .. } | Self::KeyUp { .. } | Self::KeyCodeDown { .. } | Self::KeyCodeUp { .. } | Self::Text { .. } => None,
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
        matches!(
            self,
            Self::KeyDown { .. } | Self::KeyUp { .. } | Self::KeyCodeDown { .. } | Self::KeyCodeUp { .. } | Self::Text { .. }
        )
    }

    /// Returns whether this event ends an active pointer capture when no buttons remain held.
    pub(crate) fn is_pointer_release(&self) -> bool {
        matches!(self, Self::MouseUp { .. })
    }
}
