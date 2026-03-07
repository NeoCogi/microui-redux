//! Command definitions and callback payloads recorded during container traversal.

use super::*;

/// Arguments forwarded to custom rendering callbacks.
pub struct CustomRenderArgs {
    /// Rectangle describing the widget's content area.
    pub content_area: Rect<i32>,
    /// Final clipped region that is visible.
    pub view: Rect<i32>,
    /// Latest mouse interaction affecting the widget.
    pub mouse_event: MouseEvent,
    /// Scroll delta consumed for this widget, if any.
    pub scroll_delta: Option<Vec2i>,
    /// Options provided when the widget was created.
    pub widget_opt: WidgetOption,
    /// Behaviour options provided when the widget was created.
    pub behaviour_opt: WidgetBehaviourOption,
    /// Currently active modifier keys.
    pub key_mods: KeyMode,
    /// Currently active navigation keys.
    pub key_codes: KeyCode,
    /// Text input collected while the widget was focused.
    pub text_input: String,
}

/// Controls how text should wrap when rendered inside a container.
#[derive(Copy, Clone, Debug, PartialEq, Eq)]
pub enum TextWrap {
    /// Render text on a single line without wrapping.
    None,
    /// Wrap text at word boundaries when it exceeds the cell width.
    Word,
}

/// Draw commands recorded during container traversal.
pub(crate) enum Command {
    /// Pushes or pops a clip rectangle.
    Clip {
        /// Rect to clip against.
        rect: Recti,
    },
    /// Draws a solid rectangle.
    Recti {
        /// Target rectangle.
        rect: Recti,
        /// Fill color.
        color: Color,
    },
    /// Draws text.
    Text {
        /// Font to use.
        font: FontId,
        /// Top-left text position.
        pos: Vec2i,
        /// Text color.
        color: Color,
        /// UTF-8 string to render.
        text: String,
    },
    /// Draws an icon from the atlas.
    Icon {
        /// Target rectangle.
        rect: Recti,
        /// Icon identifier.
        id: IconId,
        /// Tint color.
        color: Color,
    },
    /// Draws an arbitrary image (slot or texture).
    Image {
        /// Target rectangle.
        rect: Recti,
        /// Image identifier.
        image: Image,
        /// Tint color.
        color: Color,
    },
    /// Re-renders a slot before drawing it.
    SlotRedraw {
        /// Target rectangle.
        rect: Recti,
        /// Slot to update.
        id: SlotId,
        /// Tint color.
        color: Color,
        /// Callback generating pixels.
        payload: Rc<dyn Fn(usize, usize) -> Color4b>,
    },
    /// Invokes a user callback for custom rendering.
    CustomRender(CustomRenderArgs, Box<dyn FnMut(Dimensioni, &CustomRenderArgs)>),
    /// Sentinel used when no command is enqueued.
    None,
}

impl Default for Command {
    fn default() -> Self {
        Command::None
    }
}
