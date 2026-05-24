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
//! Single-line textbox widget and shared textbox update/paint helpers.
//!
//! The textbox stores a UTF-8 byte cursor and uses shared text-edit helpers to keep cursor movement
//! and deletion on valid character boundaries.
use crate::*;
use super::WidgetConfig;

use super::text_edit::{apply_text_input, caret_rect, centered_line_top, clamp_cursor_boundary, cursor_from_text_x, font_line_metrics, ReturnBehavior};

#[derive(Clone)]
/// Persistent state for textbox widgets.
pub struct Textbox {
    /// Buffer edited by the textbox.
    buf: String,
    /// Current cursor position within the buffer (byte index).
    cursor: usize,
    /// Shared widget configuration.
    pub config: WidgetConfig,
}

impl Textbox {
    /// Creates a textbox with default widget options.
    pub fn new(buf: impl Into<String>) -> Self {
        let buf = buf.into();
        let cursor = buf.len();
        Self {
            buf,
            cursor,
            config: WidgetConfig::default(),
        }
    }

    /// Creates a textbox with explicit widget options.
    pub fn with_opt(buf: impl Into<String>, opt: WidgetOption) -> Self {
        let buf = buf.into();
        let cursor = buf.len();
        Self {
            buf,
            cursor,
            config: WidgetConfig::new(opt, ScrollBehavior::NONE),
        }
    }

    /// Returns the current text buffer.
    pub fn text(&self) -> &str {
        self.buf.as_str()
    }

    /// Replaces the current text and moves the cursor to the end.
    pub fn set_text(&mut self, text: impl Into<String>) {
        self.buf = text.into();
        self.cursor = self.buf.len();
    }

    /// Clears the current text and resets the cursor.
    pub fn clear(&mut self) {
        self.buf.clear();
        self.cursor = 0;
    }

    /// Returns the current cursor byte position.
    pub fn cursor(&self) -> usize {
        self.cursor
    }

    /// Moves the cursor to a valid UTF-8 boundary within the current text.
    pub fn set_cursor(&mut self, cursor: usize) {
        self.cursor = clamp_cursor_boundary(&self.buf, cursor);
    }

    /// Moves the cursor to the end of the current text.
    pub fn move_cursor_to_end(&mut self) {
        self.cursor = self.buf.len();
    }

    /// Measures a single-line editor, bounded by available width when supplied.
    fn preferred_size_widget(&self, style: &Style, atlas: &AtlasHandle, avail: Dimensioni) -> Dimensioni {
        let padding = style.padding.max(0);
        let vertical_pad = (padding / 2).max(1);
        let font = style.resolve_font_choice(self.config.font);
        let font_height = atlas.get_font_height(font) as i32;
        let text_w = if self.buf.is_empty() {
            0
        } else {
            atlas.get_text_size(font, self.buf.as_str()).width
        };
        let mut width = (text_w + padding * 2 + 1).max(0);
        if avail.width > 0 {
            width = width.min(avail.width.max(0));
        }
        let height = (font_height + vertical_pad * 2).max(0);
        Dimensioni::new(width, height)
    }

    /// Applies input and cursor movement for this textbox.
    fn update_widget(&mut self, ctx: &mut WidgetCtx<'_>, control: &ControlState) -> ResourceState {
        let font = ctx.style().resolve_font_choice(self.config.font);
        textbox_update(ctx, control, &mut self.buf, &mut self.cursor, self.config.opt, font)
    }

    /// Paints the textbox frame, text, and caret.
    fn paint_widget(&mut self, ctx: &mut WidgetCtx<'_>, control: &ControlState) {
        let font = ctx.style().resolve_font_choice(self.config.font);
        textbox_paint(ctx, control, self.buf.as_str(), self.cursor, self.config.opt, font);
    }
}

/// Shared single-line text editing update used by textbox and numeric inline editors.
pub(crate) fn textbox_update(
    ctx: &mut WidgetCtx<'_>,
    control: &ControlState,
    buf: &mut String,
    cursor: &mut usize,
    _opt: WidgetOption,
    font: FontId,
) -> ResourceState {
    let mut res = ResourceState::NONE;
    let r = ctx.screen_rect();
    if !control.focused {
        // Reset to end when blurred so refocusing starts from a predictable position.
        *cursor = buf.len();
    }
    let mut cursor_pos = clamp_cursor_boundary(buf, *cursor);

    let (mouse_pressed, mouse_pos, end_pressed, edit) = {
        let input = ctx.input_or_default();
        let edit = if control.focused {
            apply_text_input(buf, cursor_pos, input, false, ReturnBehavior::Submit)
        } else {
            // Without focus, the textbox ignores key/text input but keeps a consistent outcome.
            super::text_edit::TextEditOutcome {
                cursor: cursor_pos,
                changed: false,
                moved: false,
                submit: false,
            }
        };
        (input.mouse_pressed, input.mouse_pos, input.key_code_pressed.intersects(KeyCode::END), edit)
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
        // Enter submits single-line text and releases focus.
        ctx.clear_focus();
    }

    let text_metrics = ctx.atlas().get_text_size(font, buf.as_str());
    let padding = ctx.style().padding;
    let ofx = r.width - padding - text_metrics.width - 1;
    let textx = r.x + if ofx < padding { ofx } else { padding };

    if control.focused && mouse_pressed.intersects(MouseButton::LEFT) && ctx.mouse_over(r) {
        // Convert local click x into a UTF-8 boundary cursor position.
        let click_x = mouse_pos.x - (textx - r.x);
        cursor_pos = cursor_from_text_x(buf, click_x, font, ctx.atlas());
    }

    cursor_pos = clamp_cursor_boundary(buf, cursor_pos);
    *cursor = cursor_pos;
    res
}

/// Shared single-line textbox painting used by textbox and numeric inline editors.
pub(crate) fn textbox_paint(ctx: &mut WidgetCtx<'_>, control: &ControlState, buf: &str, cursor: usize, opt: WidgetOption, font: FontId) {
    let r = ctx.screen_rect();
    ctx.draw_widget_frame(control, r, ControlColor::Base, opt);

    let metrics = font_line_metrics(font, ctx.atlas());
    let texty = centered_line_top(r, metrics.line_height);
    let baseline_y = texty + metrics.baseline;

    let text_metrics = ctx.atlas().get_text_size(font, buf);
    let padding = ctx.style().padding;
    let ofx = r.width - padding - text_metrics.width - 1;
    let textx = r.x + if ofx < padding { ofx } else { padding };
    let cursor_pos = clamp_cursor_boundary(buf, cursor);
    let caret_offset = if cursor_pos == 0 {
        0
    } else {
        ctx.atlas().get_text_size(font, &buf[..cursor_pos]).width
    };

    if control.focused {
        // Focused editing path clips text/caret to the textbox bounds.
        let color = ctx.style().colors[ControlColor::Text as usize];
        ctx.push_clip_rect(r);
        ctx.draw_text(font, buf, vec2(textx, texty), color);
        ctx.draw_rect(caret_rect(textx + caret_offset, baseline_y, metrics, r), color);
        ctx.pop_clip_rect();
    } else {
        ctx.draw_control_text_with_font(font, buf, r, ControlColor::Text, opt);
    }
}

impl Widget for Textbox {
    fn widget_opt(&self) -> &WidgetOption {
        &self.config.opt
    }

    fn scroll_behavior(&self) -> ScrollBehavior {
        self.config.scroll_behavior
    }

    fn measure(&self, style: &Style, atlas: &AtlasHandle, avail: Dimensioni) -> Dimensioni {
        self.preferred_size_widget(style, atlas, avail)
    }

    fn update(&mut self, ctx: &mut WidgetCtx<'_>, control: &ControlState) -> ResourceState {
        let old_buf = self.buf.clone();
        let old_cursor = self.cursor;
        let mut res = self.update_widget(ctx, control);
        let changed = self.buf != old_buf || self.cursor != old_cursor;
        if control.focused || changed {
            res |= ResourceState::ACTIVE;
        }
        res
    }

    fn paint(&mut self, ctx: &mut WidgetCtx<'_>, control: &ControlState) {
        self.paint_widget(ctx, control);
    }

    fn effective_widget_opt(&self) -> WidgetOption {
        self.config.opt | WidgetOption::HOLD_FOCUS
    }

    fn focus_policy(&self) -> FocusPolicy {
        FocusPolicy::HoldUntilBlur
    }

    fn needs_input_snapshot(&self) -> bool {
        true
    }
}
