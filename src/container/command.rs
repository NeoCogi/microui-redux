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
