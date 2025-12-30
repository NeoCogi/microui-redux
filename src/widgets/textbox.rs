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
use crate::*;

use super::text_edit::{apply_text_input, ReturnBehavior};

#[derive(Clone)]
/// Persistent state for textbox widgets.
pub struct Textbox {
    /// Buffer edited by the textbox.
    pub buf: String,
    /// Current cursor position within the buffer (byte index).
    pub cursor: usize,
    /// Widget options applied to the textbox.
    pub opt: WidgetOption,
    /// Behaviour options applied to the textbox.
    pub bopt: WidgetBehaviourOption,
    id: Option<Id>,
}

impl Textbox {
    /// Creates a textbox with default widget options.
    pub fn new(buf: impl Into<String>) -> Self {
        let buf = buf.into();
        let cursor = buf.len();
        Self { buf, cursor, opt: WidgetOption::NONE, bopt: WidgetBehaviourOption::NONE, id: None }
    }

    /// Creates a textbox with explicit widget options.
    pub fn with_opt(buf: impl Into<String>, opt: WidgetOption) -> Self {
        let buf = buf.into();
        let cursor = buf.len();
        Self { buf, cursor, opt, bopt: WidgetBehaviourOption::NONE, id: None }
    }

    /// Returns a copy of the textbox with an explicit ID.
    pub fn with_id(mut self, id: Id) -> Self {
        self.id = Some(id);
        self
    }

    fn handle_widget(&mut self, ctx: &mut WidgetCtx<'_>, control: &ControlState) -> ResourceState {
        textbox_handle(ctx, control, &mut self.buf, &mut self.cursor, self.opt)
    }
}

pub(crate) fn textbox_handle(
    ctx: &mut WidgetCtx<'_>,
    control: &ControlState,
    buf: &mut String,
    cursor: &mut usize,
    opt: WidgetOption,
) -> ResourceState {
    let mut res = ResourceState::NONE;
    let r = ctx.rect();
    if !control.focused {
        *cursor = buf.len();
    }
    let mut cursor_pos = (*cursor).min(buf.len());

    let (mouse_pressed, mouse_pos, end_pressed, edit) = {
        let input = ctx.input_or_default();
        let edit = if control.focused {
            apply_text_input(buf, cursor_pos, input, false, ReturnBehavior::Submit)
        } else {
            super::text_edit::TextEditOutcome {
                cursor: cursor_pos,
                changed: false,
                moved: false,
                submit: false,
            }
        };
        (input.mouse_pressed, input.mouse_pos, input.key_code_pressed.is_end(), edit)
    };
    if control.focused {
        cursor_pos = edit.cursor;
        if edit.changed {
            res |= ResourceState::CHANGE;
        }
        if edit.submit {
            res |= ResourceState::SUBMIT;
        }
        if end_pressed {
            cursor_pos = buf.len();
        }
    }
    if edit.submit {
        ctx.clear_focus();
    }

    ctx.draw_widget_frame(control, r, ControlColor::Base, opt);

    let font = ctx.style().font;
    let line_height = ctx.atlas().get_font_height(font) as i32;
    let baseline = ctx.atlas().get_font_baseline(font);
    let descent = (line_height - baseline).max(0);

    let mut texty = r.y + r.height / 2 - line_height / 2;
    if texty < r.y {
        texty = r.y;
    }
    let max_texty = (r.y + r.height - line_height).max(r.y);
    if texty > max_texty {
        texty = max_texty;
    }
    let baseline_y = texty + line_height - descent;

    let text_metrics = ctx.atlas().get_text_size(font, buf.as_str());
    let padding = ctx.style().padding;
    let ofx = r.width - padding - text_metrics.width - 1;
    let textx = r.x + if ofx < padding { ofx } else { padding };

    if control.focused && mouse_pressed.is_left() && ctx.mouse_over(r) {
        let click_x = mouse_pos.x - textx;
        if click_x <= 0 {
            cursor_pos = 0;
        } else {
            let mut last_width = 0;
            let mut new_cursor = buf.len();
            for (idx, ch) in buf.char_indices() {
                let next = idx + ch.len_utf8();
                let width = ctx.atlas().get_text_size(font, &buf[..next]).width;
                if click_x < width {
                    if click_x < (last_width + width) / 2 {
                        new_cursor = idx;
                    } else {
                        new_cursor = next;
                    }
                    break;
                }
                last_width = width;
            }
            cursor_pos = new_cursor.min(buf.len());
        }
    }

    cursor_pos = cursor_pos.min(buf.len());
    *cursor = cursor_pos;

    let caret_offset = if cursor_pos == 0 {
        0
    } else {
        ctx.atlas().get_text_size(font, &buf[..cursor_pos]).width
    };

    if control.focused {
        let color = ctx.style().colors[ControlColor::Text as usize];
        ctx.push_clip_rect(r);
        ctx.draw_text(font, buf.as_str(), vec2(textx, texty), color);
        let caret_top = (baseline_y - baseline + 2).max(r.y).min(r.y + r.height);
        let caret_bottom = (baseline_y + descent - 2).max(r.y).min(r.y + r.height);
        let caret_height = (caret_bottom - caret_top).max(1);
        ctx.draw_rect(rect(textx + caret_offset, caret_top, 1, caret_height), color);
        ctx.pop_clip_rect();
    } else {
        ctx.draw_control_text(buf.as_str(), r, ControlColor::Text, opt);
    }
    res
}

implement_widget!(Textbox, handle_widget);
